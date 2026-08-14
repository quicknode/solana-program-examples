use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{Mint, Token2022};
use spl_token_metadata_interface::instruction::remove_key;

#[derive(Accounts)]
pub struct RemoveKeyAccountConstraints {
    #[account(mut)]
    pub update_authority: Signer,

    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

// Invoke the remove_key instruction from spl_token_metadata_interface directly
// There is not an anchor CpiContext for this instruction
pub fn process_remove_key(
    context: &mut Context<RemoveKeyAccountConstraints>,
    key: String,
) -> Result<()> {
    invoke(
        &remove_key(
            &context.accounts.token_program.address(), // token program id
            &context.accounts.mint_account.address(),  // "metadata" account
            &context.accounts.update_authority.address(), // update authority
            key,                                       // key to remove
            true, // idempotent flag, if true transaction will not fail if key does not exist
        ),
        &[
            context.accounts.token_program.cpi_handle(),
            context.accounts.mint_account.cpi_handle(),
            context.accounts.update_authority.cpi_handle(),
        ],
    )?;
    Ok(())
}
