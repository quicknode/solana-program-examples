#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod error;
pub mod instructions;
use instructions::*;
pub mod state;
#[cfg(test)]
mod tests;

declare_id!("BmDHboaj1kBUoinJKKSRqKfMeRKJqQqEbUj1VgzeQe4A");

#[program]
mod quasar_counter {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn initialize_counter(
        ctx: Ctx<InitializeCounterAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::handle_initialize_counter(&mut ctx.accounts)
    }

    #[instruction(discriminator = 1)]
    pub fn increment(ctx: Ctx<IncrementAccountConstraints>) -> Result<(), ProgramError> {
        instructions::handle_increment(&mut ctx.accounts)
    }
}
