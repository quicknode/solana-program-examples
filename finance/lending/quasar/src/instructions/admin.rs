use {
    crate::{
        constants::{MINT_SPACE, TOKEN_ACCOUNT_SPACE},
        error::LendingError,
        instructions::supply::reserve_seeds,
        logic::{accrue, now, snapshot_reserve},
        math::validate_config,
        state::{
            LendingMarket, LendingMarketInner, LiquidityVaultPda, PriceFeed, PriceFeedInner,
            Reserve, ReserveInner, ShareMintPda,
        },
    },
    quasar_lang::{cpi::Seed, prelude::*, sysvars::Sysvar},
    quasar_spl::prelude::*,
};

// ---------------------------------------------------------------------------
// initialize_lending_market
// ---------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(market_id: u64)]
pub struct InitializeLendingMarket {
    #[account(mut)]
    pub owner: Signer,
    // Seeded by `market_id` alone — owner is stored for auth, not in the address.
    #[account(init, payer = owner, address = LendingMarket::seeds(market_id))]
    pub lending_market: Account<LendingMarket>,
    pub quote_mint: Account<Mint>,
    pub system_program: Program<SystemProgram>,
}

impl InitializeLendingMarket {
    #[inline(always)]
    pub fn run(&mut self, market_id: u64, bumps: &InitializeLendingMarketBumps) -> Result<(), ProgramError> {
        self.lending_market.set_inner(LendingMarketInner {
            owner: *self.owner.address(),
            market_id,
            quote_mint: *self.quote_mint.address(),
            bump: bumps.lending_market,
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// initialize_reserve
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeReserve {
    #[account(mut)]
    pub owner: Signer,
    #[account(has_one(owner))]
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
    // Bound to this market's feed for this mint (seeds: market + mint).
    #[account(address = PriceFeed::seeds(lending_market.address(), liquidity_mint.address()))]
    pub price_feed: Account<PriceFeed>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

impl InitializeReserve {
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        loan_to_value_bps: u16,
        liquidation_threshold_bps: u16,
        liquidation_bonus_bps: u16,
        close_factor_bps: u16,
        reserve_factor_bps: u16,
        optimal_utilization_bps: u16,
        min_borrow_rate_bps: u16,
        optimal_borrow_rate_bps: u16,
        max_borrow_rate_bps: u16,
        slots_per_year: u64,
        bumps: &InitializeReserveBumps,
    ) -> Result<(), ProgramError> {
        validate_config(
            loan_to_value_bps,
            liquidation_threshold_bps,
            liquidation_bonus_bps,
            close_factor_bps,
            reserve_factor_bps,
            optimal_utilization_bps,
            min_borrow_rate_bps,
            optimal_borrow_rate_bps,
            max_borrow_rate_bps,
            slots_per_year,
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
        self.token_program
            .initialize_account3(&self.liquidity_vault, &self.liquidity_mint, &reserve_address)
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
        self.token_program
            .initialize_mint2(&self.share_mint, decimals, &reserve_address, None)
            .invoke()?;

        self.reserve.set_inner(ReserveInner {
            lending_market: *self.lending_market.address(),
            liquidity_mint: *self.liquidity_mint.address(),
            liquidity_vault: *self.liquidity_vault.address(),
            share_mint: *self.share_mint.address(),
            price_feed: *self.price_feed.address(),
            available_liquidity: 0,
            share_mint_supply: 0,
            accumulated_protocol_fees: 0,
            borrowed_principal: 0,
            borrow_accumulation_factor: crate::constants::FIXED_POINT_SCALE,
            last_update_slot: now()?,
            slots_per_year,
            liquidity_decimals: decimals,
            loan_to_value_bps,
            liquidation_threshold_bps,
            liquidation_bonus_bps,
            close_factor_bps,
            reserve_factor_bps,
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
// update_slots_per_year
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct UpdateSlotsPerYear {
    pub owner: Signer,
    #[account(has_one(owner))]
    pub lending_market: Account<LendingMarket>,
    #[account(mut, has_one(lending_market))]
    pub reserve: Account<Reserve>,
}

impl UpdateSlotsPerYear {
    /// Retune the reserve to the cluster's current slot time. Every other config
    /// value is a policy choice the owner makes; this one tracks a protocol
    /// parameter that changes without asking, so it gets its own handler.
    ///
    /// Interest is accrued at the old rate first, so the slots already elapsed
    /// are charged at the figure that was in force for them rather than being
    /// silently repriced by the new one.
    #[inline(always)]
    pub fn run(&mut self, slots_per_year: u64) -> Result<(), ProgramError> {
        require!(slots_per_year > 0, LendingError::InvalidConfig);

        let mut reserve = snapshot_reserve(&self.reserve);
        accrue(&mut reserve, now()?)?;
        reserve.slots_per_year = slots_per_year;
        self.reserve.set_inner(reserve);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// set_price (Switchboard stand-in for tests)
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct SetPrice {
    #[account(mut)]
    pub owner: Signer,
    // Only the market's owner may publish its prices.
    #[account(has_one(owner))]
    pub lending_market: Account<LendingMarket>,
    // Seeded by (market, mint) — scoped to the market, not to any individual.
    #[account(init(idempotent), payer = owner, address = PriceFeed::seeds(lending_market.address(), mint.address()))]
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
        self.price_feed.set_inner(PriceFeedInner {
            market: *self.lending_market.address(),
            mint: *self.mint.address(),
            price_mantissa,
            exponent,
            last_updated_slot: now()?,
            bump: bumps.price_feed,
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// collect_protocol_fees
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct CollectProtocolFees {
    #[account(mut)]
    pub owner: Signer,
    #[account(has_one(owner))]
    pub lending_market: Account<LendingMarket>,
    #[account(mut, has_one(lending_market), has_one(liquidity_mint), has_one(liquidity_vault))]
    pub reserve: Account<Reserve>,
    pub liquidity_mint: Account<Mint>,
    #[account(mut)]
    pub liquidity_vault: Account<Token>,
    #[account(mut)]
    pub owner_liquidity: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

impl CollectProtocolFees {
    /// Pay the reserve's accrued protocol fees to the market owner. This is how
    /// the owner earns: `reserve_factor_bps` of every interest accrual is set
    /// aside in `accumulated_protocol_fees`, and this withdraws it — capped by
    /// the liquidity currently sitting in the vault.
    #[inline(always)]
    pub fn run(&mut self) -> Result<(), ProgramError> {
        let slot = now()?;
        let mut reserve = snapshot_reserve(&self.reserve);
        accrue(&mut reserve, slot)?;

        let amount = reserve
            .accumulated_protocol_fees
            .min(reserve.available_liquidity);
        require!(amount > 0, LendingError::NothingToCollect);
        reserve.accumulated_protocol_fees = reserve
            .accumulated_protocol_fees
            .checked_sub(amount)
            .ok_or(LendingError::MathOverflow)?;
        reserve.available_liquidity = reserve
            .available_liquidity
            .checked_sub(amount)
            .ok_or(LendingError::MathOverflow)?;

        let decimals = reserve.liquidity_decimals;
        let bump = [reserve.bump];
        let lending_market = reserve.lending_market;
        let liquidity_mint = reserve.liquidity_mint;
        self.reserve.set_inner(reserve);

        let seeds = reserve_seeds!(lending_market, liquidity_mint, bump);
        self.token_program
            .transfer_checked(
                &self.liquidity_vault,
                &self.liquidity_mint,
                &self.owner_liquidity,
                &self.reserve,
                amount,
                decimals,
            )
            .invoke_signed(&seeds)
    }
}
