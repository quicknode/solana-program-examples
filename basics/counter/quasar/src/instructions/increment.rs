use {
    crate::{error::CounterError, state::Counter},
    quasar_lang::prelude::*,
};

/// Accounts for incrementing a counter.
#[derive(Accounts)]
pub struct IncrementAccountConstraints {
    #[account(mut)]
    pub counter: Account<Counter>,
}

#[inline(always)]
pub fn handle_increment(accounts: &mut IncrementAccountConstraints) -> Result<(), ProgramError> {
    let current: u64 = accounts.counter.count.into();
    let next = current.checked_add(1).ok_or(CounterError::MathOverflow)?;
    accounts.counter.count = PodU64::from(next);
    Ok(())
}
