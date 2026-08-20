use anchor_lang::prelude::*;

use crate::{Counter, CounterError};

#[derive(Accounts)]
pub struct IncrementAccountConstraints {
    #[account(mut)]
    pub counter: Account<Counter>,
}

pub fn handler(context: &mut Context<IncrementAccountConstraints>) -> Result<()> {
    context.accounts.counter.count = context
        .accounts
        .counter
        .count
        .checked_add(1)
        .ok_or(CounterError::MathOverflow)?;
    Ok(())
}
