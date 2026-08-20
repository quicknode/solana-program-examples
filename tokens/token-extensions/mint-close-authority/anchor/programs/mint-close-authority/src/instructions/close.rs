use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{close_account, CloseAccount},
    token_interface::{Mint, Token2022},
};

#[derive(Accounts)]
pub struct CloseAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    // Token-2022 checks the mint's close authority against the signer when the
    // CloseAccount CPI below runs, so no constraint is needed here.
    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
}

pub fn handler(context: &mut Context<CloseAccountConstraints>) -> Result<()> {
    // `authority` fills both the destination and authority CPI slots. v2's typed
    // handles enforce borrow exclusivity at compile time, so the read-only slot
    // is built from a copy of the `AccountView`, and it still points at the same
    // underlying account.
    let authority_view = *context.accounts.authority.account();

    // cpi to token extensions programs to close mint account
    // alternatively, this can also be done in the client
    close_account(CpiContext::new(
        context.accounts.token_program.address(),
        CloseAccount {
            account: context.accounts.mint_account.cpi_handle_mut(),
            destination: context.accounts.authority.cpi_handle_mut(),
            authority: CpiHandle::readonly(&authority_view),
        },
    ))?;
    Ok(())
}
