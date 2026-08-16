use anchor_lang::prelude::*;

use crate::Message;

#[derive(Accounts)]
#[instruction(_input: String)]
pub struct InitializeAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        init,
        payer = payer,
        space = Message::required_space(input.len()),
    )]
    pub message_account: BorshAccount<Message>,
    pub system_program: Program<System>,
}

pub fn handler(context: &mut Context<InitializeAccountConstraints>, input: String) -> Result<()> {
    context.accounts.message_account.message = input;
    Ok(())
}
