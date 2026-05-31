use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::error::StopLossError;
use crate::state::Vault;

/// Owner pulls `amount` of the stable token out of the vault. Only callable
/// after `convert_if_triggered` has fired — until then there are no stables
/// to take.
pub fn handler(ctx: Context<WithdrawStablesAccountConstraints>, amount: u64) -> Result<()> {
    require!(
        ctx.accounts.vault.triggered,
        StopLossError::VaultNotTriggered
    );

    let owner_key = ctx.accounts.owner.key();
    let bump = ctx.accounts.vault.bump;
    let signer_seeds: &[&[&[u8]]] =
        &[&[Vault::SEED_PREFIX, owner_key.as_ref(), &[bump]]];

    let cpi_context = CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        TransferChecked {
            from: ctx.accounts.vault_stable_account.to_account_info(),
            mint: ctx.accounts.stable_mint.to_account_info(),
            to: ctx.accounts.owner_stable_account.to_account_info(),
            authority: ctx.accounts.vault.to_account_info(),
        },
        signer_seeds,
    );
    token_interface::transfer_checked(cpi_context, amount, ctx.accounts.stable_mint.decimals)?;
    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawStablesAccountConstraints<'info> {
    #[account(
        mut,
        seeds = [Vault::SEED_PREFIX, owner.key().as_ref()],
        bump = vault.bump,
        has_one = owner @ StopLossError::Unauthorized,
        has_one = stable_mint,
    )]
    pub vault: Account<'info, Vault>,

    pub stable_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = stable_mint,
        associated_token::authority = vault,
    )]
    pub vault_stable_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = stable_mint,
        associated_token::authority = owner,
    )]
    pub owner_stable_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}
