use {
    crate::{
        constants::{COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED},
        errors::AssetLeasingError,
        instructions::pay_lease_fee::{compute_lease_fee_due, update_last_paid_timestamp},
        state::{Lease, LeaseStatus},
    },
    quasar_lang::prelude::*,
    quasar_spl::{Mint, Token, TokenCpi},
};

/// Accounts for the happy-path return. Lessee hands the leased tokens
/// back, pays accrued lease fees out of their collateral, and receives whatever
/// collateral is left. Both vaults are closed so the lessor recoups the
/// rent-exempt lamports.
#[derive(Accounts)]
pub struct ReturnLease<'info> {
    #[account(mut)]
    pub lessee: &'info Signer,

    /// Receives the leased tokens + any accrued lease fees + the vaults'
    /// rent-exempt lamports.
    #[account(mut)]
    pub lessor: &'info UncheckedAccount,

    #[account(
        mut,
        seeds = [LEASE_SEED, lessor],
        bump = lease.bump,
        has_one = lessor,
        has_one = leased_mint,
        has_one = collateral_mint,
        constraint = lease.lessee == *lessee.address() @ AssetLeasingError::Unauthorised,
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

    #[account(mut)]
    pub lessee_leased_account: &'info mut Account<Token>,

    #[account(mut)]
    pub lessee_collateral_account: &'info mut Account<Token>,

    /// Lessor's leased-mint token account. Pre-created by the caller.
    #[account(mut)]
    pub lessor_leased_account: &'info mut Account<Token>,

    /// Lessor's collateral-mint token account. Pre-created by the caller.
    #[account(mut)]
    pub lessor_collateral_account: &'info mut Account<Token>,

    pub token_program: &'info Program<Token>,
}

#[inline(always)]
pub fn handle_return_lease(accounts: &mut ReturnLease) -> Result<(), ProgramError> {
    let now = <Clock as quasar_lang::sysvars::Sysvar>::get()?.unix_timestamp.get();
    let lease_address = *accounts.lease.address();
    let leased_amount = accounts.lease.leased_amount.get();

    // 1. Lessee returns leased tokens to the leased vault (full amount).
    accounts
        .token_program
        .transfer(
            accounts.lessee_leased_account,
            accounts.leased_vault,
            accounts.lessee,
            leased_amount,
        )
        .invoke()?;

    // 2. Forward leased tokens from the vault to the lessor.
    let leased_vault_bump = [accounts.lease.leased_vault_bump];
    let leased_vault_seeds: &[Seed] = &[
        Seed::from(LEASED_VAULT_SEED),
        Seed::from(lease_address.as_ref()),
        Seed::from(&leased_vault_bump as &[u8]),
    ];
    accounts
        .token_program
        .transfer(
            accounts.leased_vault,
            accounts.lessor_leased_account,
            accounts.leased_vault,
            leased_amount,
        )
        .invoke_signed(leased_vault_seeds)?;

    // 3. Settle accrued lease fees: collateral vault -> lessor.
    let lease_fee_due = compute_lease_fee_due(accounts.lease, now)?;
    let collateral_amount = accounts.lease.collateral_amount.get();
    let lease_fee_payable = lease_fee_due.min(collateral_amount);

    let collateral_vault_bump = [accounts.lease.collateral_vault_bump];
    let collateral_vault_seeds: &[Seed] = &[
        Seed::from(COLLATERAL_VAULT_SEED),
        Seed::from(lease_address.as_ref()),
        Seed::from(&collateral_vault_bump as &[u8]),
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

    // 4. Refund remaining collateral to the lessee. Returning early does
    // not entitle the lessee to a future-lease-fee refund — Lease fees only accrue
    // for time actually used, so `compute_lease_fee_due` already excludes the
    // unused tail.
    let collateral_after_lease_fees = collateral_amount
        .checked_sub(lease_fee_payable)
        .ok_or(AssetLeasingError::MathOverflow)?;

    if collateral_after_lease_fees > 0 {
        accounts
            .token_program
            .transfer(
                accounts.collateral_vault,
                accounts.lessee_collateral_account,
                accounts.collateral_vault,
                collateral_after_lease_fees,
            )
            .invoke_signed(collateral_vault_seeds)?;
    }

    // 5. Close both vaults so the rent-exempt lamports flow to the lessor
    // — the lessee only pays for the temporary state they held while the
    // lease was active.
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

    update_last_paid_timestamp(accounts.lease, now);
    accounts.lease.collateral_amount = 0u64.into();
    accounts.lease.status = LeaseStatus::Closed as u8;

    Ok(())
}
