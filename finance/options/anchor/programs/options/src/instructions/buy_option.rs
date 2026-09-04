use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{MARKET_SEED, OPTION_SEED, QUOTE_VAULT_SEED};
use crate::contract_math;
use crate::errors::OptionsError;
use crate::instructions::shared::{check_custody, transfer_from_signer};
use crate::state::{Market, OptionContract, OptionStatus};

/// Buy a listed lot. The premium is the only money that changes hands: the
/// venue's fee comes out of it into the quote vault, and the rest goes
/// straight to the writer, whose money it is from this moment whatever the
/// holder later does. The collateral does not move.
pub fn handle_buy_option(context: &mut Context<BuyOptionAccountConstraints>) -> Result<()> {
    let option = &mut context.accounts.option;
    require!(
        option.status == OptionStatus::Listed,
        OptionsError::OptionNotListed
    );
    // A lot nobody can exercise any more is not for sale.
    let now = Clock::get()?.unix_timestamp;
    require!(
        contract_math::may_exercise(now, option.expiry),
        OptionsError::OptionExpired
    );

    let market = &mut context.accounts.market;
    let (fee, to_writer) = contract_math::split_premium(option.premium, market.fee_bps)
        .ok_or(OptionsError::MathOverflow)?;

    // Effects before the transfers.
    option.holder = *context.accounts.buyer.address();
    option.status = OptionStatus::Held;
    market.fees_owed = market
        .fees_owed
        .checked_add(fee)
        .ok_or(OptionsError::MathOverflow)?;
    let quote_after = context
        .accounts
        .quote_vault
        .amount()
        .checked_add(fee)
        .ok_or(OptionsError::MathOverflow)?;
    check_custody(market, market.underlying_locked, quote_after)?;

    transfer_from_signer(
        &context.accounts.token_program,
        &mut context.accounts.buyer_quote,
        &context.accounts.quote_mint,
        &mut context.accounts.writer_quote,
        &context.accounts.buyer,
        to_writer,
    )?;
    if fee > 0 {
        transfer_from_signer(
            &context.accounts.token_program,
            &mut context.accounts.buyer_quote,
            &context.accounts.quote_mint,
            &mut context.accounts.quote_vault,
            &context.accounts.buyer,
            fee,
        )?;
    }

    Ok(())
}

#[derive(Accounts)]
pub struct BuyOptionAccountConstraints {
    #[account(mut)]
    pub buyer: Signer,

    /// CHECK: the writer, bound by `address = option.writer`; only used to
    /// derive the token account the premium is paid into.
    #[account(address = option.writer)]
    pub writer: UncheckedAccount,

    #[account(
        mut,
        seeds = [MARKET_SEED, market.underlying_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
        address = option.market,
    )]
    pub market: Box<BorshAccount<Market>>,

    #[account(
        mut,
        seeds = [OPTION_SEED, market.address().as_ref(), writer.address().as_ref(), option.id.to_le_bytes()],
        bump = option.bump,
    )]
    pub option: Box<BorshAccount<OptionContract>>,

    #[account(address = market.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.quote_vault,
    )]
    pub quote_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = buyer,
        associated_token::token_program = token_program,
    )]
    pub buyer_quote: Box<InterfaceAccount<TokenAccount>>,

    // Created by `write_option`, at the writer's expense, so the buyer never
    // pays rent on the writer's behalf. A writer buying their own lot would
    // put this account and `buyer_quote` in two mutable slots at once, which
    // the loader rejects, so a writer cannot pay themselves a premium.
    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = writer,
        associated_token::token_program = token_program,
    )]
    pub writer_quote: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
}
