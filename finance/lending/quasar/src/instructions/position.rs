use {
    crate::{
        constants::BPS_DENOMINATOR,
        error::LendingError,
        instructions::supply::reserve_seeds,
        logic::{accrue, now, price_scaled, snapshot_obligation, snapshot_reserve, SCALE},
        math::{current_debt, market_value, mul_div_ceil, mul_div_floor, net_total_liquidity, value_to_amount, Rounding},
        state::{
            LendingMarket, Obligation, ObligationInner, ObligationVaultPda, PriceFeed, Reserve,
        },
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Obligation PDA signer seeds, used to authorize transfers out of the
/// obligation's collateral vault.
macro_rules! obligation_seeds {
    ($lending_market:expr, $owner:expr, $bump:expr) => {
        [
            Seed::from(crate::constants::OBLIGATION_SEED),
            Seed::from($lending_market.as_ref()),
            Seed::from($owner.as_ref()),
            Seed::from($bump.as_ref()),
        ]
    };
}

// ---------------------------------------------------------------------------
// init_obligation
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitObligation {
    #[account(mut)]
    pub owner: Signer,
    pub lending_market: Account<LendingMarket>,
    #[account(init, payer = owner, address = Obligation::seeds(lending_market.address(), owner.address()))]
    pub obligation: Account<Obligation>,
    pub system_program: Program<SystemProgram>,
}

impl InitObligation {
    #[inline(always)]
    pub fn run(&mut self, bumps: &InitObligationBumps) -> Result<(), ProgramError> {
        self.obligation.set_inner(ObligationInner {
            lending_market: *self.lending_market.address(),
            owner: *self.owner.address(),
            collateral_reserve: Address::default(),
            deposited_shares: 0,
            borrow_reserve: Address::default(),
            borrowed_scaled: 0,
            bump: bumps.obligation,
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// deposit_obligation_collateral
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct DepositObligationCollateral {
    #[account(mut)]
    pub owner: Signer,
    pub lending_market: Account<LendingMarket>,
    #[account(mut, has_one(owner), has_one(lending_market), address = Obligation::seeds(lending_market.address(), owner.address()))]
    pub obligation: Account<Obligation>,
    #[account(has_one(share_mint), has_one(lending_market))]
    pub reserve: Account<Reserve>,
    pub share_mint: Account<Mint>,
    #[account(
        init(idempotent),
        payer = owner,
        token(mint = share_mint, authority = obligation, token_program = token_program),
        address = ObligationVaultPda::seeds(reserve.address(), obligation.address())
    )]
    pub obligation_vault: InterfaceAccount<Token>,
    #[account(mut)]
    pub owner_share: Account<Token>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

impl DepositObligationCollateral {
    #[inline(always)]
    pub fn run(&mut self, shares: u64) -> Result<(), ProgramError> {
        require!(shares > 0, LendingError::ZeroAmount);
        let reserve_address = *self.reserve.address();

        let mut obligation = snapshot_obligation(&self.obligation);
        if obligation.collateral_reserve == Address::default() {
            obligation.collateral_reserve = reserve_address;
        } else {
            require_keys_eq!(obligation.collateral_reserve, reserve_address, LendingError::WrongReserve);
        }
        obligation.deposited_shares = obligation
            .deposited_shares
            .checked_add(shares)
            .ok_or(LendingError::MathOverflow)?;
        let decimals = self.share_mint.decimals;
        self.obligation.set_inner(obligation);

        self.token_program
            .transfer_checked(
                &self.owner_share,
                &self.share_mint,
                &self.obligation_vault,
                &self.owner,
                shares,
                decimals,
            )
            .invoke()
    }
}

// ---------------------------------------------------------------------------
// borrow_obligation_liquidity
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct BorrowObligationLiquidity {
    #[account(mut)]
    pub owner: Signer,
    pub lending_market: Account<LendingMarket>,
    #[account(mut, has_one(owner), has_one(lending_market), address = Obligation::seeds(lending_market.address(), owner.address()))]
    pub obligation: Account<Obligation>,
    #[account(mut, has_one(lending_market))]
    pub collateral_reserve: Account<Reserve>,
    pub collateral_price: Account<PriceFeed>,
    #[account(mut, has_one(lending_market), has_one(liquidity_mint), has_one(liquidity_vault))]
    pub borrow_reserve: Account<Reserve>,
    pub borrow_price: Account<PriceFeed>,
    pub liquidity_mint: Account<Mint>,
    #[account(mut)]
    pub liquidity_vault: Account<Token>,
    #[account(mut)]
    pub owner_liquidity: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

impl BorrowObligationLiquidity {
    #[inline(always)]
    pub fn run(&mut self, amount: u64) -> Result<(), ProgramError> {
        require!(amount > 0, LendingError::ZeroAmount);
        let slot = now()?;

        require_keys_eq!(
            self.obligation.collateral_reserve,
            *self.collateral_reserve.address(),
            LendingError::WrongReserve
        );
        require_keys_eq!(
            self.collateral_reserve.price_feed,
            *self.collateral_price.address(),
            LendingError::WrongReserve
        );
        require_keys_eq!(
            self.borrow_reserve.price_feed,
            *self.borrow_price.address(),
            LendingError::WrongReserve
        );

        let mut collateral = snapshot_reserve(&self.collateral_reserve);
        accrue(&mut collateral, slot)?;
        let mut borrow = snapshot_reserve(&self.borrow_reserve);
        accrue(&mut borrow, slot)?;
        let mut obligation = snapshot_obligation(&self.obligation);
        if obligation.borrow_reserve != Address::default() {
            require_keys_eq!(
                obligation.borrow_reserve,
                *self.borrow_reserve.address(),
                LendingError::WrongReserve
            );
        }

        // Borrow power from collateral value.
        let collateral_total = net_total_liquidity(
            collateral.available_liquidity,
            collateral.borrowed_amount_scaled,
            collateral.cumulative_borrow_rate_index,
            collateral.accumulated_protocol_fees,
        )?;
        let collateral_liquidity = mul_div_floor(
            obligation.deposited_shares as u128,
            collateral_total,
            (collateral.share_mint_supply as u128).max(1),
        )?;
        let collateral_value = market_value(
            u64::try_from(collateral_liquidity).map_err(|_| LendingError::MathOverflow)?,
            collateral.liquidity_decimals,
            price_scaled(&self.collateral_price, slot)?,
            Rounding::Down,
        )?;
        let allowed = mul_div_floor(collateral_value, collateral.loan_to_value_bps as u128, BPS_DENOMINATOR)?;

        // Existing debt value + the new borrow, both rounded up.
        let borrow_price = price_scaled(&self.borrow_price, slot)?;
        let existing_debt = current_debt(obligation.borrowed_scaled, borrow.cumulative_borrow_rate_index)?;
        let existing_value = market_value(existing_debt, borrow.liquidity_decimals, borrow_price, Rounding::Up)?;
        let new_value = market_value(amount, borrow.liquidity_decimals, borrow_price, Rounding::Up)?;
        let projected = existing_value.checked_add(new_value).ok_or(LendingError::MathOverflow)?;
        require!(projected <= allowed, LendingError::BorrowTooLarge);
        require!(amount <= borrow.available_liquidity, LendingError::InsufficientLiquidity);

        let scaled_added = mul_div_ceil(amount as u128, SCALE, borrow.cumulative_borrow_rate_index)?;
        borrow.borrowed_amount_scaled = borrow
            .borrowed_amount_scaled
            .checked_add(scaled_added)
            .ok_or(LendingError::MathOverflow)?;
        borrow.available_liquidity = borrow
            .available_liquidity
            .checked_sub(amount)
            .ok_or(LendingError::MathOverflow)?;
        obligation.borrow_reserve = *self.borrow_reserve.address();
        obligation.borrowed_scaled = obligation
            .borrowed_scaled
            .checked_add(scaled_added)
            .ok_or(LendingError::MathOverflow)?;

        let bump = [borrow.bump];
        let lending_market = borrow.lending_market;
        let liquidity_mint = borrow.liquidity_mint;
        let decimals = borrow.liquidity_decimals;
        self.collateral_reserve.set_inner(collateral);
        self.borrow_reserve.set_inner(borrow);
        self.obligation.set_inner(obligation);

        let seeds = reserve_seeds!(lending_market, liquidity_mint, bump);
        self.token_program
            .transfer_checked(
                &self.liquidity_vault,
                &self.liquidity_mint,
                &self.owner_liquidity,
                &self.borrow_reserve,
                amount,
                decimals,
            )
            .invoke_signed(&seeds)
    }
}

// ---------------------------------------------------------------------------
// repay_obligation_liquidity
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct RepayObligationLiquidity {
    #[account(mut)]
    pub repayer: Signer,
    #[account(mut)]
    pub obligation: Account<Obligation>,
    #[account(mut, has_one(liquidity_mint), has_one(liquidity_vault))]
    pub borrow_reserve: Account<Reserve>,
    pub liquidity_mint: Account<Mint>,
    #[account(mut)]
    pub liquidity_vault: Account<Token>,
    #[account(mut)]
    pub repayer_liquidity: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

impl RepayObligationLiquidity {
    #[inline(always)]
    pub fn run(&mut self, amount: u64) -> Result<(), ProgramError> {
        require!(amount > 0, LendingError::ZeroAmount);
        let slot = now()?;

        require_keys_eq!(
            self.obligation.borrow_reserve,
            *self.borrow_reserve.address(),
            LendingError::WrongReserve
        );

        let mut borrow = snapshot_reserve(&self.borrow_reserve);
        accrue(&mut borrow, slot)?;
        let mut obligation = snapshot_obligation(&self.obligation);

        let debt = current_debt(obligation.borrowed_scaled, borrow.cumulative_borrow_rate_index)?;
        let repay = amount.min(debt);
        require!(repay > 0, LendingError::ZeroAmount);
        let scaled_removed = mul_div_floor(repay as u128, SCALE, borrow.cumulative_borrow_rate_index)?
            .min(obligation.borrowed_scaled);

        borrow.borrowed_amount_scaled = borrow
            .borrowed_amount_scaled
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        borrow.available_liquidity = borrow
            .available_liquidity
            .checked_add(repay)
            .ok_or(LendingError::MathOverflow)?;
        obligation.borrowed_scaled = obligation
            .borrowed_scaled
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;

        let decimals = borrow.liquidity_decimals;
        self.borrow_reserve.set_inner(borrow);
        self.obligation.set_inner(obligation);

        self.token_program
            .transfer_checked(
                &self.repayer_liquidity,
                &self.liquidity_mint,
                &self.liquidity_vault,
                &self.repayer,
                repay,
                decimals,
            )
            .invoke()
    }
}

// ---------------------------------------------------------------------------
// withdraw_obligation_collateral
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct WithdrawObligationCollateral {
    #[account(mut)]
    pub owner: Signer,
    pub lending_market: Account<LendingMarket>,
    #[account(mut, has_one(owner), has_one(lending_market), address = Obligation::seeds(lending_market.address(), owner.address()))]
    pub obligation: Account<Obligation>,
    #[account(mut, has_one(lending_market), has_one(share_mint))]
    pub collateral_reserve: Account<Reserve>,
    pub collateral_price: Account<PriceFeed>,
    pub share_mint: Account<Mint>,
    /// Pass the borrow reserve + price when the obligation has debt; ignored when
    /// `borrowed_scaled == 0` (nothing to value).
    pub borrow_reserve: Account<Reserve>,
    pub borrow_price: Account<PriceFeed>,
    #[account(mut, address = ObligationVaultPda::seeds(collateral_reserve.address(), obligation.address()))]
    pub obligation_vault: InterfaceAccount<Token>,
    #[account(mut)]
    pub owner_share: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

impl WithdrawObligationCollateral {
    #[inline(always)]
    pub fn run(&mut self, shares: u64) -> Result<(), ProgramError> {
        require!(shares > 0, LendingError::ZeroAmount);
        let slot = now()?;

        require_keys_eq!(
            self.obligation.collateral_reserve,
            *self.collateral_reserve.address(),
            LendingError::WrongReserve
        );
        require_keys_eq!(
            self.collateral_reserve.price_feed,
            *self.collateral_price.address(),
            LendingError::WrongReserve
        );

        let mut collateral = snapshot_reserve(&self.collateral_reserve);
        accrue(&mut collateral, slot)?;
        let mut obligation = snapshot_obligation(&self.obligation);
        require!(obligation.deposited_shares >= shares, LendingError::WithdrawTooLarge);

        // Remaining collateral value after withdrawing `shares`.
        let remaining_shares = obligation.deposited_shares - shares;
        let collateral_total = net_total_liquidity(
            collateral.available_liquidity,
            collateral.borrowed_amount_scaled,
            collateral.cumulative_borrow_rate_index,
            collateral.accumulated_protocol_fees,
        )?;
        let remaining_liquidity = mul_div_floor(
            remaining_shares as u128,
            collateral_total,
            (collateral.share_mint_supply as u128).max(1),
        )?;
        let remaining_value = market_value(
            u64::try_from(remaining_liquidity).map_err(|_| LendingError::MathOverflow)?,
            collateral.liquidity_decimals,
            price_scaled(&self.collateral_price, slot)?,
            Rounding::Down,
        )?;
        let allowed = mul_div_floor(remaining_value, collateral.loan_to_value_bps as u128, BPS_DENOMINATOR)?;

        // Debt value (zero when the obligation has no borrow).
        let debt_value = if obligation.borrowed_scaled > 0 {
            require_keys_eq!(
                obligation.borrow_reserve,
                *self.borrow_reserve.address(),
                LendingError::WrongReserve
            );
            require_keys_eq!(
                self.borrow_reserve.price_feed,
                *self.borrow_price.address(),
                LendingError::WrongReserve
            );
            let mut borrow = snapshot_reserve(&self.borrow_reserve);
            accrue(&mut borrow, slot)?;
            let debt = current_debt(obligation.borrowed_scaled, borrow.cumulative_borrow_rate_index)?;
            market_value(debt, borrow.liquidity_decimals, price_scaled(&self.borrow_price, slot)?, Rounding::Up)?
        } else {
            0
        };
        require!(debt_value <= allowed, LendingError::WithdrawTooLarge);

        obligation.deposited_shares = remaining_shares;

        let decimals = self.share_mint.decimals;
        let lending_market = obligation.lending_market;
        let owner = obligation.owner;
        let bump = [obligation.bump];
        self.collateral_reserve.set_inner(collateral);
        self.obligation.set_inner(obligation);

        let seeds = obligation_seeds!(lending_market, owner, bump);
        self.token_program
            .transfer_checked(
                &self.obligation_vault,
                &self.share_mint,
                &self.owner_share,
                &self.obligation,
                shares,
                decimals,
            )
            .invoke_signed(&seeds)
    }
}

// ---------------------------------------------------------------------------
// liquidate_obligation
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct LiquidateObligation {
    #[account(mut)]
    pub liquidator: Signer,
    #[account(mut, has_one(lending_market))]
    pub obligation: Account<Obligation>,
    pub lending_market: Account<LendingMarket>,
    #[account(mut, has_one(lending_market), has_one(share_mint))]
    pub collateral_reserve: Account<Reserve>,
    pub collateral_price: Account<PriceFeed>,
    pub share_mint: Account<Mint>,
    #[account(mut, address = ObligationVaultPda::seeds(collateral_reserve.address(), obligation.address()))]
    pub obligation_vault: InterfaceAccount<Token>,
    #[account(mut)]
    pub liquidator_collateral: Account<Token>,
    #[account(mut, has_one(lending_market), has_one(liquidity_mint), has_one(liquidity_vault))]
    pub borrow_reserve: Account<Reserve>,
    pub borrow_price: Account<PriceFeed>,
    pub liquidity_mint: Account<Mint>,
    #[account(mut)]
    pub liquidity_vault: Account<Token>,
    #[account(mut)]
    pub liquidator_liquidity: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

impl LiquidateObligation {
    #[inline(always)]
    pub fn run(&mut self, amount: u64) -> Result<(), ProgramError> {
        require!(amount > 0, LendingError::ZeroAmount);
        let slot = now()?;

        require_keys_eq!(self.obligation.collateral_reserve, *self.collateral_reserve.address(), LendingError::WrongReserve);
        require_keys_eq!(self.obligation.borrow_reserve, *self.borrow_reserve.address(), LendingError::WrongReserve);
        require_keys_eq!(self.collateral_reserve.price_feed, *self.collateral_price.address(), LendingError::WrongReserve);
        require_keys_eq!(self.borrow_reserve.price_feed, *self.borrow_price.address(), LendingError::WrongReserve);

        let mut collateral = snapshot_reserve(&self.collateral_reserve);
        accrue(&mut collateral, slot)?;
        let mut borrow = snapshot_reserve(&self.borrow_reserve);
        accrue(&mut borrow, slot)?;
        let mut obligation = snapshot_obligation(&self.obligation);

        let collateral_price = price_scaled(&self.collateral_price, slot)?;
        let borrow_price = price_scaled(&self.borrow_price, slot)?;

        // Health: unhealthy when debt value exceeds collateral value * liq threshold.
        let collateral_total = net_total_liquidity(
            collateral.available_liquidity,
            collateral.borrowed_amount_scaled,
            collateral.cumulative_borrow_rate_index,
            collateral.accumulated_protocol_fees,
        )?;
        let collateral_liquidity = mul_div_floor(
            obligation.deposited_shares as u128,
            collateral_total,
            (collateral.share_mint_supply as u128).max(1),
        )?;
        let collateral_value = market_value(
            u64::try_from(collateral_liquidity).map_err(|_| LendingError::MathOverflow)?,
            collateral.liquidity_decimals,
            collateral_price,
            Rounding::Down,
        )?;
        let unhealthy_threshold = mul_div_floor(collateral_value, collateral.liquidation_threshold_bps as u128, BPS_DENOMINATOR)?;
        let debt = current_debt(obligation.borrowed_scaled, borrow.cumulative_borrow_rate_index)?;
        let debt_value = market_value(debt, borrow.liquidity_decimals, borrow_price, Rounding::Up)?;
        require!(debt_value > unhealthy_threshold, LendingError::ObligationHealthy);

        // Repay capped by the close factor — taken from the borrow reserve
        // because it is a property of the debt being closed.
        let max_repay = mul_div_floor(debt as u128, borrow.close_factor_bps as u128, BPS_DENOMINATOR)?;
        let repay = amount.min(u64::try_from(max_repay).map_err(|_| LendingError::MathOverflow)?);
        require!(repay > 0, LendingError::ZeroAmount);

        // Seize collateral worth repay value + bonus, converted to share tokens.
        let repay_value = market_value(repay, borrow.liquidity_decimals, borrow_price, Rounding::Down)?;
        let bonus = mul_div_floor(repay_value, collateral.liquidation_bonus_bps as u128, BPS_DENOMINATOR)?;
        let seize_value = repay_value.checked_add(bonus).ok_or(LendingError::MathOverflow)?;
        let seize_liquidity = value_to_amount(seize_value, collateral.liquidity_decimals, collateral_price, Rounding::Down)?;
        let seize_shares = mul_div_floor(
            seize_liquidity as u128,
            collateral.share_mint_supply as u128,
            collateral_total.max(1),
        )?;
        let seize_shares = u64::try_from(seize_shares).map_err(|_| LendingError::MathOverflow)?;
        require!(seize_shares > 0, LendingError::ZeroAmount);
        // Reject rather than silently seize less: a capped seizure would make
        // the liquidator pay full price for less collateral.
        require!(
            seize_shares <= obligation.deposited_shares,
            LendingError::LiquidationTooLarge
        );

        let scaled_removed = mul_div_floor(repay as u128, SCALE, borrow.cumulative_borrow_rate_index)?
            .min(obligation.borrowed_scaled);

        borrow.borrowed_amount_scaled = borrow.borrowed_amount_scaled.checked_sub(scaled_removed).ok_or(LendingError::MathOverflow)?;
        borrow.available_liquidity = borrow.available_liquidity.checked_add(repay).ok_or(LendingError::MathOverflow)?;
        obligation.borrowed_scaled = obligation.borrowed_scaled.checked_sub(scaled_removed).ok_or(LendingError::MathOverflow)?;
        obligation.deposited_shares = obligation.deposited_shares.checked_sub(seize_shares).ok_or(LendingError::MathOverflow)?;

        let share_decimals = self.share_mint.decimals;
        let borrow_decimals = borrow.liquidity_decimals;
        let lending_market = obligation.lending_market;
        let owner = obligation.owner;
        let bump = [obligation.bump];
        self.collateral_reserve.set_inner(collateral);
        self.borrow_reserve.set_inner(borrow);
        self.obligation.set_inner(obligation);

        // Liquidator repays the debt token...
        self.token_program
            .transfer_checked(
                &self.liquidator_liquidity,
                &self.liquidity_mint,
                &self.liquidity_vault,
                &self.liquidator,
                repay,
                borrow_decimals,
            )
            .invoke()?;

        // ...and receives the seized collateral share tokens (obligation PDA signs).
        let seeds = obligation_seeds!(lending_market, owner, bump);
        self.token_program
            .transfer_checked(
                &self.obligation_vault,
                &self.share_mint,
                &self.liquidator_collateral,
                &self.obligation,
                seize_shares,
                share_decimals,
            )
            .invoke_signed(&seeds)
    }
}
