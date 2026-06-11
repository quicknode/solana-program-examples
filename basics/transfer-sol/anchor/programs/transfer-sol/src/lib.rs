use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("4fQVnLWKKKYxtxgGn7Haw8v2g2Hzbu8K61JvWKvqAi7W");

#[program]
pub mod transfer_sol {
    use super::*;

    pub fn transfer_sol_with_cpi(context: Context<TransferSolWithCpiAccountConstraints>, amount: u64) -> Result<()> {
        instructions::transfer_sol_with_cpi::handler(context, amount)
    }

    pub fn transfer_sol_with_program(
        context: Context<TransferSolWithProgramAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        instructions::transfer_sol_with_program::handler(context, amount)
    }
}
