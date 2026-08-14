use anchor_lang::prelude::*;

use crate::constants::{BPS_DENOMINATOR, FIXED_POINT_SCALE, RESERVE_SEED, SLOTS_PER_YEAR};
use crate::errors::LendingError;
use crate::math::{mul_div_ceil, mul_div_floor};

/// Signer seeds for a reserve PDA, which is the authority over its liquidity
/// vault and the mint authority of its share token.
pub fn reserve_signer_seeds<'a>(
    lending_market: &'a Address,
    liquidity_mint: &'a Address,
    bump: &'a [u8; 1],
) -> [&'a [u8]; 4] {
    [
        RESERVE_SEED,
        lending_market.as_ref(),
        liquidity_mint.as_ref(),
        bump,
    ]
}

/// One asset's lending pool. Suppliers deposit `liquidity_mint` tokens into
/// `liquidity_vault` and receive share tokens (`share_mint`); the share-to-
/// liquidity exchange rate rises as borrowers pay interest. Borrowers draw
/// `liquidity_mint` out against collateral held in their obligation.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Reserve {
    pub lending_market: Address,

    pub liquidity_mint: Address,

    /// Program-owned token account holding the un-borrowed liquidity. Its
    /// authority is this reserve PDA.
    pub liquidity_vault: Address,

    /// Share-token mint. Supply equals `share_mint_supply`. Mint authority is
    /// this reserve PDA.
    pub share_mint: Address,

    pub price_feed: Address,

    pub liquidity_decimals: u8,

    /// Base units sitting in `liquidity_vault`, available to borrow or redeem.
    /// This is the source of truth for the pool size, not the vault's token
    /// balance, so a raw token donation cannot move the exchange rate.
    pub available_liquidity: u64,

    /// Outstanding share-token supply, tracked here so valuations need only the
    /// reserve account (not the mint) to convert shares to liquidity. A holder
    /// burning share tokens directly via the token program (outside this
    /// program) makes the real mint supply drift below this mirror; that drift
    /// only lowers what the burner could have redeemed, so the pool never pays
    /// out more than it holds.
    pub share_mint_supply: u64,

    /// Total borrowed principal. The live debt is
    /// `borrowed_principal * borrow_accumulation_factor / FIXED_POINT_SCALE`.
    pub borrowed_principal: u128,

    /// Monotonically increasing accumulation factor, FIXED_POINT_SCALE-scaled.
    /// Starts at FIXED_POINT_SCALE (1.0) and only ever multiplies by factors >= 1.
    pub borrow_accumulation_factor: u128,

    pub last_update_slot: u64,

    /// Liquidity owed to the market owner: the protocol's cut of accrued
    /// interest (`config.reserve_factor_bps`). It is carved out of
    /// `total_liquidity` so it never inflates the share exchange rate, and the
    /// owner withdraws it with `collect_protocol_fees`.
    pub accumulated_protocol_fees: u64,

    pub config: ReserveConfig,

    pub bump: u8,
}

/// Risk and interest-rate parameters. All ratios are basis points (10_000 = 100%).
#[derive(
    InitSpace, Clone, Copy, Debug, Default, IdlType, wincode::SchemaRead, wincode::SchemaWrite,
)]
pub struct ReserveConfig {
    /// Fraction of deposited collateral value a borrower may borrow against.
    pub loan_to_value_bps: u16,
    /// Above this fraction the obligation may be liquidated.
    pub liquidation_threshold_bps: u16,
    /// Extra collateral a liquidator receives, as a fraction of the repaid value.
    pub liquidation_bonus_bps: u16,
    /// Maximum fraction of a borrow that one liquidation may repay.
    pub close_factor_bps: u16,
    /// Share of accrued borrow interest kept by the protocol (the rest lifts the
    /// supplier exchange rate). This is how the market owner earns.
    pub reserve_factor_bps: u16,
    /// Utilization at which the borrow rate reaches `optimal_borrow_rate_bps`.
    pub optimal_utilization_bps: u16,
    /// Borrow APR at 0% utilization.
    pub min_borrow_rate_bps: u16,
    /// Borrow APR at `optimal_utilization_bps`.
    pub optimal_borrow_rate_bps: u16,
    /// Borrow APR at 100% utilization.
    pub max_borrow_rate_bps: u16,
}

impl ReserveConfig {
    pub fn validate(&self) -> Result<()> {
        let within_bps = |value: u16| (value as u128) <= BPS_DENOMINATOR;
        require!(
            within_bps(self.loan_to_value_bps)
                && within_bps(self.liquidation_threshold_bps)
                && within_bps(self.liquidation_bonus_bps)
                && within_bps(self.close_factor_bps)
                && within_bps(self.reserve_factor_bps)
                && within_bps(self.optimal_utilization_bps),
            LendingError::InvalidConfig
        );
        // A zero close factor would make every liquidation a no-op.
        require!(self.close_factor_bps > 0, LendingError::InvalidConfig);
        // The kink must be strictly inside (0, 100%) so neither rate slope divides by zero.
        require!(
            self.optimal_utilization_bps > 0
                && (self.optimal_utilization_bps as u128) < BPS_DENOMINATOR,
            LendingError::InvalidConfig
        );
        // You cannot be allowed to borrow past the point you'd be liquidated.
        require!(
            self.loan_to_value_bps <= self.liquidation_threshold_bps,
            LendingError::InvalidConfig
        );
        require!(
            self.min_borrow_rate_bps <= self.optimal_borrow_rate_bps
                && self.optimal_borrow_rate_bps <= self.max_borrow_rate_bps,
            LendingError::InvalidConfig
        );
        Ok(())
    }
}

impl Reserve {
    /// Live total debt owed to the pool, rounded up (protocol-favourable).
    pub fn current_borrowed_amount(&self) -> Result<u64> {
        let amount = mul_div_ceil(
            self.borrowed_principal,
            self.borrow_accumulation_factor,
            FIXED_POINT_SCALE,
        )?;
        u64::try_from(amount).map_err(|_| LendingError::MathOverflow.into())
    }

    /// Available liquidity plus live debt, before the protocol's fee is removed.
    /// Used for the utilization ratio, which is about how much of the pool is lent
    /// out, independent of who owns the interest.
    pub fn gross_liquidity(&self) -> Result<u128> {
        (self.available_liquidity as u128)
            .checked_add(self.current_borrowed_amount()? as u128)
            .ok_or(LendingError::MathOverflow.into())
    }

    /// The pool size the share token is a claim on: gross liquidity minus the
    /// protocol fees owed to the owner, which belong to no supplier.
    pub fn total_liquidity(&self) -> Result<u128> {
        self.gross_liquidity()?
            .checked_sub(self.accumulated_protocol_fees as u128)
            .ok_or(LendingError::MathOverflow.into())
    }

    /// Borrowed fraction of the pool, in basis points (0..=10_000).
    pub fn utilization_bps(&self) -> Result<u128> {
        let gross = self.gross_liquidity()?;
        if gross == 0 {
            return Ok(0);
        }
        mul_div_floor(
            self.current_borrowed_amount()? as u128,
            BPS_DENOMINATOR,
            gross,
        )
    }

    /// Per-slot borrow rate (FIXED_POINT_SCALE-scaled) from the kinked curve:
    /// linear from `min` to `optimal` up to the kink, then steeper from `optimal`
    /// to `max` between the kink and full utilization.
    pub fn current_borrow_rate_per_slot(&self) -> Result<u128> {
        let utilization = self.utilization_bps()?;
        let optimal_utilization = self.config.optimal_utilization_bps as u128;

        let apr_bps = if utilization <= optimal_utilization {
            let rate_range = (self.config.optimal_borrow_rate_bps as u128)
                .checked_sub(self.config.min_borrow_rate_bps as u128)
                .ok_or(LendingError::MathOverflow)?;
            let climbed = mul_div_floor(rate_range, utilization, optimal_utilization)?;
            (self.config.min_borrow_rate_bps as u128)
                .checked_add(climbed)
                .ok_or(LendingError::MathOverflow)?
        } else {
            let rate_range = (self.config.max_borrow_rate_bps as u128)
                .checked_sub(self.config.optimal_borrow_rate_bps as u128)
                .ok_or(LendingError::MathOverflow)?;
            let utilization_above = utilization
                .checked_sub(optimal_utilization)
                .ok_or(LendingError::MathOverflow)?;
            let utilization_range = BPS_DENOMINATOR
                .checked_sub(optimal_utilization)
                .ok_or(LendingError::MathOverflow)?;
            let climbed = mul_div_floor(rate_range, utilization_above, utilization_range)?;
            (self.config.optimal_borrow_rate_bps as u128)
                .checked_add(climbed)
                .ok_or(LendingError::MathOverflow)?
        };

        // apr_bps / (BPS_DENOMINATOR * SLOTS_PER_YEAR), carried at FIXED_POINT_SCALE.
        let per_year_denominator = BPS_DENOMINATOR
            .checked_mul(SLOTS_PER_YEAR)
            .ok_or(LendingError::MathOverflow)?;
        mul_div_floor(apr_bps, FIXED_POINT_SCALE, per_year_denominator)
    }

    /// Advance the accumulation factor for the slots elapsed since the last refresh.
    /// `new_factor = old_factor * (1 + rate_per_slot * elapsed_slots)`, a single
    /// multiply per refresh that compounds across refreshes (Solend's approach).
    pub fn accrue_interest(&mut self, current_slot: u64) -> Result<()> {
        let elapsed = current_slot
            .checked_sub(self.last_update_slot)
            .ok_or(LendingError::MathOverflow)?;

        if elapsed > 0 && self.borrowed_principal > 0 {
            let borrowed_before = self.current_borrowed_amount()?;
            let rate_per_slot = self.current_borrow_rate_per_slot()?;
            let accrued = rate_per_slot
                .checked_mul(elapsed as u128)
                .ok_or(LendingError::MathOverflow)?;
            let growth_factor = FIXED_POINT_SCALE
                .checked_add(accrued)
                .ok_or(LendingError::MathOverflow)?;
            self.borrow_accumulation_factor = mul_div_floor(
                self.borrow_accumulation_factor,
                growth_factor,
                FIXED_POINT_SCALE,
            )?;

            // Borrowers owe the full interest (the factor grew for all of it); the
            // protocol keeps `reserve_factor_bps` of the newly accrued interest,
            // and the remainder lifts the supplier exchange rate. Flooring the fee
            // rounds the owner's cut down, in the suppliers' favour.
            let interest = self
                .current_borrowed_amount()?
                .saturating_sub(borrowed_before);
            let fee = mul_div_floor(
                interest as u128,
                self.config.reserve_factor_bps as u128,
                BPS_DENOMINATOR,
            )?;
            self.accumulated_protocol_fees = self
                .accumulated_protocol_fees
                .checked_add(u64::try_from(fee).map_err(|_| LendingError::MathOverflow)?)
                .ok_or(LendingError::MathOverflow)?;
        }

        self.last_update_slot = current_slot;
        Ok(())
    }

    /// Reject use of a reserve whose interest has not been accrued this slot.
    pub fn require_refreshed(&self) -> Result<()> {
        require_eq!(
            self.last_update_slot,
            Clock::get()?.slot,
            LendingError::ReserveStale
        );
        Ok(())
    }
}
