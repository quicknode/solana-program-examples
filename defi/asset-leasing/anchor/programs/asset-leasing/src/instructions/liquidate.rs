use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{
        BASIS_POINTS_DENOMINATOR, COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED,
        PYTH_MAX_AGE_SECONDS,
    },
    errors::AssetLeasingError,
    instructions::{
        pay_lease_fee::compute_lease_fee_due,
        shared::{close_vault, transfer_tokens_from_vault},
    },
    state::{Lease, LeaseStatus},
};

/// Pyth Solana Receiver program ID on mainnet (also used on devnet by the
/// canonical Pyth integrations). Declared here as a string so the tests can
/// mint mock `PriceUpdateV2` accounts owned by the same id.
pub const PYTH_RECEIVER_PROGRAM_ID: Pubkey =
    anchor_lang::pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

/// Anchor discriminator for `PriceUpdateV2`. Equal to the first 8 bytes of
/// `sha256("account:PriceUpdateV2")`. Hard-coded because we parse the account
/// by hand rather than pulling in `pyth-solana-receiver-sdk` (see Cargo.toml).
pub const PRICE_UPDATE_V2_DISCRIMINATOR: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];

#[derive(Accounts)]
pub struct Liquidate<'info> {
    /// Keeper who calls the instruction — they receive the bounty.
    #[account(mut)]
    pub keeper: Signer<'info>,

    /// CHECK: program-derived address seed + lease-fee / collateral destination.
    #[account(mut)]
    pub holder: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [LEASE_SEED, holder.key().as_ref(), &lease.lease_id.to_le_bytes()],
        bump = lease.bump,
        has_one = holder,
        has_one = leased_mint,
        has_one = collateral_mint,
        constraint = lease.status == LeaseStatus::Active @ AssetLeasingError::InvalidLeaseStatus,
        close = holder,
    )]
    pub lease: Account<'info, Lease>,

    pub leased_mint: Box<InterfaceAccount<'info, Mint>>,
    pub collateral_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        seeds = [LEASED_VAULT_SEED, lease.key().as_ref()],
        bump = lease.leased_vault_bump,
        token::mint = leased_mint,
        token::authority = leased_vault,
        token::token_program = token_program,
    )]
    pub leased_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [COLLATERAL_VAULT_SEED, lease.key().as_ref()],
        bump = lease.collateral_vault_bump,
        token::mint = collateral_mint,
        token::authority = collateral_vault,
        token::token_program = token_program,
    )]
    pub collateral_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = keeper,
        associated_token::mint = collateral_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_collateral_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = keeper,
        associated_token::mint = collateral_mint,
        associated_token::authority = keeper,
        associated_token::token_program = token_program,
    )]
    pub keeper_collateral_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: We verify the account is owned by the Pyth Receiver program and
    /// carries the expected `PriceUpdateV2` discriminator before decoding.
    /// The price feed must quote *one leased token in collateral units* —
    /// keepers are responsible for supplying an appropriate feed, the program
    /// cannot know which pair is correct for a given lease.
    #[account(owner = PYTH_RECEIVER_PROGRAM_ID)]
    pub price_update: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Minimal projection of `PriceUpdateV2` — only the fields we actually need.
/// Layout: `[discriminator(8) | write_authority(32) | verification_level(1) |
/// feed_id(32) | price(i64) | conf(u64) | exponent(i32) | publish_time(i64) |
/// prev_publish_time(i64) | ema_price(i64) | ema_conf(u64) | posted_slot(u64)]`.
pub struct DecodedPriceUpdate {
    pub feed_id: [u8; 32],
    pub price: i64,
    pub exponent: i32,
    pub publish_time: i64,
}

pub fn decode_price_update(data: &[u8]) -> Result<DecodedPriceUpdate> {
    // Discriminator (8) + write_authority (32) + verification_level (1) = 41.
    const FEED_ID_OFFSET: usize = 41;
    // feed_id (32) starts at 41, price i64 at 41 + 32 = 73.
    const PRICE_OFFSET: usize = FEED_ID_OFFSET + 32;
    const EXPONENT_OFFSET: usize = PRICE_OFFSET + 8 + 8; // price + conf
    const PUBLISH_TIME_OFFSET: usize = EXPONENT_OFFSET + 4; // exponent
    const MIN_LEN: usize = PUBLISH_TIME_OFFSET + 8;

    require!(data.len() >= MIN_LEN, AssetLeasingError::StalePrice);
    require!(
        data[..8] == PRICE_UPDATE_V2_DISCRIMINATOR,
        AssetLeasingError::StalePrice
    );

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

pub fn handle_liquidate(context: Context<Liquidate>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let price_data = context.accounts.price_update.try_borrow_data()?;
    let decoded = decode_price_update(&price_data)?;
    drop(price_data);

    // Feed pinning: reject any `PriceUpdateV2` whose feed_id does not match
    // the one the holder committed to at `create_lease`. Without this guard,
    // a keeper could pass in any feed the Pyth Receiver program owns — e.g.
    // a wildly volatile pair that dips enough to flag the position as
    // underwater — and trigger a spurious liquidation.
    require!(
        decoded.feed_id == context.accounts.lease.feed_id,
        AssetLeasingError::PriceFeedMismatch
    );

    require!(
        is_underwater(&context.accounts.lease, &decoded, now)?,
        AssetLeasingError::PositionHealthy
    );

    // Settle accrued lease fees first (up to end_timestamp) so the holder is paid for the
    // time the short_seller actually used. Only then slice off bounty + remainder.
    let lease_fee_due = compute_lease_fee_due(&context.accounts.lease, now)?;
    let lease_fee_payable = lease_fee_due.min(context.accounts.lease.collateral_amount);

    let lease_key = context.accounts.lease.key();
    let collateral_vault_bump = context.accounts.lease.collateral_vault_bump;
    let collateral_vault_seeds: &[&[u8]] = &[
        COLLATERAL_VAULT_SEED,
        lease_key.as_ref(),
        core::slice::from_ref(&collateral_vault_bump),
    ];
    let leased_vault_bump = context.accounts.lease.leased_vault_bump;
    let leased_vault_seeds: &[&[u8]] = &[
        LEASED_VAULT_SEED,
        lease_key.as_ref(),
        core::slice::from_ref(&leased_vault_bump),
    ];

    if lease_fee_payable > 0 {
        transfer_tokens_from_vault(
            &context.accounts.collateral_vault,
            &context.accounts.holder_collateral_account,
            lease_fee_payable,
            &context.accounts.collateral_mint,
            &context.accounts.collateral_vault.to_account_info(),
            &context.accounts.token_program,
            &[collateral_vault_seeds],
        )?;
    }

    let remaining = context
        .accounts
        .lease
        .collateral_amount
        .checked_sub(lease_fee_payable)
        .ok_or(AssetLeasingError::MathOverflow)?;

    // Bounty is a percentage of the collateral *after* lease fees — guarantees we
    // never try to pay out more than what actually sits in the vault.
    let bounty = (remaining as u128)
        .checked_mul(context.accounts.lease.liquidation_bounty_basis_points as u128)
        .ok_or(AssetLeasingError::MathOverflow)?
        .checked_div(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or(AssetLeasingError::MathOverflow)? as u64;

    if bounty > 0 {
        transfer_tokens_from_vault(
            &context.accounts.collateral_vault,
            &context.accounts.keeper_collateral_account,
            bounty,
            &context.accounts.collateral_mint,
            &context.accounts.collateral_vault.to_account_info(),
            &context.accounts.token_program,
            &[collateral_vault_seeds],
        )?;
    }

    let holder_share = remaining
        .checked_sub(bounty)
        .ok_or(AssetLeasingError::MathOverflow)?;
    if holder_share > 0 {
        transfer_tokens_from_vault(
            &context.accounts.collateral_vault,
            &context.accounts.holder_collateral_account,
            holder_share,
            &context.accounts.collateral_mint,
            &context.accounts.collateral_vault.to_account_info(),
            &context.accounts.token_program,
            &[collateral_vault_seeds],
        )?;
    }

    // The leased vault is empty (short_seller kept the tokens on default) but was
    // rent-exempt funded at creation. Close both vaults so the holder recoups
    // the rent-exempt lamports.
    close_vault(
        &context.accounts.leased_vault,
        &context.accounts.holder.to_account_info(),
        &context.accounts.token_program,
        &[leased_vault_seeds],
    )?;
    close_vault(
        &context.accounts.collateral_vault,
        &context.accounts.holder.to_account_info(),
        &context.accounts.token_program,
        &[collateral_vault_seeds],
    )?;

    context.accounts.lease.collateral_amount = 0;
    context.accounts.lease.last_paid_timestamp = now.min(context.accounts.lease.end_timestamp);
    context.accounts.lease.status = LeaseStatus::Liquidated;

    Ok(())
}

/// Liquidatable when collateral value < debt value * maintenance margin.
/// All math stays in integers by folding the Pyth exponent into whichever
/// side of the inequality does not already have a power of ten applied.
pub fn is_underwater(lease: &Lease, price: &DecodedPriceUpdate, now: i64) -> Result<bool> {
    // Staleness guard. `publish_time` coming from the future is treated as
    // stale too — the keeper must not front-run the clock.
    require!(price.publish_time <= now, AssetLeasingError::StalePrice);
    let age = (now - price.publish_time) as u64;
    require!(age <= PYTH_MAX_AGE_SECONDS, AssetLeasingError::StalePrice);

    require!(price.price > 0, AssetLeasingError::NonPositivePrice);
    let price_raw = price.price as u128;

    let leased_amount = lease.leased_amount as u128;
    let collateral_amount = lease.collateral_amount as u128;
    let margin_basis_points = lease.maintenance_margin_basis_points as u128;
    let denominator = BASIS_POINTS_DENOMINATOR as u128;

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
        .checked_mul(denominator)
        .ok_or(AssetLeasingError::MathOverflow)?;
    let rhs = debt_scaled
        .checked_mul(margin_basis_points)
        .ok_or(AssetLeasingError::MathOverflow)?;

    Ok(lhs < rhs)
}

fn ten_pow(exponent: u32) -> Result<u128> {
    10u128
        .checked_pow(exponent)
        .ok_or(AssetLeasingError::MathOverflow.into())
}
