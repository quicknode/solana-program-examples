use crate::state::{Counter, CounterInner};
use quasar_lang::prelude::*;

/// Accounts for creating a new counter.
/// The counter is derived as a PDA from ["counter", payer] seeds.
#[derive(Accounts)]
pub struct InitializeCounter {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut, init, payer = payer, address = Counter::seeds(payer.address()))]
    pub counter: Account<Counter>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_counter(accounts: &mut InitializeCounter) -> Result<(), ProgramError> {
    accounts.counter.set_inner(CounterInner { count: 0 });
    Ok(())
}
