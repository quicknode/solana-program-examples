use quasar_lang::cpi::Seed;
use {
    crate::{error::FundraiserError, state::Fundraiser},
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CheckContributionsAccountConstraints {
    #[account(mut)]
    pub maker: Signer,

    #[account(
        mut,
        has_one(maker),
        has_one(vault),
        has_one(mint_to_raise),
        close(dest = maker),
        address = Fundraiser::seeds(maker.address()),
    )]
    pub fundraiser: Account<Fundraiser>,

    #[account(mut)]
    pub vault: Account<Token>,

    #[account(mut)]
    pub maker_ta: Account<Token>,

    // Bound to fundraiser.mint_to_raise by has_one above; carries the decimals
    // that transfer_checked validates against the vault and maker_ta.
    pub mint_to_raise: Account<Mint>,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_check_contributions(
    accounts: &mut CheckContributionsAccountConstraints,
    bumps: &CheckContributionsAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    let current_amount: u64 = accounts.fundraiser.current_amount.into();
    let amount_to_raise: u64 = accounts.fundraiser.amount_to_raise.into();
    require!(
        current_amount >= amount_to_raise,
        FundraiserError::TargetNotMet
    );

    // Fundraiser PDA signer seeds: ["fundraiser", maker, bump].
    let bump = [bumps.fundraiser];
    let seeds = [
        Seed::from(b"fundraiser" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    // Transfer all vault funds to the maker.
    let vault_amount = accounts.vault.amount();
    accounts
        .token_program
        .transfer_checked(
            &accounts.vault,
            &accounts.mint_to_raise,
            &accounts.maker_ta,
            &accounts.fundraiser,
            vault_amount,
            accounts.mint_to_raise.decimals(),
        )
        .invoke_signed(&seeds)?;

    // Token conservation: the vault was fully drained.
    require!(
        accounts.vault.amount() == 0,
        FundraiserError::BalanceMismatch
    );

    // Close the vault token account, returning its rent to the maker.
    accounts
        .token_program
        .close_account(&accounts.vault, &accounts.maker, &accounts.fundraiser)
        .invoke_signed(&seeds)?;

    Ok(())
}
