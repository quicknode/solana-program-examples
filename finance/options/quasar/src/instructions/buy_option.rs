use {
    crate::{
        constants::{STATUS_HELD, STATUS_LISTED},
        errors::OptionsError,
        instructions::shared::{add_locked, check_custody, may_exercise, split_premium},
        state::{Market, OptionContract},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct BuyOptionAccountConstraints {
    #[account(mut)]
    pub buyer: Signer,
    /// CHECK: bound to the option via `has_one(writer)`; a seed of the option
    /// PDA and the owner of the account the premium is paid into.
    pub writer: UncheckedAccount,
    #[account(
        mut,
        address = Market::seeds(underlying_mint.address(), quote_mint.address()),
        has_one(quote_vault),
    )]
    pub market: Account<Market>,
    #[account(
        mut,
        has_one(market),
        has_one(writer),
        address = OptionContract::seeds(market.address(), writer.address(), option.id.into()),
    )]
    pub option: Account<OptionContract>,
    /// CHECK: seed input for the market PDA.
    pub underlying_mint: UncheckedAccount,
    pub quote_mint: Account<Mint>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    #[account(mut)]
    pub buyer_quote: Account<Token>,
    /// Must be the writer's quote-token account: bound below in the handler
    /// against the token account's owner, so a buyer cannot route the
    /// premium to themselves.
    #[account(mut)]
    pub writer_quote: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

/// Buy a listed lot. The premium is the only money that changes hands: the
/// venue's fee comes out of it into the quote vault, and the rest goes
/// straight to the writer. The collateral does not move.
#[inline(always)]
pub fn handle_buy_option(accounts: &mut BuyOptionAccountConstraints) -> Result<(), ProgramError> {
    require!(
        accounts.option.status == STATUS_LISTED,
        OptionsError::OptionNotListed
    );
    // A lot nobody can exercise any more is not for sale.
    let now: i64 = Clock::get()?.unix_timestamp.into();
    require!(
        may_exercise(now, accounts.option.expiry.get()),
        OptionsError::OptionExpired
    );
    // The premium goes to the writer's own quote account, nowhere else.
    require!(
        accounts.writer_quote.owner() == accounts.writer.address()
            && accounts.writer_quote.mint() == accounts.quote_mint.address(),
        OptionsError::InvalidParameter
    );
    // A writer cannot buy their own lot: the same address would sit in the
    // `buyer` and `writer` slots at once, which the runtime refuses before
    // this handler runs.

    let (fee, to_writer) =
        split_premium(accounts.option.premium.get(), accounts.market.fee_bps.get())?;

    // Effects before the transfers.
    accounts.option.holder = *accounts.buyer.address();
    accounts.option.status = STATUS_HELD;
    let mut quote_after = accounts.quote_vault.amount();
    add_locked(&mut accounts.market.fees_owed, &mut quote_after, fee)?;
    let underlying_after = accounts.market.underlying_locked.get();
    check_custody(&accounts.market, underlying_after, quote_after)?;

    accounts
        .token_program
        .transfer_checked(
            &accounts.buyer_quote,
            &accounts.quote_mint,
            &accounts.writer_quote,
            &accounts.buyer,
            to_writer,
            accounts.quote_mint.decimals(),
        )
        .invoke()?;
    if fee > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.buyer_quote,
                &accounts.quote_mint,
                &accounts.quote_vault,
                &accounts.buyer,
                fee,
                accounts.quote_mint.decimals(),
            )
            .invoke()?;
    }
    Ok(())
}
