use {
    crate::{
        constants::STATUS_LISTED,
        errors::OptionsError,
        instructions::shared::{add_locked, check_custody, require_valid_kind, Terms},
        state::{Market, OptionContract, OptionContractInner},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

/// The instruction's arguments, bundled so the handler signature stays
/// readable.
#[derive(Clone, Copy)]
pub struct WriteOptionArguments {
    pub id: u64,
    pub kind: u8,
    pub contracts: u64,
    pub underlying_per_contract: u64,
    pub strike_per_contract: u64,
    pub premium: u64,
    pub expiry: i64,
}

#[derive(Accounts)]
#[instruction(id: u64)]
pub struct WriteOptionAccountConstraints {
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
        init,
        payer = writer,
        address = OptionContract::seeds(market.address(), writer.address(), id),
    )]
    pub option: Account<OptionContract>,
    pub underlying_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    #[account(mut)]
    pub underlying_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    /// A call writer pays collateral from this account; a put writer's copy
    /// is only validated. Unlike the Anchor sibling, it must already exist.
    #[account(mut)]
    pub writer_underlying: Account<Token>,
    /// A put writer pays collateral from this account, and every writer is
    /// paid their premium into it by `buy_option`. Must already exist.
    #[account(mut)]
    pub writer_quote: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub rent: Sysvar<Rent>,
}

/// Write a lot of options. The writer posts the entire collateral up front:
/// the underlying for a call, the strike in the quote token for a put. From
/// this moment the vault holds everything a future holder could claim, which
/// is why nothing in this program ever has to be liquidated.
#[inline(always)]
pub fn handle_write_option(
    accounts: &mut WriteOptionAccountConstraints,
    arguments: WriteOptionArguments,
    bumps: &WriteOptionAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require_valid_kind(arguments.kind)?;
    // Every quantity is a multiplier in the settlement math, so a zero in any
    // of them is a lot that delivers nothing or costs nothing to exercise. A
    // zero premium is a gift rather than a sale, and is refused as a mistake.
    require!(
        arguments.contracts > 0
            && arguments.underlying_per_contract > 0
            && arguments.strike_per_contract > 0
            && arguments.premium > 0,
        OptionsError::InvalidParameter
    );
    // Written in words: the holder may exercise while now < expiry. An expiry
    // at or before now would create an option nobody could ever exercise.
    let now: i64 = Clock::get()?.unix_timestamp.into();
    require!(arguments.expiry > now, OptionsError::ExpiryInPast);

    let terms = Terms {
        kind: arguments.kind,
        contracts: arguments.contracts,
        underlying_per_contract: arguments.underlying_per_contract,
        strike_per_contract: arguments.strike_per_contract,
    };
    // Both settlement amounts are computed here, at write time, so a lot
    // whose exercise would overflow is refused before anyone pays for it.
    terms.underlying_total()?;
    terms.strike_total()?;
    let collateral = terms.collateral_amount()?;

    // Effects before the transfer: record the lot and what the vault now owes.
    accounts.option.set_inner(OptionContractInner {
        id: arguments.id,
        market: *accounts.market.address(),
        writer: *accounts.writer.address(),
        holder: Address::default(),
        contracts: arguments.contracts,
        underlying_per_contract: arguments.underlying_per_contract,
        strike_per_contract: arguments.strike_per_contract,
        premium: arguments.premium,
        expiry: arguments.expiry,
        kind: arguments.kind,
        status: STATUS_LISTED,
        bump: bumps.option,
    });

    let mut underlying_after = accounts.underlying_vault.amount();
    let mut quote_after = accounts.quote_vault.amount();
    if terms.is_call() {
        add_locked(
            &mut accounts.market.underlying_locked,
            &mut underlying_after,
            collateral,
        )?;
    } else {
        add_locked(
            &mut accounts.market.quote_locked,
            &mut quote_after,
            collateral,
        )?;
    }
    check_custody(&accounts.market, underlying_after, quote_after)?;

    if terms.is_call() {
        accounts
            .token_program
            .transfer_checked(
                &accounts.writer_underlying,
                &accounts.underlying_mint,
                &accounts.underlying_vault,
                &accounts.writer,
                collateral,
                accounts.underlying_mint.decimals(),
            )
            .invoke()
    } else {
        accounts
            .token_program
            .transfer_checked(
                &accounts.writer_quote,
                &accounts.quote_mint,
                &accounts.quote_vault,
                &accounts.writer,
                collateral,
                accounts.quote_mint.decimals(),
            )
            .invoke()
    }
}
