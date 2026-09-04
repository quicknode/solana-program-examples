use {
    crate::{
        constants::BASIS_POINTS_DENOMINATOR,
        errors::OptionsError,
        state::{Market, MarketAuthorityPda, MarketInner, QuoteVaultPda, UnderlyingVaultPda},
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct InitializeMarketAccountConstraints {
    #[account(mut)]
    pub admin: Signer,
    // One venue per pair, so every option on NVDAx settled in USDC shares the
    // two vaults and the one fee schedule.
    #[account(
        mut,
        init,
        payer = admin,
        address = Market::seeds(underlying_mint.address(), quote_mint.address()),
    )]
    pub market: Account<Market>,
    pub underlying_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    /// Authority PDA over both vaults. Holds no data; only signs.
    #[account(address = MarketAuthorityPda::seeds(market.address()))]
    pub market_authority: UncheckedAccount,
    #[account(
        mut,
        init(idempotent),
        payer = admin,
        address = UnderlyingVaultPda::seeds(market.address()),
        token(mint = underlying_mint, authority = market_authority, token_program = token_program),
    )]
    pub underlying_vault: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = admin,
        address = QuoteVaultPda::seeds(market.address()),
        token(mint = quote_mint, authority = market_authority, token_program = token_program),
    )]
    pub quote_vault: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
pub fn handle_initialize_market(
    accounts: &mut InitializeMarketAccountConstraints,
    fee_bps: u16,
    bumps: &InitializeMarketAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    // Options on a token settled in the same token are meaningless.
    require!(
        accounts.underlying_mint.address() != accounts.quote_mint.address(),
        OptionsError::InvalidParameter
    );
    // A fee of 100% or more would leave the writer nothing from a sale. Zero
    // is allowed: a venue run at cost is a valid choice.
    require!(
        fee_bps < BASIS_POINTS_DENOMINATOR as u16,
        OptionsError::InvalidParameter
    );

    accounts.market.set_inner(MarketInner {
        admin: *accounts.admin.address(),
        underlying_mint: *accounts.underlying_mint.address(),
        quote_mint: *accounts.quote_mint.address(),
        underlying_vault: *accounts.underlying_vault.address(),
        quote_vault: *accounts.quote_vault.address(),
        underlying_locked: 0,
        quote_locked: 0,
        fees_owed: 0,
        fee_bps,
        bump: bumps.market,
        authority_bump: bumps.market_authority,
    });
    Ok(())
}
