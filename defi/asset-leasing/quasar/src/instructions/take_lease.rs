use {
    crate::{
        constants::{COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED},
        errors::AssetLeasingError,
        state::{Lease, LeaseStatus},
    },
    quasar_lang::prelude::*,
    quasar_spl::{Mint, Token, TokenCpi},
};

/// Accounts for accepting a `Listed` lease. The lessee posts their
/// collateral, receives the leased tokens, and the lease transitions to
/// `Active` — all atomically.
#[derive(Accounts)]
pub struct TakeLease<'info> {
    #[account(mut)]
    pub lessee: &'info Signer,

    /// Pubkey of the lessor who created the lease. Referenced only for
    /// `Lease` program-derived address derivation and the `has_one` check below.
    pub lessor: &'info UncheckedAccount,

    #[account(
        mut,
        seeds = [LEASE_SEED, lessor],
        bump = lease.bump,
        has_one = lessor,
        has_one = leased_mint,
        has_one = collateral_mint,
        constraint = LeaseStatus::from_u8(lease.status) == Some(LeaseStatus::Listed)
            @ AssetLeasingError::InvalidLeaseStatus,
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

    /// Lessee's existing collateral-mint token account — must hold at least
    /// `required_collateral_amount` before calling.
    #[account(mut)]
    pub lessee_collateral_account: &'info mut Account<Token>,

    /// Lessee's leased-mint token account. Must be pre-created by the
    /// caller (see the Quasar section of the README for the rationale).
    #[account(mut)]
    pub lessee_leased_account: &'info mut Account<Token>,

    pub token_program: &'info Program<Token>,
}

#[inline(always)]
pub fn handle_take_lease(accounts: &mut TakeLease) -> Result<(), ProgramError> {
    let now = <Clock as quasar_lang::sysvars::Sysvar>::get()?.unix_timestamp.get();

    let required_collateral_amount = accounts.lease.required_collateral_amount.get();
    let leased_amount = accounts.lease.leased_amount.get();
    let duration_seconds = accounts.lease.duration_seconds.get();

    // Lessee deposits collateral first so a failed leased-token transfer
    // (e.g. vault under-funded) rolls back their deposit atomically.
    accounts
        .token_program
        .transfer(
            accounts.lessee_collateral_account,
            accounts.collateral_vault,
            accounts.lessee,
            required_collateral_amount,
        )
        .invoke()?;

    // Pay out leased tokens from the vault program-derived address. Signer seeds reproduce the
    // vault's derivation: [LEASED_VAULT_SEED, lease, bump].
    let leased_vault_bump = [accounts.lease.leased_vault_bump];
    let lease_address = *accounts.lease.address();
    let vault_seeds: &[Seed] = &[
        Seed::from(LEASED_VAULT_SEED),
        Seed::from(lease_address.as_ref()),
        Seed::from(&leased_vault_bump as &[u8]),
    ];
    accounts
        .token_program
        .transfer(
            accounts.leased_vault,
            accounts.lessee_leased_account,
            accounts.leased_vault,
            leased_amount,
        )
        .invoke_signed(vault_seeds)?;

    let end_timestamp = now
        .checked_add(duration_seconds)
        .ok_or(AssetLeasingError::MathOverflow)?;

    let lease = &mut accounts.lease;
    lease.lessee = *accounts.lessee.address();
    lease.collateral_amount = required_collateral_amount.into();
    lease.start_timestamp = now.into();
    lease.end_timestamp = end_timestamp.into();
    lease.last_paid_timestamp = now.into();
    lease.status = LeaseStatus::Active as u8;

    Ok(())
}
