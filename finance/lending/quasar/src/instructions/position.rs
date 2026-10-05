use {
    crate::{
        constants::BPS_DENOMINATOR,
        error::LendingError,
        instructions::supply::reserve_seeds,
        logic::{accrue, now, price_scaled, snapshot_obligation, snapshot_reserve, SCALE},
        math::{
            current_debt, market_value, mul_div_ceil, mul_div_floor, net_total_liquidity,
            total_shares, value_to_amount, Rounding,
        },
        state::{
            LendingMarket, Obligation, ObligationInner, ObligationVaultPda, PriceFeed, Reserve,
            ReserveInner,
        },
    },
    quasar_lang::{cpi::Seed, prelude::*},
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
// initialize_obligation
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeObligation {
    #[account(mut)]
    pub owner: Signer,
    pub lending_market: Account<LendingMarket>,
    #[account(init, payer = owner, address = Obligation::seeds(lending_market.address(), owner.address()))]
    pub obligation: Account<Obligation>,
    pub system_program: Program<SystemProgram>,
}

impl InitializeObligation {
    #[inline(always)]
    pub fn run(&mut self, bumps: &InitializeObligationBumps) -> Result<(), ProgramError> {
        self.obligation.set_inner(ObligationInner {
            lending_market: *self.lending_market.address(),
            owner: *self.owner.address(),
            collateral_reserve: Address::default(),
            deposited_shares: 0,
            borrow_reserve: Address::default(),
            borrowed_principal: 0,
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
            require_keys_eq!(
                obligation.collateral_reserve,
                reserve_address,
                LendingError::WrongReserve
            );
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
    #[account(
        mut,
        has_one(lending_market),
        has_one(liquidity_mint),
        has_one(liquidity_vault)
    )]
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
        let (slot, timestamp) = now()?;

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
        accrue(&mut collateral, slot, timestamp)?;
        let mut borrow = snapshot_reserve(&self.borrow_reserve);
        accrue(&mut borrow, slot, timestamp)?;
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
            collateral.borrowed_principal,
            collateral.borrow_accumulation_factor,
            collateral.accumulated_program_fees,
        )?;
        let collateral_liquidity = mul_div_floor(
            obligation.deposited_shares as u128,
            collateral_total,
            total_shares(collateral.share_mint_supply)?,
        )?;
        let collateral_value = market_value(
            u64::try_from(collateral_liquidity).map_err(|_| LendingError::MathOverflow)?,
            collateral.liquidity_decimals,
            price_scaled(&self.collateral_price, slot, collateral.max_confidence_bps)?,
            Rounding::Down,
        )?;
        let allowed = mul_div_floor(
            collateral_value,
            collateral.loan_to_value_bps as u128,
            BPS_DENOMINATOR,
        )?;

        // Existing debt value + the new borrow, both rounded up.
        let borrow_price = price_scaled(&self.borrow_price, slot, borrow.max_confidence_bps)?;
        let existing_debt = current_debt(
            obligation.borrowed_principal,
            borrow.borrow_accumulation_factor,
        )?;
        let existing_value = market_value(
            existing_debt,
            borrow.liquidity_decimals,
            borrow_price,
            Rounding::Up,
        )?;
        let new_value = market_value(
            amount,
            borrow.liquidity_decimals,
            borrow_price,
            Rounding::Up,
        )?;
        let projected = existing_value
            .checked_add(new_value)
            .ok_or(LendingError::MathOverflow)?;
        require!(projected <= allowed, LendingError::BorrowTooLarge);
        require!(
            amount <= borrow.available_liquidity,
            LendingError::InsufficientLiquidity
        );

        let scaled_added = mul_div_ceil(amount as u128, SCALE, borrow.borrow_accumulation_factor)?;
        borrow.borrowed_principal = borrow
            .borrowed_principal
            .checked_add(scaled_added)
            .ok_or(LendingError::MathOverflow)?;
        borrow.available_liquidity = borrow
            .available_liquidity
            .checked_sub(amount)
            .ok_or(LendingError::MathOverflow)?;
        obligation.borrow_reserve = *self.borrow_reserve.address();
        obligation.borrowed_principal = obligation
            .borrowed_principal
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
        let (slot, timestamp) = now()?;

        require_keys_eq!(
            self.obligation.borrow_reserve,
            *self.borrow_reserve.address(),
            LendingError::WrongReserve
        );

        let mut borrow = snapshot_reserve(&self.borrow_reserve);
        accrue(&mut borrow, slot, timestamp)?;
        let mut obligation = snapshot_obligation(&self.obligation);

        let debt = current_debt(
            obligation.borrowed_principal,
            borrow.borrow_accumulation_factor,
        )?;
        let repay = amount.min(debt);
        require!(repay > 0, LendingError::ZeroAmount);
        let scaled_removed =
            mul_div_floor(repay as u128, SCALE, borrow.borrow_accumulation_factor)?
                .min(obligation.borrowed_principal);

        borrow.borrowed_principal = borrow
            .borrowed_principal
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        borrow.available_liquidity = borrow
            .available_liquidity
            .checked_add(repay)
            .ok_or(LendingError::MathOverflow)?;
        obligation.borrowed_principal = obligation
            .borrowed_principal
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

/// Withdraw posted share-token collateral.
///
/// With debt outstanding this is a health-dependent action: both price feeds
/// must be fresh and the debt must still fit under the borrow limit of the
/// collateral that remains. With no debt the collateral backs nothing, so no
/// price is read and no health check runs: the whole deposit can come out
/// whatever the feeds are doing, since a borrower who owes nothing must never
/// be locked in by a stale or silent oracle. The price accounts are still
/// passed, and checked to be the reserves' own, but their values are not
/// read.
///
/// A withdrawal that takes the last share closes the collateral vault and
/// returns its rent to the owner, who paid it when
/// `deposit_obligation_collateral` created the vault (`init(idempotent)`
/// creates it again on a later deposit). The whole vault balance goes to the
/// owner first, so share tokens someone sent straight to the vault cannot keep
/// it open or block the withdrawal.
#[derive(Accounts)]
pub struct WithdrawObligationCollateral {
    #[account(mut)]
    pub owner: Signer,
    pub lending_market: Account<LendingMarket>,
    #[account(mut, has_one(owner), has_one(lending_market), address = Obligation::seeds(lending_market.address(), owner.address()))]
    pub obligation: Account<Obligation>,
    #[account(mut, has_one(lending_market), has_one(share_mint))]
    pub collateral_reserve: Account<Reserve>,
    /// Read only when the obligation has debt.
    pub collateral_price: Account<PriceFeed>,
    pub share_mint: Account<Mint>,
    /// Pass the borrow reserve + price when the obligation has debt; ignored when
    /// `borrowed_principal == 0` (nothing to value).
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
        let (slot, timestamp) = now()?;

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
        accrue(&mut collateral, slot, timestamp)?;
        let mut obligation = snapshot_obligation(&self.obligation);
        require!(
            obligation.deposited_shares >= shares,
            LendingError::WithdrawTooLarge
        );
        let remaining_shares = obligation.deposited_shares - shares;

        // With debt, the collateral that remains must still cover it at fresh
        // prices. Without debt there is nothing to cover, and no price is read.
        if obligation.borrowed_principal > 0 {
            self.require_debt_covered_by(&collateral, remaining_shares, slot, timestamp)?;
        }

        obligation.deposited_shares = remaining_shares;
        let empties_vault = remaining_shares == 0;
        // Emptying the position sweeps the vault, donations included, so it
        // can close.
        let transfer_amount = if empties_vault {
            self.obligation_vault.amount()
        } else {
            shares
        };

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
                transfer_amount,
                decimals,
            )
            .invoke_signed(&seeds)?;
        if empties_vault {
            self.token_program
                .close_account(&self.obligation_vault, &self.owner, &self.obligation)
                .invoke_signed(&seeds)?;
        }
        Ok(())
    }

    /// The health check for a withdrawal from an obligation with debt: value
    /// the `remaining_shares` of collateral at a fresh price, floored, and the
    /// debt at a fresh price, ceiled, and refuse unless the debt fits under
    /// the remaining collateral's loan-to-value.
    #[inline(always)]
    fn require_debt_covered_by(
        &self,
        collateral: &ReserveInner,
        remaining_shares: u64,
        slot: u64,
        timestamp: i64,
    ) -> Result<(), ProgramError> {
        let obligation = snapshot_obligation(&self.obligation);
        let collateral_total = net_total_liquidity(
            collateral.available_liquidity,
            collateral.borrowed_principal,
            collateral.borrow_accumulation_factor,
            collateral.accumulated_program_fees,
        )?;
        let remaining_liquidity = mul_div_floor(
            remaining_shares as u128,
            collateral_total,
            total_shares(collateral.share_mint_supply)?,
        )?;
        let remaining_value = market_value(
            u64::try_from(remaining_liquidity).map_err(|_| LendingError::MathOverflow)?,
            collateral.liquidity_decimals,
            price_scaled(&self.collateral_price, slot, collateral.max_confidence_bps)?,
            Rounding::Down,
        )?;
        let allowed = mul_div_floor(
            remaining_value,
            collateral.loan_to_value_bps as u128,
            BPS_DENOMINATOR,
        )?;

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
        accrue(&mut borrow, slot, timestamp)?;
        let debt = current_debt(
            obligation.borrowed_principal,
            borrow.borrow_accumulation_factor,
        )?;
        let debt_value = market_value(
            debt,
            borrow.liquidity_decimals,
            price_scaled(&self.borrow_price, slot, borrow.max_confidence_bps)?,
            Rounding::Up,
        )?;
        require!(debt_value <= allowed, LendingError::WithdrawTooLarge);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// close_obligation
// ---------------------------------------------------------------------------

/// Close an obligation that holds nothing, returning its rent to the owner.
///
/// The obligation must have no deposited shares and no borrowed principal:
/// repaying the last unit zeroes the principal, and withdrawing the last share
/// zeroes the deposit, so both being zero means the position is fully
/// unwound. Closing one that still holds either would strand the collateral
/// in its vault, or forgive the debt, so the handler refuses with
/// `ObligationNotEmpty`. Only the owner may close it (`has_one(owner)`), since
/// the rent is theirs and a stranger could otherwise close a position its
/// owner means to use again. The account itself closes through the
/// `close(dest = owner)` constraint once the handler returns.
#[derive(Accounts)]
pub struct CloseObligation {
    #[account(mut)]
    pub owner: Signer,
    pub lending_market: Account<LendingMarket>,
    #[account(
        mut,
        close(dest = owner),
        has_one(owner),
        has_one(lending_market),
        address = Obligation::seeds(lending_market.address(), owner.address())
    )]
    pub obligation: Account<Obligation>,
}

impl CloseObligation {
    #[inline(always)]
    pub fn run(&mut self) -> Result<(), ProgramError> {
        let obligation = snapshot_obligation(&self.obligation);
        require!(
            obligation.deposited_shares == 0 && obligation.borrowed_principal == 0,
            LendingError::ObligationNotEmpty
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// liquidate_obligation
// ---------------------------------------------------------------------------

/// A seizure that takes the last share closes the collateral vault, rent to
/// the obligation's owner (`obligation_owner`), who paid it. The whole vault
/// balance goes to the liquidator first, so share tokens someone sent straight
/// to the vault cannot keep it open.
#[derive(Accounts)]
pub struct LiquidateObligation {
    #[account(mut)]
    pub liquidator: Signer,
    #[account(mut, has_one(lending_market))]
    pub obligation: Account<Obligation>,
    /// The obligation's owner, who paid the collateral vault's rent; receives
    /// it back if this seizure empties the vault.
    #[account(mut, address = obligation.owner)]
    pub obligation_owner: UncheckedAccount,
    pub lending_market: Account<LendingMarket>,
    #[account(mut, has_one(lending_market), has_one(share_mint))]
    pub collateral_reserve: Account<Reserve>,
    pub collateral_price: Account<PriceFeed>,
    pub share_mint: Account<Mint>,
    #[account(mut, address = ObligationVaultPda::seeds(collateral_reserve.address(), obligation.address()))]
    pub obligation_vault: InterfaceAccount<Token>,
    #[account(mut)]
    pub liquidator_collateral: Account<Token>,
    #[account(
        mut,
        has_one(lending_market),
        has_one(liquidity_mint),
        has_one(liquidity_vault)
    )]
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
        let (slot, timestamp) = now()?;

        require_keys_eq!(
            self.obligation.collateral_reserve,
            *self.collateral_reserve.address(),
            LendingError::WrongReserve
        );
        require_keys_eq!(
            self.obligation.borrow_reserve,
            *self.borrow_reserve.address(),
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
        accrue(&mut collateral, slot, timestamp)?;
        let mut borrow = snapshot_reserve(&self.borrow_reserve);
        accrue(&mut borrow, slot, timestamp)?;
        let mut obligation = snapshot_obligation(&self.obligation);

        let collateral_price =
            price_scaled(&self.collateral_price, slot, collateral.max_confidence_bps)?;
        let borrow_price = price_scaled(&self.borrow_price, slot, borrow.max_confidence_bps)?;

        // Health: unhealthy when debt value exceeds collateral value * liquidation threshold.
        let collateral_total = net_total_liquidity(
            collateral.available_liquidity,
            collateral.borrowed_principal,
            collateral.borrow_accumulation_factor,
            collateral.accumulated_program_fees,
        )?;
        let collateral_liquidity = mul_div_floor(
            obligation.deposited_shares as u128,
            collateral_total,
            total_shares(collateral.share_mint_supply)?,
        )?;
        let collateral_value = market_value(
            u64::try_from(collateral_liquidity).map_err(|_| LendingError::MathOverflow)?,
            collateral.liquidity_decimals,
            collateral_price,
            Rounding::Down,
        )?;
        let unhealthy_threshold = mul_div_floor(
            collateral_value,
            collateral.liquidation_threshold_bps as u128,
            BPS_DENOMINATOR,
        )?;
        let debt = current_debt(
            obligation.borrowed_principal,
            borrow.borrow_accumulation_factor,
        )?;
        let debt_value = market_value(debt, borrow.liquidity_decimals, borrow_price, Rounding::Up)?;
        require!(
            debt_value > unhealthy_threshold,
            LendingError::ObligationHealthy
        );

        // Repay capped by the close factor — taken from the borrow reserve
        // because it is a property of the debt being closed.
        let max_repay = mul_div_floor(
            debt as u128,
            borrow.close_factor_bps as u128,
            BPS_DENOMINATOR,
        )?;
        let repay = amount.min(u64::try_from(max_repay).map_err(|_| LendingError::MathOverflow)?);
        require!(repay > 0, LendingError::ZeroAmount);

        // Seize collateral worth repay value + bonus, converted to share tokens.
        let repay_value = market_value(
            repay,
            borrow.liquidity_decimals,
            borrow_price,
            Rounding::Down,
        )?;
        let bonus = mul_div_floor(
            repay_value,
            collateral.liquidation_bonus_bps as u128,
            BPS_DENOMINATOR,
        )?;
        let seize_value = repay_value
            .checked_add(bonus)
            .ok_or(LendingError::MathOverflow)?;
        let seize_liquidity = value_to_amount(
            seize_value,
            collateral.liquidity_decimals,
            collateral_price,
            Rounding::Down,
        )?;
        let seize_shares = mul_div_floor(
            seize_liquidity as u128,
            total_shares(collateral.share_mint_supply)?,
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

        let scaled_removed =
            mul_div_floor(repay as u128, SCALE, borrow.borrow_accumulation_factor)?
                .min(obligation.borrowed_principal);

        borrow.borrowed_principal = borrow
            .borrowed_principal
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        borrow.available_liquidity = borrow
            .available_liquidity
            .checked_add(repay)
            .ok_or(LendingError::MathOverflow)?;
        obligation.borrowed_principal = obligation
            .borrowed_principal
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        obligation.deposited_shares = obligation
            .deposited_shares
            .checked_sub(seize_shares)
            .ok_or(LendingError::MathOverflow)?;
        let empties_vault = obligation.deposited_shares == 0;
        // Emptying the position sweeps the vault, donations included, so it
        // can close.
        let seize_transfer = if empties_vault {
            self.obligation_vault.amount()
        } else {
            seize_shares
        };

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
                seize_transfer,
                share_decimals,
            )
            .invoke_signed(&seeds)?;
        if empties_vault {
            self.token_program
                .close_account(
                    &self.obligation_vault,
                    &self.obligation_owner,
                    &self.obligation,
                )
                .invoke_signed(&seeds)?;
        }
        Ok(())
    }
}
