use {
    crate::state::Fundraiser,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CheckContributions {
    #[account(mut)]
    pub maker: Signer,
    #[account(
        mut,
        has_one(maker),
        close(dest = maker),
        address = Fundraiser::seeds(maker.address()),
    )]
    pub fundraiser: Account<Fundraiser>,
    #[account(mut)]
    pub vault: Account<Token>,
    #[account(mut)]
    pub maker_ta: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_check_contributions(accounts: &mut CheckContributions, bumps: &CheckContributionsBumps) -> Result<(), ProgramError> {
    // Verify the target was met
    require!(
        accounts.fundraiser.current_amount >= accounts.fundraiser.amount_to_raise,
        ProgramError::Custom(0) // TargetNotMet
    );

    // Build PDA signer seeds for the fundraiser:
    // ["fundraiser", maker, bump]. Inline rather than via a helper because
    // post-PR-#195 the derive no longer emits a `<struct>_seeds()` method.
    let bump = [bumps.fundraiser];
    let seeds = [
        Seed::from(b"fundraiser" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    // Transfer all vault funds to the maker
    let vault_amount = accounts.vault.amount();
    accounts.token_program
        .transfer(&accounts.vault, &accounts.maker_ta, &accounts.fundraiser, vault_amount)
        .invoke_signed(&seeds)?;

    // Close the vault token account
    accounts.token_program
        .close_account(&accounts.vault, &accounts.maker, &accounts.fundraiser)
        .invoke_signed(&seeds)?;

    Ok(())
}
