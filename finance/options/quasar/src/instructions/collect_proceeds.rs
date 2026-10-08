use {
    crate::{
        constants::STATUS_EXERCISED,
        errors::OptionsError,
        instructions::shared::{check_custody, sub_owed, transfer_from_vault, Terms},
        state::{Market, OptionContract},
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CollectProceedsAccountConstraints {
    #[account(mut)]
    pub writer: Signer,
    #[account(
        mut,
        address = Market::seeds(underlying_mint.address(), quote_mint.address()),
        has_one(underlying_vault),
        has_one(quote_vault),
    )]
    pub market: Account<Market>,
    #[account(
        mut,
        has_one(market),
        has_one(writer),
        close(dest = writer),
        address = OptionContract::seeds(market.address(), writer.address(), option.id.into()),
    )]
    pub option: Account<OptionContract>,
    pub underlying_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    #[account(mut)]
    pub underlying_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    /// A put writer is paid in the underlying, which they may never have
    /// held, so the account is created if needed, at the writer's expense.
    #[account(
        mut,
        init(idempotent),
        payer = writer,
        associated_token(mint = underlying_mint, authority = writer, token_program = token_program),
    )]
    pub writer_underlying: Account<Token>,
    #[account(mut)]
    pub writer_quote: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub associated_token_program: Program<AssociatedTokenProgram>,
    pub system_program: Program<SystemProgram>,
}

/// The writer collects what the holder paid at exercise: the strike for a
/// call, the underlying for a put. The option closes.
#[inline(always)]
pub fn handle_collect_proceeds(
    accounts: &mut CollectProceedsAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        accounts.option.status == STATUS_EXERCISED,
        OptionsError::OptionNotExercised
    );
    let terms = Terms {
        kind: accounts.option.kind,
        underlying_amount: accounts.option.underlying_amount.get(),
        strike_amount: accounts.option.strike_amount.get(),
    };
    let proceeds = terms.exercise_payment();

    let mut underlying_after = accounts.underlying_vault.amount();
    let mut quote_after = accounts.quote_vault.amount();
    if terms.is_call() {
        // A call's proceeds are the strike, in the quote token.
        sub_owed(&mut accounts.market.quote_owed, &mut quote_after, proceeds)?;
    } else {
        // A put's proceeds are the delivered underlying.
        sub_owed(
            &mut accounts.market.underlying_owed,
            &mut underlying_after,
            proceeds,
        )?;
    }
    check_custody(&accounts.market, underlying_after, quote_after)?;

    if terms.is_call() {
        transfer_from_vault(
            &accounts.token_program,
            &accounts.quote_vault,
            &accounts.quote_mint,
            &accounts.writer_quote,
            &accounts.market,
            proceeds,
        )
    } else {
        transfer_from_vault(
            &accounts.token_program,
            &accounts.underlying_vault,
            &accounts.underlying_mint,
            &accounts.writer_underlying,
            &accounts.market,
            proceeds,
        )
    }
    // The option closes to the writer through `close(dest = writer)`.
}
