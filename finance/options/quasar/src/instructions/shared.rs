//! The pure contract math and the custody check, ported from the Anchor
//! sibling (`options::contract_math` and `instructions::shared`). There is no
//! division anywhere in settlement: every amount is a product of two of the
//! lot's integers, and the only rounding is the floor in the fee split.

use {
    crate::{
        constants::{BASIS_POINTS_DENOMINATOR, KIND_CALL, KIND_PUT},
        errors::OptionsError,
        state::Market,
    },
    quasar_lang::{cpi::Seed, prelude::*},
    quasar_spl::prelude::*,
};

/// A lot's terms, as read from the account or the instruction arguments.
#[derive(Clone, Copy)]
pub struct Terms {
    pub kind: u8,
    pub contracts: u64,
    pub underlying_per_contract: u64,
    pub strike_per_contract: u64,
}

impl Terms {
    pub fn is_call(&self) -> bool {
        self.kind == KIND_CALL
    }

    /// `contracts * underlying_per_contract`.
    pub fn underlying_total(&self) -> Result<u64, ProgramError> {
        self.contracts
            .checked_mul(self.underlying_per_contract)
            .ok_or_else(|| OptionsError::MathOverflow.into())
    }

    /// `contracts * strike_per_contract`.
    pub fn strike_total(&self) -> Result<u64, ProgramError> {
        self.contracts
            .checked_mul(self.strike_per_contract)
            .ok_or_else(|| OptionsError::MathOverflow.into())
    }

    /// What the writer posts: the underlying for a call, the strike for a put.
    pub fn collateral_amount(&self) -> Result<u64, ProgramError> {
        if self.is_call() {
            self.underlying_total()
        } else {
            self.strike_total()
        }
    }

    /// What the holder pays at exercise and the writer later collects: the
    /// mirror of `collateral_amount`, in the other token.
    pub fn exercise_payment(&self) -> Result<u64, ProgramError> {
        if self.is_call() {
            self.strike_total()
        } else {
            self.underlying_total()
        }
    }
}

/// Which of the two kinds a `kind` argument names, or an error.
pub fn require_valid_kind(kind: u8) -> Result<(), ProgramError> {
    require!(
        kind == KIND_CALL || kind == KIND_PUT,
        OptionsError::InvalidParameter
    );
    Ok(())
}

/// Split a premium into the venue's fee and the writer's share. The fee
/// floors, so the writer receives the rounding minor unit.
pub fn split_premium(premium: u64, fee_bps: u16) -> Result<(u64, u64), ProgramError> {
    let fee = (premium as u128)
        .checked_mul(fee_bps as u128)
        .ok_or(OptionsError::MathOverflow)?
        .checked_div(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or(OptionsError::MathOverflow)?;
    let fee = u64::try_from(fee).map_err(|_| OptionsError::MathOverflow)?;
    let to_writer = premium.checked_sub(fee).ok_or(OptionsError::MathOverflow)?;
    Ok((fee, to_writer))
}

/// The holder may exercise while the option has not expired.
pub fn may_exercise(now: i64, expiry: i64) -> bool {
    now < expiry
}

/// The writer may reclaim once the option has expired: the exact complement
/// of `may_exercise`.
pub fn may_reclaim(now: i64, expiry: i64) -> bool {
    now >= expiry
}

/// The custody invariant, asserted after the math in every handler that
/// moves tokens: each vault covers what the market owes. The two arguments
/// are the vault balances the handler's transfers will leave behind.
pub fn check_custody(
    market: &Account<Market>,
    underlying_after: u64,
    quote_after: u64,
) -> Result<(), ProgramError> {
    require!(
        underlying_after >= market.underlying_locked.get(),
        OptionsError::CustodyInvariantViolated
    );
    let quote_owed = market
        .quote_locked
        .get()
        .checked_add(market.fees_owed.get())
        .ok_or(OptionsError::MathOverflow)?;
    require!(
        quote_after >= quote_owed,
        OptionsError::CustodyInvariantViolated
    );
    Ok(())
}

/// Add `amount` to a ledger counter and to the matching projected balance.
pub fn add_locked(
    counter: &mut PodU64,
    balance: &mut u64,
    amount: u64,
) -> Result<(), ProgramError> {
    counter.set(
        counter
            .get()
            .checked_add(amount)
            .ok_or(OptionsError::MathOverflow)?,
    );
    *balance = balance
        .checked_add(amount)
        .ok_or(OptionsError::MathOverflow)?;
    Ok(())
}

/// Subtract `amount` from a ledger counter and from the matching projected
/// balance. A balance that cannot cover the subtraction is a custody failure.
pub fn sub_locked(
    counter: &mut PodU64,
    balance: &mut u64,
    amount: u64,
) -> Result<(), ProgramError> {
    counter.set(
        counter
            .get()
            .checked_sub(amount)
            .ok_or(OptionsError::MathOverflow)?,
    );
    *balance = balance
        .checked_sub(amount)
        .ok_or(OptionsError::CustodyInvariantViolated)?;
    Ok(())
}

/// A transfer out of a vault, signed by the market's vault authority PDA.
pub fn transfer_from_vault(
    token_program: &Program<TokenProgram>,
    vault: &Account<Token>,
    mint: &Account<Mint>,
    to: &Account<Token>,
    market_authority: &UncheckedAccount,
    market: &Account<Market>,
    amount: u64,
) -> Result<(), ProgramError> {
    let bump = [market.authority_bump];
    let market_address = *market.address();
    let seeds: &[Seed] = &[
        Seed::from(b"authority".as_ref()),
        Seed::from(market_address.as_ref()),
        Seed::from(&bump as &[u8]),
    ];
    token_program
        .transfer_checked(vault, mint, to, market_authority, amount, mint.decimals())
        .invoke_signed(seeds)
}
