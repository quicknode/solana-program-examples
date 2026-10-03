use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::{MARKET_SEED, OPTION_SEED, QUOTE_VAULT_SEED, UNDERLYING_VAULT_SEED};
use crate::contract_math;
use crate::errors::OptionsError;
use crate::instructions::shared::{check_custody, transfer_from_signer};
use crate::state::{Market, OptionContract, OptionKind, OptionStatus};

/// The terms of an option, chosen by the writer. Bundled into one struct so the
/// instruction signature stays readable.
#[derive(Clone, Copy, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct OptionTerms {
    pub kind: OptionKind,

    /// Underlying minor units the option covers: what a call writer posts and
    /// a call holder receives, or a put holder delivers (1 NVDAx = 1_000_000).
    pub underlying_amount: u64,

    /// Quote minor units paid for the underlying on exercise: what a put
    /// writer posts and a put holder receives, or a call holder pays. The
    /// strike for the whole option, as an amount rather than a price.
    pub strike_amount: u64,

    /// Quote minor units the buyer pays the writer for the whole option.
    pub premium: u64,

    /// Unix timestamp after which the holder can no longer exercise.
    pub expiry: i64,
}

/// Write an option. The writer posts the entire collateral up front:
/// the underlying for a call, the strike in the quote token for a put. From
/// this moment the vault holds everything a future holder could claim, which
/// is why nothing in this program ever has to be liquidated.
pub fn handle_write_option(
    context: &mut Context<WriteOptionAccountConstraints>,
    id: u64,
    terms: OptionTerms,
) -> Result<()> {
    let OptionTerms {
        kind,
        underlying_amount,
        strike_amount,
        premium,
        expiry,
    } = terms;
    // A zero amount is an option that delivers nothing or costs nothing to
    // exercise. A zero premium is a gift rather than a sale, and is refused as
    // a mistake.
    require!(
        underlying_amount > 0 && strike_amount > 0 && premium > 0,
        OptionsError::InvalidParameter
    );
    // Written in words: the holder may exercise while now < expiry. An expiry
    // at or before now would create an option nobody could ever exercise.
    let now = Clock::get()?.unix_timestamp;
    require!(expiry > now, OptionsError::ExpiryInPast);

    let collateral = contract_math::collateral_amount(kind, underlying_amount, strike_amount);

    // Effects before the transfer: record the option and what the vault now owes.
    let option = &mut context.accounts.option;
    option.id = id;
    option.market = *context.accounts.market.address();
    option.writer = *context.accounts.writer.address();
    option.holder = Address::default();
    option.kind = kind;
    option.status = OptionStatus::Listed;
    option.underlying_amount = underlying_amount;
    option.strike_amount = strike_amount;
    option.premium = premium;
    option.expiry = expiry;
    option.bump = context.bumps.option;

    let market = &mut context.accounts.market;
    let mut underlying_after = context.accounts.underlying_vault.amount();
    let mut quote_after = context.accounts.quote_vault.amount();
    match kind {
        OptionKind::Call => {
            market.underlying_owed = market
                .underlying_owed
                .checked_add(collateral)
                .ok_or(OptionsError::MathOverflow)?;
            underlying_after = underlying_after
                .checked_add(collateral)
                .ok_or(OptionsError::MathOverflow)?;
        }
        OptionKind::Put => {
            market.quote_owed = market
                .quote_owed
                .checked_add(collateral)
                .ok_or(OptionsError::MathOverflow)?;
            quote_after = quote_after
                .checked_add(collateral)
                .ok_or(OptionsError::MathOverflow)?;
        }
    }
    check_custody(market, underlying_after, quote_after)?;

    match kind {
        OptionKind::Call => transfer_from_signer(
            &context.accounts.token_program,
            &mut context.accounts.writer_underlying,
            &context.accounts.underlying_mint,
            &mut context.accounts.underlying_vault,
            &context.accounts.writer,
            collateral,
        ),
        OptionKind::Put => transfer_from_signer(
            &context.accounts.token_program,
            &mut context.accounts.writer_quote,
            &context.accounts.quote_mint,
            &mut context.accounts.quote_vault,
            &context.accounts.writer,
            collateral,
        ),
    }
}

#[derive(Accounts)]
// The leading underscore is for rustc: `#[derive(Accounts)]` expands
// `_id` into a path that never reads it, so the plain name warns as
// unused. The `seeds` expression below is the real use.
#[instruction(_id: u64)]
pub struct WriteOptionAccountConstraints {
    #[account(mut)]
    pub writer: Signer,

    #[account(
        mut,
        seeds = [MARKET_SEED, market.underlying_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
    )]
    pub market: Box<BorshAccount<Market>>,

    #[account(
        init,
        payer = writer,
        space = OptionContract::DISCRIMINATOR.len() + OptionContract::INIT_SPACE,
        seeds = [OPTION_SEED, market.address().as_ref(), writer.address().as_ref(), _id.to_le_bytes()],
        bump,
    )]
    pub option: Box<BorshAccount<OptionContract>>,

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

    // A call writer pays collateral from this account; a put writer's copy
    // is only validated.
    #[account(
        mut,
        associated_token::mint = underlying_mint,
        associated_token::authority = writer,
        associated_token::token_program = token_program,
    )]
    pub writer_underlying: Box<InterfaceAccount<TokenAccount>>,

    // A put writer pays collateral from this account, and every writer is
    // paid their premium into it by `buy_option`, which requires it to exist.
    // Created here if needed, paid for by the writer, so the party who chose
    // to list carries the rent rather than the buyer.
    #[account(
        init_if_needed,
        payer = writer,
        associated_token::mint = quote_mint,
        associated_token::authority = writer,
        associated_token::token_program = token_program,
    )]
    pub writer_quote: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
