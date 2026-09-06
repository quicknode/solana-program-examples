use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::{
    AUTHORITY_SEED, MARKET_SEED, OPTION_SEED, QUOTE_VAULT_SEED, UNDERLYING_VAULT_SEED,
};
use crate::contract_math;
use crate::errors::OptionsError;
use crate::instructions::shared::{check_custody, transfer_from_vault};
use crate::state::{Market, OptionContract, OptionKind, OptionStatus};

/// The writer collects what the holder paid at exercise: the strike for a
/// call, the underlying for a put. The option closes, rent back to
/// the writer who paid it.
pub fn handle_collect_proceeds(
    context: &mut Context<CollectProceedsAccountConstraints>,
) -> Result<()> {
    let option = &context.accounts.option;
    require!(
        option.status == OptionStatus::Exercised,
        OptionsError::OptionNotExercised
    );

    let kind = option.kind;
    let proceeds = contract_math::exercise_payment(
        kind,
        option.contracts,
        option.underlying_per_contract,
        option.strike_per_contract,
    )
    .ok_or(OptionsError::MathOverflow)?;

    let market = &mut context.accounts.market;
    let mut underlying_after = context.accounts.underlying_vault.amount();
    let mut quote_after = context.accounts.quote_vault.amount();
    match kind {
        // A call's proceeds are the strike, in the quote token.
        OptionKind::Call => {
            market.quote_locked = market
                .quote_locked
                .checked_sub(proceeds)
                .ok_or(OptionsError::MathOverflow)?;
            quote_after = quote_after
                .checked_sub(proceeds)
                .ok_or(OptionsError::CustodyInvariantViolated)?;
        }
        // A put's proceeds are the delivered underlying.
        OptionKind::Put => {
            market.underlying_locked = market
                .underlying_locked
                .checked_sub(proceeds)
                .ok_or(OptionsError::MathOverflow)?;
            underlying_after = underlying_after
                .checked_sub(proceeds)
                .ok_or(OptionsError::CustodyInvariantViolated)?;
        }
    }
    check_custody(market, underlying_after, quote_after)?;

    match kind {
        OptionKind::Call => transfer_from_vault(
            &context.accounts.token_program,
            &mut context.accounts.quote_vault,
            &context.accounts.quote_mint,
            &mut context.accounts.writer_quote,
            &context.accounts.market_authority,
            market,
            proceeds,
        ),
        OptionKind::Put => transfer_from_vault(
            &context.accounts.token_program,
            &mut context.accounts.underlying_vault,
            &context.accounts.underlying_mint,
            &mut context.accounts.writer_underlying,
            &context.accounts.market_authority,
            market,
            proceeds,
        ),
    }
    // The option closes to the writer through `close = writer`.
}

#[derive(Accounts)]
pub struct CollectProceedsAccountConstraints {
    #[account(mut, address = option.writer)]
    pub writer: Signer,

    #[account(
        mut,
        seeds = [MARKET_SEED, market.underlying_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
        address = option.market,
    )]
    pub market: Box<BorshAccount<Market>>,

    #[account(
        mut,
        close = writer,
        seeds = [OPTION_SEED, market.address().as_ref(), writer.address().as_ref(), option.id.to_le_bytes()],
        bump = option.bump,
    )]
    pub option: Box<BorshAccount<OptionContract>>,

    /// CHECK: PDA authority over both vaults; holds no data, only signs.
    #[account(
        seeds = [AUTHORITY_SEED, market.address().as_ref()],
        bump = market.authority_bump,
    )]
    pub market_authority: UncheckedAccount,

    #[account(address = market.underlying_mint)]
    pub underlying_mint: Box<InterfaceAccount<Mint>>,

    #[account(address = market.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [UNDERLYING_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.underlying_vault,
    )]
    pub underlying_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.quote_vault,
    )]
    pub quote_vault: Box<InterfaceAccount<TokenAccount>>,

    // A put writer is paid in the underlying, which they may never have held,
    // so the account is created if needed, at the writer's expense.
    #[account(
        init_if_needed,
        payer = writer,
        associated_token::mint = underlying_mint,
        associated_token::authority = writer,
        associated_token::token_program = token_program,
    )]
    pub writer_underlying: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = writer,
        associated_token::token_program = token_program,
    )]
    pub writer_quote: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
