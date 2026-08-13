use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::token_interface::{
    default_account_state_update, DefaultAccountStateUpdate, Mint, Token2022,
};

use crate::AnchorAccountState;

#[derive(Accounts)]
pub struct UpdateDefaultStateAccountConstraints {
    #[account(mut)]
    pub freeze_authority: Signer,
    #[account(
        mut,
        mint::freeze_authority = freeze_authority,
    )]
    pub mint_account: InterfaceAccount<Mint>,

    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

pub fn handler(
    context: &mut Context<UpdateDefaultStateAccountConstraints>,
    account_state: AnchorAccountState,
) -> Result<()> {
    // Convert AnchorAccountState to spl_token_2022::state::AccountState
    let account_state = account_state.to_spl_account_state();

    default_account_state_update(
        CpiContext::new(
            context.accounts.token_program.address(),
            DefaultAccountStateUpdate {
                mint: context.accounts.mint_account.cpi_handle_mut(),
                freeze_authority: context.accounts.freeze_authority.cpi_handle(),
            },
        ),
        &account_state,
    )?;
    Ok(())
}
