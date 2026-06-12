use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::OBLIGATION_SHARE_VAULT_SEED;
use crate::errors::LendingError;
use crate::state::{Obligation, Reserve};

/// Post share tokens as collateral. The shares move into a per-(reserve,
/// obligation) vault owned by the obligation PDA. No health check is needed —
/// adding collateral only improves health — but the obligation is marked stale
/// so its cached values are recomputed before the next health-dependent action.
pub fn handle_deposit_obligation_collateral(
    context: Context<DepositObligationCollateral>,
    share_amount: u64,
) -> Result<()> {
    require!(share_amount > 0, LendingError::ZeroAmount);

    let reserve_key = context.accounts.reserve.key();
    let obligation = &mut context.accounts.obligation;
    let index = obligation.upsert_collateral(reserve_key)?;
    obligation.deposits[index].deposited_shares = obligation.deposits[index]
        .deposited_shares
        .checked_add(share_amount)
        .ok_or(LendingError::MathOverflow)?;
    obligation.stale = true;

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.user_share.to_account_info(),
                mint: context.accounts.share_mint.to_account_info(),
                to: context.accounts.obligation_share_vault.to_account_info(),
                authority: context.accounts.owner.to_account_info(),
            },
        ),
        share_amount,
        context.accounts.share_mint.decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct DepositObligationCollateral<'info> {
    #[account(mut, has_one = owner)]
    pub obligation: Account<'info, Obligation>,

    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        has_one = share_mint,
        constraint = reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub reserve: Account<'info, Reserve>,

    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = owner,
        token::mint = share_mint,
        token::authority = obligation,
        seeds = [OBLIGATION_SHARE_VAULT_SEED, reserve.key().as_ref(), obligation.key().as_ref()],
        bump,
    )]
    pub obligation_share_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_share: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
