use anchor_lang::prelude::*;

use crate::Message;

#[derive(Accounts)]
#[instruction(input: String)]
pub struct InitializeAccountConstraints<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = Message::required_space(input.len()),
    )]
    pub message_account: Account<'info, Message>,
    pub system_program: Program<'info, System>,
}

pub fn handler(context: Context<InitializeAccountConstraints>, input: String) -> Result<()> {
    context.accounts.message_account.message = input;
    Ok(())
}
