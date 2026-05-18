use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED},
    errors::AssetLeasingError,
    instructions::{
        pay_lease_fee::update_last_paid_timestamp,
        shared::{close_vault, transfer_tokens_from_vault},
    },
    state::{Lease, LeaseStatus},
};

/// Holder-only recovery path. Two real-world situations collapse here:
///
/// - The lease sat in `Listed` and the holder wants to cancel it, recovering
///   the leased tokens they pre-funded. Allowed any time.
/// - The lease was `Active` but the short_seller ghosted past `end_timestamp`. The holder
///   takes the collateral as compensation and closes the books.
#[derive(Accounts)]
pub struct CloseExpired<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,

    #[account(
        mut,
        seeds = [LEASE_SEED, holder.key().as_ref(), &lease.lease_id.to_le_bytes()],
        bump = lease.bump,
        has_one = holder,
        has_one = leased_mint,
        has_one = collateral_mint,
        constraint = matches!(lease.status, LeaseStatus::Listed | LeaseStatus::Active)
            @ AssetLeasingError::InvalidLeaseStatus,
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
        payer = holder,
        associated_token::mint = leased_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_leased_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = holder,
        associated_token::mint = collateral_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_collateral_account: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_close_expired(context: Context<CloseExpired>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let lease_key = context.accounts.lease.key();
    let status = context.accounts.lease.status;

    // Active leases can only be closed after they expire. Listed leases have
    // no start/end so the check is skipped.
    if status == LeaseStatus::Active {
        require!(
            now >= context.accounts.lease.end_timestamp,
            AssetLeasingError::LeaseNotExpired
        );
    }

    let leased_vault_bump = context.accounts.lease.leased_vault_bump;
    let leased_vault_seeds: &[&[u8]] = &[
        LEASED_VAULT_SEED,
        lease_key.as_ref(),
        core::slice::from_ref(&leased_vault_bump),
    ];
    let collateral_vault_bump = context.accounts.lease.collateral_vault_bump;
    let collateral_vault_seeds: &[&[u8]] = &[
        COLLATERAL_VAULT_SEED,
        lease_key.as_ref(),
        core::slice::from_ref(&collateral_vault_bump),
    ];

    // Drain whatever is in the leased vault back to the holder. For a Listed
    // lease this is the full leased_amount; for a defaulted Active lease the
    // vault is empty (the short_seller never returned) and this is a no-op.
    let leased_vault_balance = context.accounts.leased_vault.amount;
    if leased_vault_balance > 0 {
        transfer_tokens_from_vault(
            &context.accounts.leased_vault,
            &context.accounts.holder_leased_account,
            leased_vault_balance,
            &context.accounts.leased_mint,
            &context.accounts.leased_vault.to_account_info(),
            &context.accounts.token_program,
            &[leased_vault_seeds],
        )?;
    }

    // Drain the collateral vault to the holder. For a Listed lease this is 0.
    // For a defaulted Active lease this is the short_seller's forfeited collateral.
    let collateral_vault_balance = context.accounts.collateral_vault.amount;
    if collateral_vault_balance > 0 {
        transfer_tokens_from_vault(
            &context.accounts.collateral_vault,
            &context.accounts.holder_collateral_account,
            collateral_vault_balance,
            &context.accounts.collateral_mint,
            &context.accounts.collateral_vault.to_account_info(),
            &context.accounts.token_program,
            &[collateral_vault_seeds],
        )?;
    }

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

    // Settle lease-fee accounting on the default path.
    //
    // We are not forwarding any accrued lease fees to the holder here — on default
    // the holder takes the whole collateral vault as compensation — but we
    // still bump \`last_paid_timestamp\` so the invariant
    // \`last_paid_timestamp <= now.min(end_timestamp)\` stays intact. That matters for
    // any future version of the program that wants to split the collateral
    // differently (pro-rata lease fees, partial refund on default, haircut to the
    // short_seller for unused time): such a version can read
    // \`last_paid_timestamp\` and trust that everything up to \`now\` is already
    // settled, rather than having to reason about whether this branch ever
    // bumped the timestamp.
    //
    // No-op on the \`Listed\` branch because Lease fees never started accruing.
    if status == LeaseStatus::Active {
        update_last_paid_timestamp(&mut context.accounts.lease, now);
    }
    context.accounts.lease.collateral_amount = 0;
    context.accounts.lease.status = LeaseStatus::Closed;

    Ok(())
}
