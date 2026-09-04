use {
    crate::{
        constants::{STATUS_EXERCISED, STATUS_HELD},
        errors::OptionsError,
        instructions::shared::{
            add_locked, check_custody, may_exercise, sub_locked, transfer_from_vault, Terms,
        },
        state::{Market, MarketAuthorityPda, OptionContract},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct ExerciseOptionAccountConstraints {
    #[account(mut)]
    pub holder: Signer,
    /// CHECK: bound to the option via `has_one(writer)`; a seed of the option
    /// PDA.
    pub writer: UncheckedAccount,
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
        has_one(holder),
        address = OptionContract::seeds(market.address(), writer.address(), option.id.into()),
    )]
    pub option: Account<OptionContract>,
    /// Authority PDA over both vaults; holds no data, only signs.
    #[account(address = MarketAuthorityPda::seeds(market.address()))]
    pub market_authority: UncheckedAccount,
    pub underlying_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    #[account(mut)]
    pub underlying_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    /// Unlike the Anchor sibling, both holder accounts must already exist.
    #[account(mut)]
    pub holder_underlying: Account<Token>,
    #[account(mut)]
    pub holder_quote: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

/// Exercise a held lot before expiry. A call holder pays the strike and takes
/// the underlying; a put holder delivers the underlying and takes the strike.
/// The payment stays in the vault, owed to the writer, until they call
/// `collect_proceeds`. No price is read: whether exercising is worth it is
/// the holder's decision.
#[inline(always)]
pub fn handle_exercise_option(
    accounts: &mut ExerciseOptionAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        accounts.option.status == STATUS_HELD,
        OptionsError::OptionNotHeld
    );
    // The holder may exercise while now < expiry.
    let now: i64 = Clock::get()?.unix_timestamp.into();
    require!(
        may_exercise(now, accounts.option.expiry.get()),
        OptionsError::OptionExpired
    );

    let terms = Terms {
        kind: accounts.option.kind,
        contracts: accounts.option.contracts.get(),
        underlying_per_contract: accounts.option.underlying_per_contract.get(),
        strike_per_contract: accounts.option.strike_per_contract.get(),
    };
    let underlying_total = terms.underlying_total()?;
    let strike_total = terms.strike_total()?;

    // Effects: the lot is exercised, and the vault now owes the writer the
    // payment instead of owing the holder the collateral.
    accounts.option.status = STATUS_EXERCISED;
    let mut underlying_after = accounts.underlying_vault.amount();
    let mut quote_after = accounts.quote_vault.amount();
    if terms.is_call() {
        sub_locked(
            &mut accounts.market.underlying_locked,
            &mut underlying_after,
            underlying_total,
        )?;
        add_locked(
            &mut accounts.market.quote_locked,
            &mut quote_after,
            strike_total,
        )?;
    } else {
        sub_locked(
            &mut accounts.market.quote_locked,
            &mut quote_after,
            strike_total,
        )?;
        add_locked(
            &mut accounts.market.underlying_locked,
            &mut underlying_after,
            underlying_total,
        )?;
    }
    check_custody(&accounts.market, underlying_after, quote_after)?;

    // The holder pays in, then the vault pays out, atomically or not at all.
    if terms.is_call() {
        accounts
            .token_program
            .transfer_checked(
                &accounts.holder_quote,
                &accounts.quote_mint,
                &accounts.quote_vault,
                &accounts.holder,
                strike_total,
                accounts.quote_mint.decimals(),
            )
            .invoke()?;
        transfer_from_vault(
            &accounts.token_program,
            &accounts.underlying_vault,
            &accounts.underlying_mint,
            &accounts.holder_underlying,
            &accounts.market_authority,
            &accounts.market,
            underlying_total,
        )
    } else {
        accounts
            .token_program
            .transfer_checked(
                &accounts.holder_underlying,
                &accounts.underlying_mint,
                &accounts.underlying_vault,
                &accounts.holder,
                underlying_total,
                accounts.underlying_mint.decimals(),
            )
            .invoke()?;
        transfer_from_vault(
            &accounts.token_program,
            &accounts.quote_vault,
            &accounts.quote_mint,
            &accounts.holder_quote,
            &accounts.market_authority,
            &accounts.market,
            strike_total,
        )
    }
}
