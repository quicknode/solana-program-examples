use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{Mint, Token2022};
use spl_token_metadata_interface::instruction::emit;

#[derive(Accounts)]
pub struct EmitAccountConstraints {
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
}

// Invoke the emit instruction from spl_token_metadata_interface directly
// There is not an anchor CpiContext for this instruction
pub fn process_emit(context: &mut Context<EmitAccountConstraints>) -> Result<()> {
    invoke(
        &emit(
            &context.accounts.token_program.address(), // token program id
            &context.accounts.mint_account.address(),  // "metadata" account
            None,
            None,
        ),
        &[
            context.accounts.token_program.cpi_handle(),
            context.accounts.mint_account.cpi_handle(),
        ],
    )?;
    Ok(())
}
