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
    context: &mut Context<WithdrawInventoryAccountConstraints>,
    base_amount: u64,
    quote_amount: u64,
) -> Result<()> {
    require!(
        base_amount > 0 || quote_amount > 0,
        PropAmmError::ZeroAmount
    );
    require!(
        base_amount <= context.accounts.base_vault.amount(),
        PropAmmError::InsufficientInventory
    );
    require!(
        quote_amount <= context.accounts.quote_vault.amount(),
        PropAmmError::InsufficientInventory
    );

    let market = &context.accounts.market;
    let market_key = market.address();
    let authority_seeds: &[&[u8]] = &[
        AUTHORITY_SEED,
        market_key.as_ref(),
        &[market.authority_bump],
    ];

    if base_amount > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.base_vault.to_cpi_handle_mut(),
                    mint: context.accounts.base_mint.to_cpi_handle(),
                    to: context.accounts.operator_base.to_cpi_handle_mut(),
                    authority: context.accounts.market_authority.cpi_handle(),
                },
                &[authority_seeds],
            ),
            base_amount,
            context.accounts.base_mint.decimals(),
        )?;
    }

    if quote_amount > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.quote_vault.to_cpi_handle_mut(),
                    mint: context.accounts.quote_mint.to_cpi_handle(),
                    to: context.accounts.operator_quote.to_cpi_handle_mut(),
                    authority: context.accounts.market_authority.cpi_handle(),
                },
                &[authority_seeds],
            ),
            quote_amount,
            context.accounts.quote_mint.decimals(),
        )?;
    }

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawInventoryAccountConstraints {
    #[account(mut, address = market.operator)]
    pub operator: Signer,

    #[account(
        seeds = [MARKET_SEED, market.base_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
    )]
    pub market: Box<BorshAccount<Market>>,

    /// CHECK: PDA authority over both vaults; holds no data, only signs.
    #[account(
        seeds = [AUTHORITY_SEED, market.address().as_ref()],
        bump = market.authority_bump,
    )]
    pub market_authority: UncheckedAccount,

    #[account(address = market.base_mint)]
    pub base_mint: Box<InterfaceAccount<Mint>>,

    #[account(address = market.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [BASE_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.base_vault,
    )]
    pub base_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.quote_vault,
    )]
    pub quote_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = base_mint,
        associated_token::authority = operator,
        associated_token::token_program = token_program,
    )]
    pub operator_base: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = operator,
        associated_token::token_program = token_program,
    )]
    pub operator_quote: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
}
