use {
    crate::{
        constants::STATUS_HELD,
        errors::OptionsError,
        instructions::shared::{
            check_custody, may_reclaim, sub_locked, transfer_from_vault, Terms,
        },
        state::{Market, MarketAuthorityPda, OptionContract},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct ReclaimCollateralAccountConstraints {
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
    /// Authority PDA over both vaults; holds no data, only signs.
    #[account(address = MarketAuthorityPda::seeds(market.address()))]
    pub market_authority: UncheckedAccount,
    pub underlying_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    #[account(mut)]
    pub underlying_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    #[account(mut)]
    pub writer_underlying: Account<Token>,
    #[account(mut)]
    pub writer_quote: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

/// The holder let the option expire, so the writer takes the collateral
/// back. The premium was the writer's the moment it was paid. The option
/// account closes.
#[inline(always)]
pub fn handle_reclaim_collateral(
    accounts: &mut ReclaimCollateralAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        accounts.option.status == STATUS_HELD,
        OptionsError::OptionNotHeld
    );
    // The writer may reclaim once now >= expiry: the complement of the
    // holder's window, so the two can never both claim the collateral.
    let now: i64 = Clock::get()?.unix_timestamp.into();
    require!(
        may_reclaim(now, accounts.option.expiry.get()),
        OptionsError::OptionNotExpired
    );

    let terms = Terms {
        kind: accounts.option.kind,
        contracts: accounts.option.contracts.get(),
        underlying_per_contract: accounts.option.underlying_per_contract.get(),
        strike_per_contract: accounts.option.strike_per_contract.get(),
    };
    let collateral = terms.collateral_amount()?;

    let mut underlying_after = accounts.underlying_vault.amount();
    let mut quote_after = accounts.quote_vault.amount();
    if terms.is_call() {
        sub_locked(
            &mut accounts.market.underlying_locked,
            &mut underlying_after,
            collateral,
        )?;
    } else {
        sub_locked(
            &mut accounts.market.quote_locked,
            &mut quote_after,
            collateral,
        )?;
    }
    check_custody(&accounts.market, underlying_after, quote_after)?;

    if terms.is_call() {
        transfer_from_vault(
            &accounts.token_program,
            &accounts.underlying_vault,
            &accounts.underlying_mint,
            &accounts.writer_underlying,
            &accounts.market_authority,
            &accounts.market,
            collateral,
        )
    } else {
        transfer_from_vault(
            &accounts.token_program,
            &accounts.quote_vault,
            &accounts.quote_mint,
            &accounts.writer_quote,
            &accounts.market_authority,
            &accounts.market,
            collateral,
        )
    }
    // The option account closes to the writer through `close(dest = writer)`.
}
