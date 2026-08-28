use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::error::StopLossError;
use crate::state::Vault;

/// Move `amount` of the volatile token from the owner's ATA into the vault's
/// volatile ATA. The vault must not already have triggered.
pub fn handler(ctx: Context<DepositAccountConstraints>, amount: u64) -> Result<()> {
    require!(
        !ctx.accounts.vault.triggered,
        StopLossError::VaultAlreadyTriggered
    );

    let cpi_context = CpiContext::new(
        ctx.accounts.token_program.key(),
        TransferChecked {
            from: ctx.accounts.owner_volatile_account.to_account_info(),
            mint: ctx.accounts.volatile_mint.to_account_info(),
            to: ctx.accounts.vault_volatile_account.to_account_info(),
            authority: ctx.accounts.owner.to_account_info(),
        },
    );
    token_interface::transfer_checked(cpi_context, amount, ctx.accounts.volatile_mint.decimals)?;
    Ok(())
}

#[derive(Accounts)]
pub struct DepositAccountConstraints<'info> {
    #[account(
        mut,
        seeds = [Vault::SEED_PREFIX, owner.key().as_ref()],
        bump = vault.bump,
        has_one = owner @ StopLossError::Unauthorized,
        has_one = volatile_mint,
    )]
    pub vault: Account<'info, Vault>,

    pub volatile_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = volatile_mint,
        associated_token::authority = vault,
    )]
    pub vault_volatile_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = volatile_mint,
        associated_token::authority = owner,
    )]
    pub owner_volatile_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}
