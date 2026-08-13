use anchor_lang::prelude::*;

use crate::constants::MAX_OBLIGATION_RESERVES;
use crate::errors::LendingError;

/// A borrower's position in one lending market: the share-token collateral they
/// have posted and the liquidity they have borrowed, plus the cached quote-
/// currency valuations that `refresh_obligation` recomputes.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Obligation {
    pub lending_market: Address,

    pub owner: Address,

    pub last_update_slot: u64,

    /// Set whenever deposits/borrows change; cleared by `refresh_obligation`.
    /// Health-dependent handlers reject a stale obligation so they never act on
    /// cached values that a prior instruction in the same transaction invalidated.
    pub stale: bool,

    /// Sum of every deposit's market value, FIXED_POINT_SCALE-scaled.
    pub deposited_value: u128,

    /// Sum of every borrow's market value, FIXED_POINT_SCALE-scaled.
    pub borrowed_value: u128,

    /// Σ (deposit value * reserve loan_to_value). Borrows may not exceed this.
    pub allowed_borrow_value: u128,

    /// Σ (deposit value * reserve liquidation_threshold). Above this the
    /// obligation is liquidatable.
    pub unhealthy_borrow_value: u128,

    #[max_len(MAX_OBLIGATION_RESERVES)]
    pub deposits: Vec<ObligationCollateral>,

    #[max_len(MAX_OBLIGATION_RESERVES)]
    pub borrows: Vec<ObligationLiquidity>,

    pub bump: u8,
}

#[derive(InitSpace, Clone, Copy, AnchorSerialize, AnchorDeserialize, Debug, Default)]
pub struct ObligationCollateral {
    pub reserve: Address,
    pub deposited_shares: u64,
    pub market_value: u128,
}

#[derive(InitSpace, Clone, Copy, AnchorSerialize, AnchorDeserialize, Debug, Default)]
pub struct ObligationLiquidity {
    pub reserve: Address,
    /// Borrowed principal, scaled by the reserve's index at borrow time so the
    /// live debt grows automatically as that index advances:
    /// `debt = borrowed_principal * reserve.borrow_accumulation_factor / FIXED_POINT_SCALE`.
    pub borrowed_principal: u128,
    pub market_value: u128,
}

impl Obligation {
    /// Reject a health-dependent action when the obligation has not been
    /// refreshed in this same transaction.
    pub fn require_refreshed(&self) -> Result<()> {
        require!(!self.stale, LendingError::ObligationStale);
        require_eq!(
            self.last_update_slot,
            Clock::get()?.slot,
            LendingError::ObligationStale
        );
        Ok(())
    }

    /// Index of the collateral entry for `reserve`, creating an empty one if the
    /// obligation has room. Used when posting collateral.
    pub fn upsert_collateral(&mut self, reserve: Address) -> Result<usize> {
        if let Some(index) = self.deposits.iter().position(|entry| entry.reserve == reserve) {
            return Ok(index);
        }
        require!(
            self.deposits.len() < MAX_OBLIGATION_RESERVES,
            LendingError::TooManyReserves
        );
        self.deposits.push(ObligationCollateral {
            reserve,
            deposited_shares: 0,
            market_value: 0,
        });
        Ok(self.deposits.len() - 1)
    }

    /// Index of the borrow entry for `reserve`, creating an empty one if the
    /// obligation has room. Used when borrowing.
    pub fn upsert_borrow(&mut self, reserve: Address) -> Result<usize> {
        if let Some(index) = self.borrows.iter().position(|entry| entry.reserve == reserve) {
            return Ok(index);
        }
        require!(
            self.borrows.len() < MAX_OBLIGATION_RESERVES,
            LendingError::TooManyReserves
        );
        self.borrows.push(ObligationLiquidity {
            reserve,
            borrowed_principal: 0,
            market_value: 0,
        });
        Ok(self.borrows.len() - 1)
    }

    pub fn find_collateral(&self, reserve: Address) -> Result<usize> {
        self.deposits
            .iter()
            .position(|entry| entry.reserve == reserve)
            .ok_or(LendingError::ReserveNotFound.into())
    }

    pub fn find_borrow(&self, reserve: Address) -> Result<usize> {
        self.borrows
            .iter()
            .position(|entry| entry.reserve == reserve)
            .ok_or(LendingError::ReserveNotFound.into())
    }
}
