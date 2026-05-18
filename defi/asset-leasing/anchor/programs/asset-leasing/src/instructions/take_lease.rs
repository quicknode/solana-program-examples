use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED},
    errors::AssetLeasingError,
    instructions::shared::{transfer_tokens_from_user, transfer_tokens_from_vault},
    state::{Lease, LeaseStatus},
};

#[derive(Accounts)]
pub struct TakeLease<'info> {
    #[account(mut)]
    pub short_seller: Signer<'info>,

    /// CHECK: Only used as a reference for the program-derived address seeds; no data accessed.
    pub holder: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [LEASE_SEED, holder.key().as_ref(), &lease.lease_id.to_le_bytes()],
        bump = lease.bump,
        has_one = holder,
        has_one = leased_mint,
        has_one = collateral_mint,
        constraint = lease.status == LeaseStatus::Listed @ AssetLeasingError::InvalidLeaseStatus,
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

    /// ShortSeller's existing collateral account — they must already hold the
    /// required collateral before calling.
    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = short_seller,
        associated_token::token_program = token_program,
    )]
    pub short_seller_collateral_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// ShortSeller's associated token account for the leased mint. Created on-demand if missing so the
    /// UI only has to hand over a short_seller keypair plus the two mints.
    #[account(
        init_if_needed,
        payer = short_seller,
        associated_token::mint = leased_mint,
        associated_token::authority = short_seller,
        associated_token::token_program = token_program,
    )]
    pub short_seller_leased_account: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_take_lease(context: Context<TakeLease>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    // Bindings for values we still need after `&mut lease` is borrowed.
    let required_collateral_amount = context.accounts.lease.required_collateral_amount;
    let leased_amount = context.accounts.lease.leased_amount;
    let duration_seconds = context.accounts.lease.duration_seconds;

    // ShortSeller deposits collateral first so a failed leased-token transfer
    // rolls back their deposit atomically.
    transfer_tokens_from_user(
        &context.accounts.short_seller_collateral_account,
        &context.accounts.collateral_vault,
        required_collateral_amount,
        &context.accounts.collateral_mint,
        &context.accounts.short_seller,
        &context.accounts.token_program,
    )?;

    // Pay out leased tokens from the vault program-derived address.
    let lease_key = context.accounts.lease.key();
    let leased_vault_bump = context.accounts.lease.leased_vault_bump;
    let leased_vault_seeds: &[&[u8]] = &[
        LEASED_VAULT_SEED,
        lease_key.as_ref(),
        core::slice::from_ref(&leased_vault_bump),
    ];
    let signer_seeds = [leased_vault_seeds];

    transfer_tokens_from_vault(
        &context.accounts.leased_vault,
        &context.accounts.short_seller_leased_account,
        leased_amount,
        &context.accounts.leased_mint,
        &context.accounts.leased_vault.to_account_info(),
        &context.accounts.token_program,
        &signer_seeds,
    )?;

    let end_timestamp = now
        .checked_add(duration_seconds)
        .ok_or(AssetLeasingError::MathOverflow)?;

    let lease = &mut context.accounts.lease;
    lease.short_seller = context.accounts.short_seller.key();
    lease.collateral_amount = required_collateral_amount;
    lease.start_timestamp = now;
    lease.end_timestamp = end_timestamp;
    lease.last_paid_timestamp = now;
    lease.status = LeaseStatus::Active;

    Ok(())
}
