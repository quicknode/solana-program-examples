#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod instructions;
use instructions::*;
#[cfg(test)]
mod tests;

declare_id!("2phbC62wekpw95XuBk4i1KX4uA8zBUWmYbiTMhicSuBV");

#[program]
mod quasar_hello_solana {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn hello(ctx: Ctx<HelloAccountConstraints>) -> Result<(), ProgramError> {
        instructions::handle_hello(&mut ctx.accounts)
    }
}
