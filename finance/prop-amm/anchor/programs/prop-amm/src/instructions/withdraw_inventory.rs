use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::{AUTHORITY_SEED, BASE_VAULT_SEED, MARKET_SEED, QUOTE_VAULT_SEED};
use crate::errors::PropAmmError;
use crate::state::Market;

/// The operator takes inventory back out — up to every token in both vaults,
/// at any time, with nobody's permission. This is the property that separates
/// a prop AMM from the pooled venues earlier in the companion repository:
/// there are no liquidity-provider shares because there are no liquidity
/// providers. The capital being quoted is the firm's own, so its exit needs no
/// waterfall, no share burn, and no pro-rata math.
pub fn handle_withdraw_inventory(
    context: Context<WithdrawInventoryAccountConstraints>,
    base_amount: u64,
    quote_amount: u64,
) -> Result<()> {
    require!(
        base_amount > 0 || quote_amount > 0,
        PropAmmError::ZeroAmount
    );
    require!(
        base_amount <= context.accounts.base_vault.amount,
        PropAmmError::InsufficientInventory
    );
    require!(
        quote_amount <= context.accounts.quote_vault.amount,
        PropAmmError::InsufficientInventory
    );

    let market = &context.accounts.market;
    let market_key = market.key();
    let authority_seeds: &[&[u8]] = &[
        AUTHORITY_SEED,
        market_key.as_ref(),
        &[market.authority_bump],
    ];

    if base_amount > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.base_vault.to_account_info(),
                    mint: context.accounts.base_mint.to_account_info(),
                    to: context.accounts.operator_base.to_account_info(),
                    authority: context.accounts.market_authority.to_account_info(),
                },
                &[authority_seeds],
            ),
            base_amount,
            context.accounts.base_mint.decimals,
        )?;
    }

    if quote_amount > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.quote_vault.to_account_info(),
                    mint: context.accounts.quote_mint.to_account_info(),
                    to: context.accounts.operator_quote.to_account_info(),
                    authority: context.accounts.market_authority.to_account_info(),
                },
                &[authority_seeds],
            ),
            quote_amount,
            context.accounts.quote_mint.decimals,
        )?;
    }

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawInventoryAccountConstraints<'info> {
    #[account(mut)]
    pub operator: Signer<'info>,

    #[account(
        seeds = [MARKET_SEED, market.base_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
        has_one = operator,
        has_one = base_mint,
        has_one = quote_mint,
        has_one = base_vault,
        has_one = quote_vault,
    )]
    pub market: Box<Account<'info, Market>>,

    /// CHECK: PDA authority over both vaults; holds no data, only signs.
    #[account(
        seeds = [AUTHORITY_SEED, market.key().as_ref()],
        bump = market.authority_bump,
    )]
    pub market_authority: UncheckedAccount<'info>,

    pub base_mint: Box<InterfaceAccount<'info, Mint>>,

    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        seeds = [BASE_VAULT_SEED, market.key().as_ref()],
        bump,
    )]
    pub base_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.key().as_ref()],
        bump,
    )]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = base_mint,
        associated_token::authority = operator,
        associated_token::token_program = token_program,
    )]
    pub operator_base: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = operator,
        associated_token::token_program = token_program,
    )]
    pub operator_quote: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
}
