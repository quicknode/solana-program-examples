use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED},
    errors::AssetLeasingError,
    instructions::{
        pay_lease_fee::{compute_lease_fee_due, update_last_paid_timestamp},
        shared::{close_vault, transfer_tokens_from_user, transfer_tokens_from_vault},
    },
    state::{Lease, LeaseStatus},
};

#[derive(Accounts)]
pub struct ReturnLease<'info> {
    #[account(mut)]
    pub short_seller: Signer<'info>,

    /// CHECK: Reference only — receives the lease fee + closed-vault rent-exempt-lamport refund.
    #[account(mut)]
    pub holder: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [LEASE_SEED, holder.key().as_ref(), &lease.lease_id.to_le_bytes()],
        bump = lease.bump,
        has_one = holder,
        has_one = leased_mint,
        has_one = collateral_mint,
        constraint = lease.short_seller == short_seller.key() @ AssetLeasingError::Unauthorised,
        constraint = lease.status == LeaseStatus::Active @ AssetLeasingError::InvalidLeaseStatus,
        close = holder,
    )]
    pub lease: Account<'info, Lease>,

    pub leased_mint: Box<InterfaceAccount<'info, Mint>>,
    pub collateral_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Leased tokens flow back into this vault from the short_seller, then out to
    /// the holder in the same instruction. Closed at the end to reclaim rent-exempt lamports.
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
        mut,
        associated_token::mint = leased_mint,
        associated_token::authority = short_seller,
        associated_token::token_program = token_program,
    )]
    pub short_seller_leased_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = short_seller,
        associated_token::token_program = token_program,
    )]
    pub short_seller_collateral_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Holder's leased-mint associated token account, created on demand. They may have sent the
    /// original tokens from a different account.
    #[account(
        init_if_needed,
        payer = short_seller,
        associated_token::mint = leased_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_leased_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = short_seller,
        associated_token::mint = collateral_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_collateral_account: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_return_lease(context: Context<ReturnLease>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let lease_key = context.accounts.lease.key();

    // 1. ShortSeller returns leased tokens to the leased vault (full amount).
    let leased_amount = context.accounts.lease.leased_amount;
    transfer_tokens_from_user(
        &context.accounts.short_seller_leased_account,
        &context.accounts.leased_vault,
        leased_amount,
        &context.accounts.leased_mint,
        &context.accounts.short_seller,
        &context.accounts.token_program,
    )?;

    // 2. Forward leased tokens from the vault to the holder.
    let leased_vault_bump = context.accounts.lease.leased_vault_bump;
    let leased_vault_seeds: &[&[u8]] = &[
        LEASED_VAULT_SEED,
        lease_key.as_ref(),
        core::slice::from_ref(&leased_vault_bump),
    ];
    transfer_tokens_from_vault(
        &context.accounts.leased_vault,
        &context.accounts.holder_leased_account,
        leased_amount,
        &context.accounts.leased_mint,
        &context.accounts.leased_vault.to_account_info(),
        &context.accounts.token_program,
        &[leased_vault_seeds],
    )?;

    // 3. Settle accrued lease fees: collateral vault -> holder.
    let lease_fee_due = compute_lease_fee_due(&context.accounts.lease, now)?;
    let lease_fee_payable = lease_fee_due.min(context.accounts.lease.collateral_amount);

    let collateral_vault_bump = context.accounts.lease.collateral_vault_bump;
    let collateral_vault_seeds: &[&[u8]] = &[
        COLLATERAL_VAULT_SEED,
        lease_key.as_ref(),
        core::slice::from_ref(&collateral_vault_bump),
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

    // 4. Refund remaining collateral to the short_seller. Returning early does not
    // entitle the short_seller to a future-lease-fee refund — Lease fees only accrue for time
    // actually used, so `compute_lease_fee_due` already excludes the unused tail.
    let collateral_after_lease_fees = context
        .accounts
        .lease
        .collateral_amount
        .checked_sub(lease_fee_payable)
        .ok_or(AssetLeasingError::MathOverflow)?;

    if collateral_after_lease_fees > 0 {
        transfer_tokens_from_vault(
            &context.accounts.collateral_vault,
            &context.accounts.short_seller_collateral_account,
            collateral_after_lease_fees,
            &context.accounts.collateral_mint,
            &context.accounts.collateral_vault.to_account_info(),
            &context.accounts.token_program,
            &[collateral_vault_seeds],
        )?;
    }

    // 5. Close both vaults so the rent-exempt lamports come back to the
    // holder — the short_seller only pays for the temporary state they held.
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

    update_last_paid_timestamp(&mut context.accounts.lease, now);
    context.accounts.lease.collateral_amount = 0;
    context.accounts.lease.status = LeaseStatus::Closed;

    Ok(())
}
