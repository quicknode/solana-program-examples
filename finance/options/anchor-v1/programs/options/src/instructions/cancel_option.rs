use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{
    AUTHORITY_SEED, MARKET_SEED, OPTION_SEED, QUOTE_VAULT_SEED, UNDERLYING_VAULT_SEED,
};
use crate::contract_math;
use crate::errors::OptionsError;
use crate::instructions::shared::{check_custody, transfer_from_vault};
use crate::state::{Market, OptionContract, OptionKind, OptionStatus};

/// Withdraw an unsold option. Without this, an option nobody buys would hold the
/// writer's collateral forever. Any time is fine, including after expiry: an
/// unsold option has no holder whose rights could be cut short.
pub fn handle_cancel_option(context: Context<CancelOptionAccountConstraints>) -> Result<()> {
    let option = &context.accounts.option;
    require!(
        option.status == OptionStatus::Listed,
        OptionsError::OptionNotListed
    );

    let collateral = contract_math::collateral_amount(
        option.kind,
        option.contracts,
        option.underlying_per_contract,
        option.strike_per_contract,
    )
    .ok_or(OptionsError::MathOverflow)?;
    let kind = option.kind;

    let market = &mut context.accounts.market;
    let mut underlying_after = context.accounts.underlying_vault.amount;
    let mut quote_after = context.accounts.quote_vault.amount;
    match kind {
        OptionKind::Call => {
            market.underlying_locked = market
                .underlying_locked
                .checked_sub(collateral)
                .ok_or(OptionsError::MathOverflow)?;
            underlying_after = underlying_after
                .checked_sub(collateral)
                .ok_or(OptionsError::CustodyInvariantViolated)?;
        }
        OptionKind::Put => {
            market.quote_locked = market
                .quote_locked
                .checked_sub(collateral)
                .ok_or(OptionsError::MathOverflow)?;
            quote_after = quote_after
                .checked_sub(collateral)
                .ok_or(OptionsError::CustodyInvariantViolated)?;
        }
    }
    check_custody(market, underlying_after, quote_after)?;

    match kind {
        OptionKind::Call => transfer_from_vault(
            &context.accounts.token_program,
            &mut context.accounts.underlying_vault,
            &context.accounts.underlying_mint,
            &mut context.accounts.writer_underlying,
            &context.accounts.market_authority,
            market,
            collateral,
        ),
        OptionKind::Put => transfer_from_vault(
            &context.accounts.token_program,
            &mut context.accounts.quote_vault,
            &context.accounts.quote_mint,
            &mut context.accounts.writer_quote,
            &context.accounts.market_authority,
            market,
            collateral,
        ),
    }
    // The option closes to the writer through `close = writer`.
}

#[derive(Accounts)]
pub struct CancelOptionAccountConstraints<'info> {
    #[account(mut, address = option.writer)]
    pub writer: Signer<'info>,

    #[account(
        mut,
        seeds = [MARKET_SEED, market.underlying_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
        address = option.market,
    )]
    pub market: Box<Account<'info, Market>>,

    #[account(
        mut,
        close = writer,
        seeds = [OPTION_SEED, market.key().as_ref(), writer.key().as_ref(), option.id.to_le_bytes().as_ref()],
        bump = option.bump,
    )]
    pub option: Box<Account<'info, OptionContract>>,

    /// CHECK: PDA authority over both vaults; holds no data, only signs.
    #[account(
        seeds = [AUTHORITY_SEED, market.key().as_ref()],
        bump = market.authority_bump,
    )]
    pub market_authority: UncheckedAccount<'info>,

    #[account(address = market.underlying_mint)]
    pub underlying_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(address = market.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        seeds = [UNDERLYING_VAULT_SEED, market.key().as_ref()],
        bump,
        address = market.underlying_vault,
    )]
    pub underlying_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.key().as_ref()],
        bump,
        address = market.quote_vault,
    )]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = underlying_mint,
        associated_token::authority = writer,
        associated_token::token_program = token_program,
    )]
    pub writer_underlying: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = writer,
        associated_token::token_program = token_program,
    )]
    pub writer_quote: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
}
