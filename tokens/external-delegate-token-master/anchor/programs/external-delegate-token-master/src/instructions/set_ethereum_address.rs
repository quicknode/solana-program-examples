use anchor_lang::prelude::*;

use crate::UserAccount;

#[derive(Accounts)]
pub struct SetEthereumAddressAccountConstraints {
    #[account(mut)]
    pub user_account: BorshAccount<UserAccount>,

    #[account(address = user_account.authority)]
    pub authority: Signer,
}

pub fn handler(
    context: &mut Context<SetEthereumAddressAccountConstraints>,
    ethereum_address: [u8; 20],
) -> Result<()> {
    let user_account = &mut context.accounts.user_account;
    user_account.ethereum_address = ethereum_address;
    Ok(())
}
