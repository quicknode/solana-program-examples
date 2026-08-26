use anchor_lang::prelude::*;

use crate::WhiteList;

#[derive(Accounts)]
pub struct AddToWhiteListAccountConstraints {
    /// CHECK: New account to add to white list
    #[account()]
    pub new_account: UncheckedAccount,
    #[account(
        mut,
        seeds = [b"white_list"],
        bump = white_list.bump
    )]
    pub white_list: BorshAccount<WhiteList>,
    #[account(mut)]
    pub signer: Signer,
}

pub fn handler(context: &mut Context<AddToWhiteListAccountConstraints>) -> Result<()> {
    if context.accounts.white_list.authority != *context.accounts.signer.address() {
        panic!("Only the authority can add to the white list!");
    }

    context
        .accounts
        .white_list
        .white_list
        .push(*context.accounts.new_account.address());
    msg!(
        "New account white listed! {0}",
        context.accounts.new_account.address().to_string()
    );
    msg!(
        "White list length! {0}",
        context.accounts.white_list.white_list.len()
    );

    Ok(())
}
