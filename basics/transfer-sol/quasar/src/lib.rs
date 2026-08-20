#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod instructions;
use instructions::*;
#[cfg(test)]
mod tests;

declare_id!("4fQVnLWKKKYxtxgGn7Haw8v2g2Hzbu8K61JvWKvqAi7W");

#[program]
mod quasar_transfer_sol {
    use super::*;

    /// Transfer SOL from payer to recipient via system program CPI.
    #[instruction(discriminator = 0)]
    pub fn transfer_sol_with_cpi(
        ctx: Ctx<TransferSolWithCpiAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_transfer_sol_with_cpi(&mut ctx.accounts, amount)
    }

    /// Transfer SOL by directly manipulating lamports.
    /// The payer account must be owned by this program.
    #[instruction(discriminator = 1)]
    pub fn transfer_sol_with_program(
        ctx: Ctx<TransferSolWithProgramAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_transfer_sol_with_program(&mut ctx.accounts, amount)
    }
}
