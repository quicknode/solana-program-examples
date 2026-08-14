use anchor_lang::prelude::*;
use anchor_spl::token;
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
    context: &mut Context<DepositObligationCollateral>,
    share_amount: u64,
) -> Result<()> {
    require!(share_amount > 0, LendingError::ZeroAmount);

    let reserve_key = context.accounts.reserve.address();
    let obligation = &mut context.accounts.obligation;
    let index = obligation.upsert_collateral(*reserve_key)?;
    obligation.deposits[index].deposited_shares = obligation.deposits[index]
        .deposited_shares
        .checked_add(share_amount)
        .ok_or(LendingError::MathOverflow)?;
    obligation.stale = true;

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.user_share.cpi_handle_mut(),
                mint: context.accounts.share_mint.cpi_handle(),
                to: context.accounts.obligation_share_vault.cpi_handle_mut(),
                authority: context.accounts.owner.cpi_handle(),
            },
        ),
        share_amount,
        context.accounts.share_mint.decimals(),
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct DepositObligationCollateral {
    #[account(mut)]
    pub obligation: BorshAccount<Obligation>,

    #[account(mut, address = obligation.owner)]
    pub owner: Signer,

    #[account(
        constraint = reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub reserve: BorshAccount<Reserve>,

    #[account(address = reserve.share_mint)]
    pub share_mint: InterfaceAccount<Mint>,

    #[account(
        init_if_needed,
        payer = owner,
        token::mint = share_mint,
        token::authority = obligation,
        seeds = [OBLIGATION_SHARE_VAULT_SEED, reserve.address().as_ref(), obligation.address().as_ref()],
        bump,
    )]
    pub obligation_share_vault: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub user_share: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,

    pub system_program: Program<System>,
}
