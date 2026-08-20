use anchor_lang::prelude::*;
use anchor_spl::token;
use anchor_spl::token_interface::{memo_transfer_disable, MemoTransfer, Token2022, TokenAccount};

#[derive(Accounts)]
pub struct DisableAccountConstraints {
    #[account(mut)]
    pub owner: Signer,

    #[account(
        mut,
        token::authority = owner,
    )]
    pub token_account: InterfaceAccount<TokenAccount>,
    pub token_program: Program<Token2022>,
}

pub fn handler(context: &mut Context<DisableAccountConstraints>) -> Result<()> {
    memo_transfer_disable(CpiContext::new(
        context.accounts.token_program.address(),
        MemoTransfer {
            account: context.accounts.token_account.cpi_handle_mut(),
            owner: context.accounts.owner.cpi_handle(),
        },
    ))?;
    Ok(())
}
