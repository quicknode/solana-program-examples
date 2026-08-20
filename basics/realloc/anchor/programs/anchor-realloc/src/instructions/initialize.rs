// v2's `#[derive(Accounts)]` binds the `#[instruction(...)]` args in more
// than one generated item, and only the one evaluating the constraints below
// reads them, so the binding looks unused to rustc even though `space` uses it.
#![allow(unused_variables)]

use anchor_lang::prelude::*;

use crate::Message;

#[derive(Accounts)]
#[instruction(input: String)]
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
