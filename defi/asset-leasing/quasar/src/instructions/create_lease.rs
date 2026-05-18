use {
    crate::{
        constants::{
            COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED, MAX_LIQUIDATION_BOUNTY_BASIS_POINTS,
            MAX_MAINTENANCE_MARGIN_BASIS_POINTS,
        },
        errors::AssetLeasingError,
        state::{Lease, LeaseStatus},
    },
    quasar_lang::prelude::*,
    quasar_spl::{Mint, Token, TokenCpi},
};

/// Accounts needed to create a new `Listed` lease. The lessor funds the
/// lease state account and both program-derived address-owned token vaults up front, then
/// transfers the leased tokens into the leased vault in the same
/// transaction so a lessee can never accept a lease the lessor has not
/// pre-funded.
#[derive(Accounts)]
pub struct CreateLease<'info> {
    #[account(mut)]
    pub lessor: &'info Signer,

    pub leased_mint: &'info Account<Mint>,
    pub collateral_mint: &'info Account<Mint>,

    /// Lessor's existing token account for the leased mint. Pre-created by
    /// the caller — the Quasar port does not do `init_if_needed` associated token accounts
    /// (the Anchor version does, via cross-program invocation to the Associated Token Account
    /// program; see the Quasar section of the README for the rationale).
    #[account(mut)]
    pub lessor_leased_account: &'info mut Account<Token>,

    #[account(
        mut,
        init,
        payer = lessor,
        seeds = [LEASE_SEED, lessor],
        bump,
    )]
    pub lease: &'info mut Account<Lease>,

    /// Leased-token vault. Authority is the vault program-derived address itself — signing
    /// with the vault seeds is the only way to move tokens out.
    #[account(
        mut,
        init,
        payer = lessor,
        seeds = [LEASED_VAULT_SEED, lease],
        bump,
        token::mint = leased_mint,
        token::authority = leased_vault,
    )]
    pub leased_vault: &'info mut Account<Token>,

    /// Collateral vault. Empty while `Listed`; filled on `take_lease`.
    #[account(
        mut,
        init,
        payer = lessor,
        seeds = [COLLATERAL_VAULT_SEED, lease],
        bump,
        token::mint = collateral_mint,
        token::authority = collateral_vault,
    )]
    pub collateral_vault: &'info mut Account<Token>,

    pub rent: &'info Sysvar<Rent>,
    pub token_program: &'info Program<Token>,
    pub system_program: &'info Program<System>,
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn handle_create_lease(
    accounts: &mut CreateLease,
    lease_id: u64,
    leased_amount: u64,
    required_collateral_amount: u64,
    lease_fee_per_second: u64,
    duration_seconds: i64,
    maintenance_margin_basis_points: u16,
    liquidation_bounty_basis_points: u16,
    feed_id: [u8; 32],
    bumps: &CreateLeaseBumps,
) -> Result<(), ProgramError> {
    // Two vaults keyed on the same mint would collide on the shared
    // token-balance pool and make lease-fee-vs-collateral accounting
    // ambiguous. Reject up-front.
    require!(
        accounts.leased_mint.address() != accounts.collateral_mint.address(),
        AssetLeasingError::LeasedMintEqualsCollateralMint
    );

    require!(leased_amount > 0, AssetLeasingError::InvalidLeasedAmount);
    require!(
        required_collateral_amount > 0,
        AssetLeasingError::InvalidCollateralAmount
    );
    require!(
        lease_fee_per_second > 0,
        AssetLeasingError::InvalidLeaseFeePerSecond
    );
    require!(duration_seconds > 0, AssetLeasingError::InvalidDuration);
    require!(
        maintenance_margin_basis_points > 0 && maintenance_margin_basis_points <= MAX_MAINTENANCE_MARGIN_BASIS_POINTS,
        AssetLeasingError::InvalidMaintenanceMargin
    );
    require!(
        liquidation_bounty_basis_points <= MAX_LIQUIDATION_BOUNTY_BASIS_POINTS,
        AssetLeasingError::InvalidLiquidationBounty
    );

    // Lock the leased tokens into the vault up-front. Doing this here —
    // rather than on `take_lease` — guarantees that by the time a lessee
    // sees a `Listed` lease the lessor cannot have moved the funds
    // elsewhere.
    accounts
        .token_program
        .transfer(
            accounts.lessor_leased_account,
            accounts.leased_vault,
            accounts.lessor,
            leased_amount,
        )
        .invoke()?;

    accounts.lease.set_inner(
        lease_id,
        *accounts.lessor.address(),
        // No lessee yet — populated by `take_lease`.
        Address::new_from_array([0u8; 32]),
        *accounts.leased_mint.address(),
        leased_amount,
        *accounts.collateral_mint.address(),
        // No collateral yet — posted on `take_lease`.
        0,
        required_collateral_amount,
        lease_fee_per_second,
        duration_seconds,
        // start_timestamp / end_timestamp / last_paid_timestamp set on `take_lease`.
        0,
        0,
        0,
        maintenance_margin_basis_points,
        liquidation_bounty_basis_points,
        feed_id,
        LeaseStatus::Listed as u8,
        bumps.lease,
        bumps.leased_vault,
        bumps.collateral_vault,
    );

    Ok(())
}
