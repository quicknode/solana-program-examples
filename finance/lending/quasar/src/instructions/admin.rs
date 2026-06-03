use {
    crate::{
        constants::{MINT_SPACE, TOKEN_ACCOUNT_SPACE},
        error::LendingError,
        logic::now,
        math::validate_config,
        state::{
            LendingMarket, LendingMarketInner, LiquidityVaultPda, PriceFeed, PriceFeedInner,
            Reserve, ReserveInner, ShareMintPda,
        },
    },
    quasar_lang::{prelude::*, sysvars::Sysvar},
    quasar_spl::{initialize_account3, initialize_mint2, prelude::*},
};

// ---------------------------------------------------------------------------
// init_lending_market
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitLendingMarket {
    #[account(mut)]
    pub owner: Signer,
    #[account(init, payer = owner, address = LendingMarket::seeds(owner.address()))]
    pub lending_market: Account<LendingMarket>,
    pub quote_mint: Account<Mint>,
    pub system_program: Program<SystemProgram>,
}

impl InitLendingMarket {
    #[inline(always)]
    pub fn run(&mut self, bumps: &InitLendingMarketBumps) -> Result<(), ProgramError> {
        self.lending_market.set_inner(LendingMarketInner {
            owner: *self.owner.address(),
            quote_mint: *self.quote_mint.address(),
            bump: bumps.lending_market,
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// init_reserve
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitReserve {
    #[account(mut)]
    pub owner: Signer,
    #[account(has_one(owner), address = LendingMarket::seeds(owner.address()))]
    pub lending_market: Account<LendingMarket>,
    #[account(init, payer = owner, address = Reserve::seeds(lending_market.address(), liquidity_mint.address()))]
    pub reserve: Account<Reserve>,
    pub liquidity_mint: Account<Mint>,
    /// Created and initialized as a token account (authority = reserve) in the handler.
    #[account(mut, address = LiquidityVaultPda::seeds(reserve.address()))]
    pub liquidity_vault: UncheckedAccount,
    /// Created and initialized as a share-token mint (authority = reserve) in the handler.
    #[account(mut, address = ShareMintPda::seeds(reserve.address()))]
    pub share_mint: UncheckedAccount,
    #[account(address = PriceFeed::seeds(liquidity_mint.address()))]
    pub price_feed: Account<PriceFeed>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

impl InitReserve {
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        loan_to_value_bps: u16,
        liquidation_threshold_bps: u16,
        liquidation_bonus_bps: u16,
        close_factor_bps: u16,
        optimal_utilization_bps: u16,
        min_borrow_rate_bps: u16,
        optimal_borrow_rate_bps: u16,
        max_borrow_rate_bps: u16,
        bumps: &InitReserveBumps,
    ) -> Result<(), ProgramError> {
        validate_config(
            loan_to_value_bps,
            liquidation_threshold_bps,
            liquidation_bonus_bps,
            close_factor_bps,
            optimal_utilization_bps,
            min_borrow_rate_bps,
            optimal_borrow_rate_bps,
            max_borrow_rate_bps,
        )?;

        let reserve_address = *self.reserve.address();
        let decimals = self.liquidity_mint.decimals;
        let rent = Rent::get()?;

        // Create the program-owned liquidity vault PDA, then initialize it as a
        // token account whose authority is the reserve PDA.
        let vault_bump = [bumps.liquidity_vault];
        let vault_seeds = [
            Seed::from(crate::constants::LIQUIDITY_VAULT_SEED),
            Seed::from(reserve_address.as_ref()),
            Seed::from(vault_bump.as_ref()),
        ];
        self.system_program
            .create_account(
                &self.owner,
                &self.liquidity_vault,
                rent.minimum_balance_unchecked(TOKEN_ACCOUNT_SPACE as usize),
                TOKEN_ACCOUNT_SPACE,
                self.token_program.address(),
            )
            .invoke_signed(&vault_seeds)?;
        initialize_account3(
            self.token_program.to_account_view(),
            self.liquidity_vault.to_account_view(),
            self.liquidity_mint.to_account_view(),
            &reserve_address,
        )
        .invoke()?;

        // Create the share-token mint PDA (authority = reserve, same decimals).
        let mint_bump = [bumps.share_mint];
        let mint_seeds = [
            Seed::from(crate::constants::SHARE_MINT_SEED),
            Seed::from(reserve_address.as_ref()),
            Seed::from(mint_bump.as_ref()),
        ];
        self.system_program
            .create_account(
                &self.owner,
                &self.share_mint,
                rent.minimum_balance_unchecked(MINT_SPACE as usize),
                MINT_SPACE,
                self.token_program.address(),
            )
            .invoke_signed(&mint_seeds)?;
        initialize_mint2(
            self.token_program.to_account_view(),
            self.share_mint.to_account_view(),
            decimals,
            &reserve_address,
            None,
        )
        .invoke()?;

        self.reserve.set_inner(ReserveInner {
            lending_market: *self.lending_market.address(),
            liquidity_mint: *self.liquidity_mint.address(),
            liquidity_vault: *self.liquidity_vault.address(),
            share_mint: *self.share_mint.address(),
            price_feed: *self.price_feed.address(),
            available_liquidity: 0,
            share_mint_supply: 0,
            borrowed_amount_scaled: 0,
            cumulative_borrow_rate_index: crate::constants::FIXED_POINT_SCALE,
            last_update_slot: now()?,
            liquidity_decimals: decimals,
            loan_to_value_bps,
            liquidation_threshold_bps,
            liquidation_bonus_bps,
            close_factor_bps,
            optimal_utilization_bps,
            min_borrow_rate_bps,
            optimal_borrow_rate_bps,
            max_borrow_rate_bps,
            bump: bumps.reserve,
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// set_price (Switchboard stand-in for tests)
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct SetPrice {
    #[account(mut)]
    pub authority: Signer,
    #[account(init(idempotent), payer = authority, address = PriceFeed::seeds(mint.address()))]
    pub price_feed: Account<PriceFeed>,
    pub mint: Account<Mint>,
    pub system_program: Program<SystemProgram>,
}

impl SetPrice {
    #[inline(always)]
    pub fn run(
        &mut self,
        price_mantissa: i128,
        exponent: i32,
        bumps: &SetPriceBumps,
    ) -> Result<(), ProgramError> {
        // On first creation the stored authority is the zero address; claim it.
        // Afterwards only that authority may update the feed.
        let existing = self.price_feed.authority;
        if existing != Address::default() {
            require_keys_eq!(existing, *self.authority.address(), LendingError::InvalidConfig);
        }
        self.price_feed.set_inner(PriceFeedInner {
            mint: *self.mint.address(),
            price_mantissa,
            exponent,
            last_updated_slot: now()?,
            authority: *self.authority.address(),
            bump: bumps.price_feed,
        });
        Ok(())
    }
}
