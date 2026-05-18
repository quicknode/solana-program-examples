use {
    crate::{
        constants::{COLLATERAL_VAULT_SEED, LEASE_SEED},
        errors::AssetLeasingError,
        state::{Lease, LeaseStatus},
    },
    quasar_lang::prelude::*,
    quasar_spl::{Mint, Token, TokenCpi},
};

/// Accounts for settling the lease fee on an `Active` lease. Permissionless: the
/// lessee has every incentive to keep the lease current, but a keeper bot
/// could also push a lease fee payment before a liquidation check.
#[derive(Accounts)]
pub struct PayLeaseFee<'info> {
    #[account(mut)]
    pub payer: &'info Signer,

    /// program-derived address seed + `has_one` target. Not read directly.
    pub lessor: &'info UncheckedAccount,

    #[account(
        mut,
        seeds = [LEASE_SEED, lessor],
        bump = lease.bump,
        has_one = lessor,
        has_one = collateral_mint,
        constraint = LeaseStatus::from_u8(lease.status) == Some(LeaseStatus::Active)
            @ AssetLeasingError::InvalidLeaseStatus,
    )]
    pub lease: &'info mut Account<Lease>,

    pub collateral_mint: &'info Account<Mint>,

    #[account(
        mut,
        seeds = [COLLATERAL_VAULT_SEED, lease],
        bump = lease.collateral_vault_bump,
    )]
    pub collateral_vault: &'info mut Account<Token>,

    /// Lessor's collateral token account. Pre-created by the caller.
    #[account(mut)]
    pub lessor_collateral_account: &'info mut Account<Token>,

    pub token_program: &'info Program<Token>,
}

#[inline(always)]
pub fn handle_pay_lease_fee(accounts: &mut PayLeaseFee) -> Result<(), ProgramError> {
    let now = <Clock as quasar_lang::sysvars::Sysvar>::get()?.unix_timestamp.get();

    let lease_fee_amount = compute_lease_fee_due(accounts.lease, now)?;

    if lease_fee_amount == 0 {
        update_last_paid_timestamp(accounts.lease, now);
        return Ok(());
    }

    // Cap lease fees at whatever collateral actually sits in the vault. If the
    // lessee under-collateralised we cannot magically create funds; the
    // remainder is their debt and can trigger liquidation.
    let collateral_amount = accounts.lease.collateral_amount.get();
    let payable = lease_fee_amount.min(collateral_amount);

    if payable > 0 {
        let lease_address = *accounts.lease.address();
        let collateral_vault_bump = [accounts.lease.collateral_vault_bump];
        let vault_seeds: &[Seed] = &[
            Seed::from(COLLATERAL_VAULT_SEED),
            Seed::from(lease_address.as_ref()),
            Seed::from(&collateral_vault_bump as &[u8]),
        ];
        accounts
            .token_program
            .transfer(
                accounts.collateral_vault,
                accounts.lessor_collateral_account,
                accounts.collateral_vault,
                payable,
            )
            .invoke_signed(vault_seeds)?;

        let new_collateral = collateral_amount
            .checked_sub(payable)
            .ok_or(AssetLeasingError::MathOverflow)?;
        accounts.lease.collateral_amount = new_collateral.into();
    }

    update_last_paid_timestamp(accounts.lease, now);
    Ok(())
}

/// Lease fee accrues linearly: `(min(now, end_timestamp) - last_paid_timestamp) * rate`.
/// Shared with `return_lease` and `liquidate` for final settlement.
pub fn compute_lease_fee_due(lease: &Lease, now: i64) -> Result<u64, ProgramError> {
    let end_timestamp = lease.end_timestamp.get();
    let last_paid = lease.last_paid_timestamp.get();
    let cutoff = now.min(end_timestamp);
    if cutoff <= last_paid {
        return Ok(0);
    }
    let elapsed = (cutoff - last_paid) as u64;
    elapsed
        .checked_mul(lease.lease_fee_per_second.get())
        .ok_or_else(|| AssetLeasingError::MathOverflow.into())
}

/// Advance `last_paid_timestamp`, but never past `end_timestamp` — once the lease
/// is over, extra Lease fees do not accrue.
pub fn update_last_paid_timestamp(lease: &mut Lease, now: i64) {
    let end_timestamp = lease.end_timestamp.get();
    let capped = now.min(end_timestamp);
    lease.last_paid_timestamp = capped.into();
}
