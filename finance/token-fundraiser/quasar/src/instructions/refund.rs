use {
    crate::state::{Contributor, ContributorInner, Fundraiser},
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct Refund {
    #[account(mut)]
    pub contributor: Signer,
    pub maker: UncheckedAccount,
    #[account(
        mut,
        has_one(maker),
        address = Fundraiser::seeds(maker.address()),
    )]
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
pub fn handle_refund(accounts: &mut Refund, bumps: &RefundBumps) -> Result<(), ProgramError> {
    let refund_amount = accounts.contributor_account.amount;

    // Build PDA signer seeds inline; see comment in check_contributions.rs
    // for why we no longer use a struct helper method.
    let bump = [bumps.fundraiser];
    let seeds = [
        Seed::from(b"fundraiser" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    // Transfer contributor's tokens back from vault
    accounts.token_program
        .transfer(&accounts.vault, &accounts.contributor_ta, &accounts.fundraiser, refund_amount)
        .invoke_signed(&seeds)?;

    // Update fundraiser state
    accounts.fundraiser.current_amount = accounts.fundraiser.current_amount
        .checked_sub(refund_amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    // Zero out contributor amount
    accounts.contributor_account.set_inner(ContributorInner { amount: 0 });

    Ok(())
}
