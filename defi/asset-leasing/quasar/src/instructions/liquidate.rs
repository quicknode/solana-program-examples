use {
    crate::{
        constants::{
            BASIS_POINTS_DENOMINATOR, COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED,
            PYTH_MAX_AGE_SECONDS,
        },
        errors::AssetLeasingError,
        instructions::pay_lease_fee::compute_lease_fee_due,
        state::{Lease, LeaseStatus},
    },
    quasar_lang::prelude::*,
    quasar_spl::{Mint, Token, TokenCpi},
};

/// Pyth Solana Receiver program id on mainnet/devnet. Liquidation
/// rejects any `price_update` account not owned by this program.
// Base58: rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ
pub const PYTH_RECEIVER_PROGRAM_ID: Address = Address::new_from_array([
    12, 183, 250, 187, 82, 247, 166, 72, 187, 91, 49, 125, 154, 1, 139, 144, 87, 203, 2, 71, 116,
    250, 254, 1, 230, 196, 223, 152, 204, 56, 88, 129,
]);

/// 8-byte Anchor discriminator for `PriceUpdateV2`. Equal to the first
/// 8 bytes of `sha256("account:PriceUpdateV2")`. Hard-coded because the
/// Pyth SDK pulls in a large dependency tree we don't need for the two
/// numeric fields we actually read.
pub const PRICE_UPDATE_V2_DISCRIMINATOR: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];

/// Accounts for the keeper-driven liquidation of an underwater lease.
#[derive(Accounts)]
pub struct Liquidate<'info> {
    #[account(mut)]
    pub keeper: &'info Signer,

    /// Receives the lease fee + the post-bounty remainder. Also the destination
    /// for the closed-vault rent-exempt lamports.
    #[account(mut)]
    pub lessor: &'info UncheckedAccount,

    #[account(
        mut,
        seeds = [LEASE_SEED, lessor],
        bump = lease.bump,
        has_one = lessor,
        has_one = leased_mint,
        has_one = collateral_mint,
        constraint = LeaseStatus::from_u8(lease.status) == Some(LeaseStatus::Active)
            @ AssetLeasingError::InvalidLeaseStatus,
        close = lessor,
    )]
    pub lease: &'info mut Account<Lease>,

    pub leased_mint: &'info Account<Mint>,
    pub collateral_mint: &'info Account<Mint>,

    #[account(
        mut,
        seeds = [LEASED_VAULT_SEED, lease],
        bump = lease.leased_vault_bump,
    )]
    pub leased_vault: &'info mut Account<Token>,

    #[account(
        mut,
        seeds = [COLLATERAL_VAULT_SEED, lease],
        bump = lease.collateral_vault_bump,
    )]
    pub collateral_vault: &'info mut Account<Token>,

    /// Lessor's collateral-mint token account. Pre-created by the caller.
    #[account(mut)]
    pub lessor_collateral_account: &'info mut Account<Token>,

    /// Keeper's collateral-mint token account — bounty destination.
    /// Pre-created by the caller.
    #[account(mut)]
    pub keeper_collateral_account: &'info mut Account<Token>,

    /// Pyth `PriceUpdateV2` account. Must be owned by the Pyth receiver
    /// program and carry the expected discriminator; the `feed_id`
    /// inside must match the one pinned on the `Lease` at creation so a
    /// keeper cannot swap in an unrelated feed.
    pub price_update: &'info UncheckedAccount,

    pub token_program: &'info Program<Token>,
}

/// Minimal projection of `PriceUpdateV2` — only the fields we read.
/// Layout: `[discriminator(8) | write_authority(32) | verification_level(1)
/// | feed_id(32) | price(i64) | conf(u64) | exponent(i32) |
/// publish_time(i64) | ...]`.
pub struct DecodedPriceUpdate {
    pub feed_id: [u8; 32],
    pub price: i64,
    pub exponent: i32,
    pub publish_time: i64,
}

pub fn decode_price_update(data: &[u8]) -> Result<DecodedPriceUpdate, ProgramError> {
    // Discriminator (8) + write_authority (32) + verification_level (1) = 41.
    const FEED_ID_OFFSET: usize = 41;
    const PRICE_OFFSET: usize = FEED_ID_OFFSET + 32;
    const EXPONENT_OFFSET: usize = PRICE_OFFSET + 8 /* price */ + 8 /* conf */;
    const PUBLISH_TIME_OFFSET: usize = EXPONENT_OFFSET + 4 /* exponent */;
    const MIN_LEN: usize = PUBLISH_TIME_OFFSET + 8;

    if data.len() < MIN_LEN {
        return Err(AssetLeasingError::StalePrice.into());
    }
    if data[..8] != PRICE_UPDATE_V2_DISCRIMINATOR {
        return Err(AssetLeasingError::StalePrice.into());
    }

    let mut feed_id = [0u8; 32];
    feed_id.copy_from_slice(&data[FEED_ID_OFFSET..FEED_ID_OFFSET + 32]);

    let price = i64::from_le_bytes(data[PRICE_OFFSET..PRICE_OFFSET + 8].try_into().unwrap());
    let exponent = i32::from_le_bytes(
        data[EXPONENT_OFFSET..EXPONENT_OFFSET + 4]
            .try_into()
            .unwrap(),
    );
    let publish_time = i64::from_le_bytes(
        data[PUBLISH_TIME_OFFSET..PUBLISH_TIME_OFFSET + 8]
            .try_into()
            .unwrap(),
    );

    Ok(DecodedPriceUpdate {
        feed_id,
        price,
        exponent,
        publish_time,
    })
}

#[inline(always)]
pub fn handle_liquidate(accounts: &mut Liquidate) -> Result<(), ProgramError> {
    // Owner check: the price update must come from the Pyth receiver
    // program. Without this a keeper could forge an arbitrary account.
    let price_view = accounts.price_update.to_account_view();
    if price_view.owner() != &PYTH_RECEIVER_PROGRAM_ID {
        return Err(ProgramError::IllegalOwner);
    }

    let now = <Clock as quasar_lang::sysvars::Sysvar>::get()?.unix_timestamp.get();
    let decoded = {
        let price_data = unsafe { price_view.borrow_unchecked() };
        decode_price_update(price_data)?
    };

    // Feed pinning: reject any `PriceUpdateV2` whose feed_id does not
    // match the one the lessor committed to at `create_lease`. Without
    // this guard, a keeper could pass in any feed the Pyth receiver
    // owns — e.g. an unrelated volatile pair that happens to dip —
    // and trigger a spurious liquidation.
    if decoded.feed_id != accounts.lease.feed_id {
        return Err(AssetLeasingError::PriceFeedMismatch.into());
    }

    if !is_underwater(accounts.lease, &decoded, now)? {
        return Err(AssetLeasingError::PositionHealthy.into());
    }

    // Settle accrued lease fees first (up to end_timestamp) so the lessor is paid for
    // the time the lessee actually used. Only then slice off bounty +
    // remainder.
    let lease_fee_due = compute_lease_fee_due(accounts.lease, now)?;
    let collateral_amount = accounts.lease.collateral_amount.get();
    let lease_fee_payable = lease_fee_due.min(collateral_amount);

    let lease_address = *accounts.lease.address();
    let collateral_vault_bump = [accounts.lease.collateral_vault_bump];
    let collateral_vault_seeds: &[Seed] = &[
        Seed::from(COLLATERAL_VAULT_SEED),
        Seed::from(lease_address.as_ref()),
        Seed::from(&collateral_vault_bump as &[u8]),
    ];
    let leased_vault_bump = [accounts.lease.leased_vault_bump];
    let leased_vault_seeds: &[Seed] = &[
        Seed::from(LEASED_VAULT_SEED),
        Seed::from(lease_address.as_ref()),
        Seed::from(&leased_vault_bump as &[u8]),
    ];

    if lease_fee_payable > 0 {
        accounts
            .token_program
            .transfer(
                accounts.collateral_vault,
                accounts.lessor_collateral_account,
                accounts.collateral_vault,
                lease_fee_payable,
            )
            .invoke_signed(collateral_vault_seeds)?;
    }

    let remaining = collateral_amount
        .checked_sub(lease_fee_payable)
        .ok_or(AssetLeasingError::MathOverflow)?;

    // Bounty is a percentage of the collateral *after* lease fees — guarantees
    // we never try to pay out more than what actually sits in the vault.
    let bounty = (remaining as u128)
        .checked_mul(accounts.lease.liquidation_bounty_basis_points.get() as u128)
        .ok_or(AssetLeasingError::MathOverflow)?
        .checked_div(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or(AssetLeasingError::MathOverflow)? as u64;

    if bounty > 0 {
        accounts
            .token_program
            .transfer(
                accounts.collateral_vault,
                accounts.keeper_collateral_account,
                accounts.collateral_vault,
                bounty,
            )
            .invoke_signed(collateral_vault_seeds)?;
    }

    let lessor_share = remaining
        .checked_sub(bounty)
        .ok_or(AssetLeasingError::MathOverflow)?;
    if lessor_share > 0 {
        accounts
            .token_program
            .transfer(
                accounts.collateral_vault,
                accounts.lessor_collateral_account,
                accounts.collateral_vault,
                lessor_share,
            )
            .invoke_signed(collateral_vault_seeds)?;
    }

    // Close both vaults. The leased vault is empty on the default path
    // (lessee kept the tokens) but was rent-exempt funded at creation,
    // so closing it still returns lamports to the lessor.
    accounts
        .token_program
        .close_account(
            accounts.leased_vault,
            accounts.lessor,
            accounts.leased_vault,
        )
        .invoke_signed(leased_vault_seeds)?;
    accounts
        .token_program
        .close_account(
            accounts.collateral_vault,
            accounts.lessor,
            accounts.collateral_vault,
        )
        .invoke_signed(collateral_vault_seeds)?;

    accounts.lease.collateral_amount = 0u64.into();
    let end_timestamp = accounts.lease.end_timestamp.get();
    accounts.lease.last_paid_timestamp = now.min(end_timestamp).into();
    accounts.lease.status = LeaseStatus::Liquidated as u8;

    Ok(())
}

/// Liquidatable when collateral value < debt value * maintenance margin.
/// All math stays in integers by folding the Pyth exponent into whichever
/// side of the inequality does not already have a power of ten applied.
pub fn is_underwater(
    lease: &Lease,
    price: &DecodedPriceUpdate,
    now: i64,
) -> Result<bool, ProgramError> {
    // Staleness guard. `publish_time` coming from the future is treated
    // as stale — the keeper must not front-run the clock.
    if price.publish_time > now {
        return Err(AssetLeasingError::StalePrice.into());
    }
    let age = (now - price.publish_time) as u64;
    if age > PYTH_MAX_AGE_SECONDS {
        return Err(AssetLeasingError::StalePrice.into());
    }

    if price.price <= 0 {
        return Err(AssetLeasingError::NonPositivePrice.into());
    }
    let price_raw = price.price as u128;

    let leased_amount = lease.leased_amount.get() as u128;
    let collateral_amount = lease.collateral_amount.get() as u128;
    let margin_basis_points = lease.maintenance_margin_basis_points.get() as u128;
    let denom = BASIS_POINTS_DENOMINATOR as u128;

    let (collateral_scaled, debt_scaled) = if price.exponent >= 0 {
        let scale = ten_pow(price.exponent as u32)?;
        let debt = leased_amount
            .checked_mul(price_raw)
            .and_then(|product| product.checked_mul(scale))
            .ok_or(AssetLeasingError::MathOverflow)?;
        (collateral_amount, debt)
    } else {
        let scale = ten_pow((-price.exponent) as u32)?;
        let collateral = collateral_amount
            .checked_mul(scale)
            .ok_or(AssetLeasingError::MathOverflow)?;
        let debt = leased_amount
            .checked_mul(price_raw)
            .ok_or(AssetLeasingError::MathOverflow)?;
        (collateral, debt)
    };

    let lhs = collateral_scaled
        .checked_mul(denom)
        .ok_or(AssetLeasingError::MathOverflow)?;
    let rhs = debt_scaled
        .checked_mul(margin_basis_points)
        .ok_or(AssetLeasingError::MathOverflow)?;

    Ok(lhs < rhs)
}

fn ten_pow(exponent: u32) -> Result<u128, ProgramError> {
    10u128
        .checked_pow(exponent)
        .ok_or_else(|| AssetLeasingError::MathOverflow.into())
}
