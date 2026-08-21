use anchor_lang::prelude::*;

use crate::UserAccount;

#[derive(Accounts)]
pub struct SetEthereumAddressAccountConstraints<'info> {
    #[account(mut, has_one = authority)]
    pub user_account: Account<'info, UserAccount>,

    pub authority: Signer<'info>,
}

pub fn handler(
    context: Context<SetEthereumAddressAccountConstraints>,
    ethereum_address: [u8; 20],
) -> Result<()> {
    let user_account = &mut context.accounts.user_account;
    user_account.ethereum_address = ethereum_address;
    Ok(())
}
