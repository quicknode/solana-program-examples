use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    default_account_state_update, DefaultAccountStateUpdate, Mint, Token2022,
};

use crate::AnchorAccountState;

#[derive(Accounts)]
pub struct UpdateDefaultStateAccountConstraints<'info> {
    #[account(mut)]
    pub freeze_authority: Signer<'info>,
    #[account(
        mut,
        mint::freeze_authority = freeze_authority,
    )]
    pub mint_account: InterfaceAccount<'info, Mint>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn handler(
    context: Context<UpdateDefaultStateAccountConstraints>,
    account_state: AnchorAccountState,
) -> Result<()> {
    // Convert AnchorAccountState to spl_token_2022::state::AccountState
    let account_state = account_state.to_spl_account_state();

    default_account_state_update(
        CpiContext::new(
            context.accounts.token_program.key(),
            DefaultAccountStateUpdate {
                token_program_id: context.accounts.token_program.to_account_info(),
                mint: context.accounts.mint_account.to_account_info(),
                freeze_authority: context.accounts.freeze_authority.to_account_info(),
            },
        ),
        &account_state,
    )?;
    Ok(())
}
