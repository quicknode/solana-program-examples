use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{COLLATERAL_VAULT_SEED, LEASE_SEED},
    errors::AssetLeasingError,
    instructions::shared::transfer_tokens_from_vault,
    state::{Lease, LeaseStatus},
};

#[derive(Accounts)]
pub struct PayLeaseFee<'info> {
    /// Anyone may settle the lease fee — the short_seller has every incentive to keep the
    /// lease current, but a keeper bot could also push a lease fee payment before a
    /// liquidation check so healthy leases stay healthy.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Referenced only for program-derived address derivation + has_one check on `lease`.
    pub holder: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [LEASE_SEED, holder.key().as_ref(), &lease.lease_id.to_le_bytes()],
        bump = lease.bump,
        has_one = holder,
        has_one = collateral_mint,
        constraint = lease.status == LeaseStatus::Active @ AssetLeasingError::InvalidLeaseStatus,
    )]
    pub lease: Account<'info, Lease>,

    pub collateral_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        seeds = [COLLATERAL_VAULT_SEED, lease.key().as_ref()],
        bump = lease.collateral_vault_bump,
        token::mint = collateral_mint,
        token::authority = collateral_vault,
        token::token_program = token_program,
    )]
    pub collateral_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Holder's collateral-mint associated token account, created on demand so the holder does not
    /// need to pre-fund it with the lease fee.
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = collateral_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_collateral_account: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_pay_lease_fee(context: Context<PayLeaseFee>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    let lease_fee_amount = compute_lease_fee_due(&context.accounts.lease, now)?;

    // No time has passed (or already capped at end_timestamp). Nothing to do.
    if lease_fee_amount == 0 {
        update_last_paid_timestamp(&mut context.accounts.lease, now);
        return Ok(());
    }

    // Cap lease fees at whatever collateral actually sits in the vault. If the
    // short_seller under-collateralised we cannot magically create funds; the
    // remainder is their debt and can trigger liquidation.
    let payable = lease_fee_amount.min(context.accounts.collateral_amount_available());

    if payable > 0 {
        let lease_key = context.accounts.lease.key();
        let collateral_vault_bump = context.accounts.lease.collateral_vault_bump;
        let collateral_vault_seeds: &[&[u8]] = &[
            COLLATERAL_VAULT_SEED,
            lease_key.as_ref(),
            core::slice::from_ref(&collateral_vault_bump),
        ];
        let signer_seeds = [collateral_vault_seeds];

        transfer_tokens_from_vault(
            &context.accounts.collateral_vault,
            &context.accounts.holder_collateral_account,
            payable,
            &context.accounts.collateral_mint,
            &context.accounts.collateral_vault.to_account_info(),
            &context.accounts.token_program,
            &signer_seeds,
        )?;

        context.accounts.lease.collateral_amount = context
            .accounts
            .lease
            .collateral_amount
            .checked_sub(payable)
            .ok_or(AssetLeasingError::MathOverflow)?;
    }

    update_last_paid_timestamp(&mut context.accounts.lease, now);
    Ok(())
}

/// Lease fee accrues linearly: `(min(now, end_timestamp) - last_paid_timestamp) * rate`.
/// Extracted so it can be re-used by `return_lease` and `liquidate` for a
/// final settlement before closing the lease.
pub fn compute_lease_fee_due(lease: &Lease, now: i64) -> Result<u64> {
    let cutoff = now.min(lease.end_timestamp);
    if cutoff <= lease.last_paid_timestamp {
        return Ok(0);
    }
    let elapsed = (cutoff - lease.last_paid_timestamp) as u64;
    elapsed
        .checked_mul(lease.lease_fee_per_second)
        .ok_or(AssetLeasingError::MathOverflow.into())
}

/// Advance `last_paid_timestamp` but never past the lease end — after end_timestamp
/// the lease is settled and extra Lease fees do not accrue.
pub fn update_last_paid_timestamp(lease: &mut Lease, now: i64) {
    lease.last_paid_timestamp = now.min(lease.end_timestamp);
}

impl<'info> PayLeaseFee<'info> {
    fn collateral_amount_available(&self) -> u64 {
        self.lease.collateral_amount
    }
}
