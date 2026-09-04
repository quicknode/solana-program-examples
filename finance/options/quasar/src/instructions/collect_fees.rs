use {
    crate::{
        errors::OptionsError,
        instructions::shared::{check_custody, transfer_from_vault},
        state::{Market, MarketAuthorityPda},
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CollectFeesAccountConstraints {
    #[account(mut)]
    pub admin: Signer,
    #[account(
        mut,
        address = Market::seeds(underlying_mint.address(), quote_mint.address()),
        has_one(admin),
        has_one(underlying_vault),
        has_one(quote_vault),
    )]
    pub market: Account<Market>,
    /// Authority PDA over both vaults; holds no data, only signs.
    #[account(address = MarketAuthorityPda::seeds(market.address()))]
    pub market_authority: UncheckedAccount,
    /// CHECK: seed input for the market PDA.
    pub underlying_mint: UncheckedAccount,
    pub quote_mint: Account<Mint>,
    /// Read only, for the custody check: the invariant covers both vaults.
    pub underlying_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    /// Unlike the Anchor sibling, it must already exist.
    #[account(mut)]
    pub admin_quote: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

/// The admin sweeps the fees the venue has earned on premiums. `fees_owed`
/// is the only part of the quote vault the admin can reach.
#[inline(always)]
pub fn handle_collect_fees(
    accounts: &mut CollectFeesAccountConstraints,
) -> Result<(), ProgramError> {
    let amount = accounts.market.fees_owed.get();
    require!(amount > 0, OptionsError::NothingToCollect);

    // Effects before the transfer: zero the balance, then pay it out.
    accounts.market.fees_owed.set(0);
    let quote_after = accounts
        .quote_vault
        .amount()
        .checked_sub(amount)
        .ok_or(OptionsError::CustodyInvariantViolated)?;
    check_custody(
        &accounts.market,
        accounts.underlying_vault.amount(),
        quote_after,
    )?;

    transfer_from_vault(
        &accounts.token_program,
        &accounts.quote_vault,
        &accounts.quote_mint,
        &accounts.admin_quote,
        &accounts.market_authority,
        &accounts.market,
        amount,
    )
}
