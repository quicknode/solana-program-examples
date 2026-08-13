use anchor_lang::prelude::*;

use crate::Message;

#[derive(Accounts)]
#[instruction(input: String)]
pub struct UpdateAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        mut,
        realloc = Message::required_space(input.len()),
        realloc_payer = payer,
        realloc_zero = true,
    )]
    pub message_account: BorshAccount<Message>,
    pub system_program: Program<System>,
}

pub fn handler(context: &mut Context<UpdateAccountConstraints>, input: String) -> Result<()> {
    context.accounts.message_account.message = input;
    Ok(())
}
