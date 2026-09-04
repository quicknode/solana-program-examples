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
use crate::instructions::shared::{check_custody, transfer_from_signer, transfer_from_vault};
use crate::state::{Market, OptionContract, OptionKind, OptionStatus};

/// Exercise a held lot before expiry. A call holder pays the strike in the
/// quote token and takes the underlying; a put holder delivers the underlying
/// and takes the strike. The payment stays in the vault, owed to the writer,
/// until they call `collect_proceeds`; paying the writer directly would let
/// a writer who closed their token account block the exercise.
///
/// No price is read. Whether exercising is worth it is the holder's decision,
/// made against whatever the market is doing offchain; the program only
/// enforces the terms.
pub fn handle_exercise_option(
    context: &mut Context<ExerciseOptionAccountConstraints>,
) -> Result<()> {
    let option = &mut context.accounts.option;
    require!(
        option.status == OptionStatus::Held,
        OptionsError::OptionNotHeld
    );
    // The holder may exercise while now < expiry.
    let now = Clock::get()?.unix_timestamp;
    require!(
        contract_math::may_exercise(now, option.expiry),
        OptionsError::OptionExpired
    );

    let kind = option.kind;
    let underlying_total =
        contract_math::underlying_total(option.contracts, option.underlying_per_contract)
            .ok_or(OptionsError::MathOverflow)?;
    let strike_total = contract_math::strike_total(option.contracts, option.strike_per_contract)
        .ok_or(OptionsError::MathOverflow)?;

    // Effects: the lot is exercised, and the vault now owes the writer the
    // payment instead of owing the holder the collateral.
    option.status = OptionStatus::Exercised;

    let market = &mut context.accounts.market;
    let underlying_before = context.accounts.underlying_vault.amount();
    let quote_before = context.accounts.quote_vault.amount();
    let (underlying_after, quote_after) = match kind {
        OptionKind::Call => {
            market.underlying_locked = market
                .underlying_locked
                .checked_sub(underlying_total)
                .ok_or(OptionsError::MathOverflow)?;
            market.quote_locked = market
                .quote_locked
                .checked_add(strike_total)
                .ok_or(OptionsError::MathOverflow)?;
            (
                underlying_before
                    .checked_sub(underlying_total)
                    .ok_or(OptionsError::CustodyInvariantViolated)?,
                quote_before
                    .checked_add(strike_total)
                    .ok_or(OptionsError::MathOverflow)?,
            )
        }
        OptionKind::Put => {
            market.quote_locked = market
                .quote_locked
                .checked_sub(strike_total)
                .ok_or(OptionsError::MathOverflow)?;
            market.underlying_locked = market
                .underlying_locked
                .checked_add(underlying_total)
                .ok_or(OptionsError::MathOverflow)?;
            (
                underlying_before
                    .checked_add(underlying_total)
                    .ok_or(OptionsError::MathOverflow)?,
                quote_before
                    .checked_sub(strike_total)
                    .ok_or(OptionsError::CustodyInvariantViolated)?,
            )
        }
    };
    check_custody(market, underlying_after, quote_after)?;

    // The holder pays in, then the vault pays out, atomically or not at all.
    match kind {
        OptionKind::Call => {
            transfer_from_signer(
                &context.accounts.token_program,
                &mut context.accounts.holder_quote,
                &context.accounts.quote_mint,
                &mut context.accounts.quote_vault,
                &context.accounts.holder,
                strike_total,
            )?;
            transfer_from_vault(
                &context.accounts.token_program,
                &mut context.accounts.underlying_vault,
                &context.accounts.underlying_mint,
                &mut context.accounts.holder_underlying,
                &context.accounts.market_authority,
                market,
                underlying_total,
            )
        }
        OptionKind::Put => {
            transfer_from_signer(
                &context.accounts.token_program,
                &mut context.accounts.holder_underlying,
                &context.accounts.underlying_mint,
                &mut context.accounts.underlying_vault,
                &context.accounts.holder,
                underlying_total,
            )?;
            transfer_from_vault(
                &context.accounts.token_program,
                &mut context.accounts.quote_vault,
                &context.accounts.quote_mint,
                &mut context.accounts.holder_quote,
                &context.accounts.market_authority,
                market,
                strike_total,
            )
        }
    }
}

#[derive(Accounts)]
pub struct ExerciseOptionAccountConstraints {
    #[account(mut, address = option.holder)]
    pub holder: Signer,

    /// CHECK: the writer, bound by `address = option.writer`; a seed of the
    /// option PDA.
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

    // A call holder receives into this account and may never have held the
    // underlying before, so it is created if needed, at the holder's expense.
    #[account(
        init_if_needed,
        payer = holder,
        associated_token::mint = underlying_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_underlying: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = holder,
        associated_token::mint = quote_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_quote: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
