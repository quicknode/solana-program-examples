use anchor_lang::prelude::*;
use anchor_spl::token_interface::{memo_transfer_disable, MemoTransfer, Token2022, TokenAccount};

#[derive(Accounts)]
pub struct DisableAccountConstraints<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        token::authority = owner,
    )]
    pub token_account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

pub fn handler(context: Context<DisableAccountConstraints>) -> Result<()> {
    memo_transfer_disable(CpiContext::new(
        context.accounts.token_program.key(),
        MemoTransfer {
            token_program_id: context.accounts.token_program.to_account_info(),
            account: context.accounts.token_account.to_account_info(),
            owner: context.accounts.owner.to_account_info(),
        },
    ))?;
    Ok(())
}
