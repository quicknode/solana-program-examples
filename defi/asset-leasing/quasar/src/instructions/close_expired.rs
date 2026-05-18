use {
    crate::{
        constants::{COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED},
        errors::AssetLeasingError,
        instructions::pay_lease_fee::update_last_paid_timestamp,
        state::{Lease, LeaseStatus},
    },
    quasar_lang::prelude::*,
    quasar_spl::{Mint, Token, TokenCpi},
};

/// Lessor-only recovery path. Two situations collapse into this handler:
///
/// - The lease sat in `Listed` and the lessor wants to cancel it,
///   recovering the leased tokens they pre-funded. Allowed any time.
/// - The lease was `Active` but the lessee ghosted past `end_timestamp`. The
///   lessor takes the collateral as compensation and closes the books.
#[derive(Accounts)]
pub struct CloseExpired<'info> {
    #[account(mut)]
    pub lessor: &'info Signer,

    #[account(
        mut,
        seeds = [LEASE_SEED, lessor],
        bump = lease.bump,
        has_one = lessor,
        has_one = leased_mint,
        has_one = collateral_mint,
        constraint = {
            let s = LeaseStatus::from_u8(lease.status);
            s == Some(LeaseStatus::Listed) || s == Some(LeaseStatus::Active)
        } @ AssetLeasingError::InvalidLeaseStatus,
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
    pub lessor_leased_account: &'info mut Account<Token>,

    #[account(mut)]
    pub lessor_collateral_account: &'info mut Account<Token>,

    pub token_program: &'info Program<Token>,
}

#[inline(always)]
pub fn handle_close_expired(accounts: &mut CloseExpired) -> Result<(), ProgramError> {
    let now = <Clock as quasar_lang::sysvars::Sysvar>::get()?.unix_timestamp.get();
    let lease_address = *accounts.lease.address();
    let status = LeaseStatus::from_u8(accounts.lease.status)
        .ok_or(AssetLeasingError::InvalidStatusByte)?;

    // Active leases can only be closed after they expire. Listed leases
    // have no start/end so the check is skipped.
    if status == LeaseStatus::Active {
        let end_timestamp = accounts.lease.end_timestamp.get();
        if now < end_timestamp {
            return Err(AssetLeasingError::LeaseNotExpired.into());
        }
    }

    let leased_vault_bump = [accounts.lease.leased_vault_bump];
    let leased_vault_seeds: &[Seed] = &[
        Seed::from(LEASED_VAULT_SEED),
        Seed::from(lease_address.as_ref()),
        Seed::from(&leased_vault_bump as &[u8]),
    ];
    let collateral_vault_bump = [accounts.lease.collateral_vault_bump];
    let collateral_vault_seeds: &[Seed] = &[
        Seed::from(COLLATERAL_VAULT_SEED),
        Seed::from(lease_address.as_ref()),
        Seed::from(&collateral_vault_bump as &[u8]),
    ];

    // Drain whatever is in the leased vault back to the lessor. For a
    // Listed lease this is the full leased_amount; for a defaulted
    // Active lease the vault is empty (the lessee never returned) so
    // this is a no-op.
    let leased_vault_balance = accounts.leased_vault.amount();
    if leased_vault_balance > 0 {
        accounts
            .token_program
            .transfer(
                accounts.leased_vault,
                accounts.lessor_leased_account,
                accounts.leased_vault,
                leased_vault_balance,
            )
            .invoke_signed(leased_vault_seeds)?;
    }

    // Drain the collateral vault to the lessor. For a Listed lease this
    // is 0. For a defaulted Active lease this is the lessee's forfeited
    // collateral.
    let collateral_vault_balance = accounts.collateral_vault.amount();
    if collateral_vault_balance > 0 {
        accounts
            .token_program
            .transfer(
                accounts.collateral_vault,
                accounts.lessor_collateral_account,
                accounts.collateral_vault,
                collateral_vault_balance,
            )
            .invoke_signed(collateral_vault_seeds)?;
    }

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

    // Keep the lease-fee-settlement invariant intact even on default: the
    // lessor takes the whole collateral vault as compensation here, but
    // any future version of the program that wants to split the
    // collateral differently (pro-rata lease fees, partial refund on default)
    // can read `last_paid_timestamp` and trust that everything up to
    // `now` is already settled.
    if status == LeaseStatus::Active {
        update_last_paid_timestamp(accounts.lease, now);
    }
    accounts.lease.collateral_amount = 0u64.into();
    accounts.lease.status = LeaseStatus::Closed as u8;

    Ok(())
}
