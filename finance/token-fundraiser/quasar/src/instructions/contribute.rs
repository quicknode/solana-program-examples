use {
    crate::state::{Contributor, Fundraiser},
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct Contribute {
    #[account(mut)]
    pub contributor: Signer,
    #[account(mut)]
    pub fundraiser: Account<Fundraiser>,
    #[account(mut)]
    pub contributor_account: Account<Contributor>,
    #[account(mut)]
    pub contributor_ta: Account<Token>,
    #[account(mut)]
    pub vault: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_contribute(accounts: &mut Contribute, amount: u64) -> Result<(), ProgramError> {
    require!(amount > 0, ProgramError::InvalidArgument);

    // Transfer tokens from contributor to vault
    accounts.token_program
        .transfer(&accounts.contributor_ta, &accounts.vault, &accounts.contributor, amount)
        .invoke()?;

    // Update fundraiser state
    accounts.fundraiser.current_amount = accounts.fundraiser.current_amount.checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    // Update contributor tracking
    accounts.contributor_account.amount = accounts.contributor_account.amount.checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    Ok(())
}
