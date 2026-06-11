use anchor_lang::prelude::*;

use crate::WhiteList;

#[derive(Accounts)]
pub struct AddToWhiteListAccountConstraints<'info> {
    /// CHECK: New account to add to white list
    #[account()]
    pub new_account: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"white_list"],
        bump
    )]
    pub white_list: Account<'info, WhiteList>,
    #[account(mut)]
    pub signer: Signer<'info>,
}

pub fn handler(context: Context<AddToWhiteListAccountConstraints>) -> Result<()> {
    if context.accounts.white_list.authority != context.accounts.signer.key() {
        panic!("Only the authority can add to the white list!");
    }

    context.accounts
        .white_list
        .white_list
        .push(context.accounts.new_account.key());
    msg!(
        "New account white listed! {0}",
        context.accounts.new_account.key().to_string()
    );
    msg!(
        "White list length! {0}",
        context.accounts.white_list.white_list.len()
    );

    Ok(())
}
