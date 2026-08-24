use anchor_lang::prelude::*;

use crate::Message;

#[derive(Accounts)]
#[instruction(input: String)]
pub struct UpdateAccountConstraints<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        realloc = Message::required_space(input.len()),
        realloc::payer = payer,
        realloc::zero = true,
    )]
    pub message_account: Account<'info, Message>,
    pub system_program: Program<'info, System>,
}

pub fn handler(context: Context<UpdateAccountConstraints>, input: String) -> Result<()> {
    context.accounts.message_account.message = input;
    Ok(())
}
