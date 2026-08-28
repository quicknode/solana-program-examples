use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::error::StopLossError;
use crate::state::Vault;

/// Owner pulls `amount` of the volatile token back out of the vault. This is
/// the escape hatch for a vault that never triggered: while the price stays
/// above the threshold the deposit would otherwise be locked with no way out.
/// Refused once the vault has triggered — at that point the volatile balance
/// is zero and the position is held in stables (use `withdraw_stables`).
pub fn handler(ctx: Context<WithdrawVolatileAccountConstraints>, amount: u64) -> Result<()> {
    require!(
        !ctx.accounts.vault.triggered,
        StopLossError::VaultAlreadyTriggered
    );

    let owner_key = ctx.accounts.owner.key();
    let bump = ctx.accounts.vault.bump;
    let signer_seeds: &[&[&[u8]]] =
        &[&[Vault::SEED_PREFIX, owner_key.as_ref(), &[bump]]];

    let cpi_context = CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        TransferChecked {
            from: ctx.accounts.vault_volatile_account.to_account_info(),
            mint: ctx.accounts.volatile_mint.to_account_info(),
            to: ctx.accounts.owner_volatile_account.to_account_info(),
            authority: ctx.accounts.vault.to_account_info(),
        },
        signer_seeds,
    );
    token_interface::transfer_checked(cpi_context, amount, ctx.accounts.volatile_mint.decimals)?;
    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawVolatileAccountConstraints<'info> {
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
