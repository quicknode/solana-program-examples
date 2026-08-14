use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::{BASE_VAULT_SEED, MARKET_SEED, QUOTE_VAULT_SEED};
use crate::errors::PropAmmError;
use crate::state::Market;

pub fn handle_deposit_inventory(
    context: &mut Context<DepositInventoryAccountConstraints>,
    base_amount: u64,
    quote_amount: u64,
) -> Result<()> {
    require!(
        base_amount > 0 || quote_amount > 0,
        PropAmmError::ZeroAmount
    );

    if base_amount > 0 {
        transfer_checked(
            CpiContext::new(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.operator_base.cpi_handle_mut(),
                    mint: context.accounts.base_mint.cpi_handle(),
                    to: context.accounts.base_vault.cpi_handle_mut(),
                    authority: context.accounts.operator.cpi_handle(),
                },
            ),
            base_amount,
            context.accounts.base_mint.decimals(),
        )?;
    }

    if quote_amount > 0 {
        transfer_checked(
            CpiContext::new(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.operator_quote.cpi_handle_mut(),
                    mint: context.accounts.quote_mint.cpi_handle(),
                    to: context.accounts.quote_vault.cpi_handle_mut(),
                    authority: context.accounts.operator.cpi_handle(),
                },
            ),
            quote_amount,
            context.accounts.quote_mint.decimals(),
        )?;
    }

    Ok(())
}

#[derive(Accounts)]
pub struct DepositInventoryAccountConstraints {
    // `address = market.operator` on the operator is the whole access control: only the
    // firm's key can stock the market.
    #[account(mut, address = market.operator)]
    pub operator: Signer,

    #[account(
        seeds = [MARKET_SEED, market.base_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
    )]
    pub market: Box<BorshAccount<Market>>,

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
