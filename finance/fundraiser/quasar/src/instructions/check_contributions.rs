use quasar_lang::cpi::Seed;
use {
    crate::{error::FundraiserError, state::Fundraiser},
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CheckContributionsAccountConstraints {
    pub maker: Signer,

    #[account(
        mut,
        has_one(maker),
        has_one(vault),
        has_one(mint_to_raise),
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

/// Pays the vault out to the maker once the target is met, and marks the
/// fundraiser claimed.
///
/// The fundraiser account and the vault stay open: contribution accounts are
/// derived from the fundraiser's address, so the fundraiser must outlive every
/// one of them. Otherwise the maker could initialize a new fundraiser at the
/// same address, and contribution accounts left over from this raise would
/// count as contributions to the new one. `close_contribution` closes them,
/// then `close_fundraiser` closes the fundraiser and the vault.
#[inline(always)]
pub fn handle_check_contributions(
    accounts: &mut CheckContributionsAccountConstraints,
    bumps: &CheckContributionsAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require!(
        !bool::from(accounts.fundraiser.claimed),
        FundraiserError::FundraiserClaimed
    );

    // Compare the state-tracked total, not the vault balance, so tokens
    // donated directly to the vault cannot trigger an early release.
    let current_amount: u64 = accounts.fundraiser.current_amount.into();
    let amount_to_raise: u64 = accounts.fundraiser.amount_to_raise.into();
    require!(
        current_amount >= amount_to_raise,
        FundraiserError::TargetNotMet
    );

    accounts.fundraiser.claimed = PodBool::from(true);

    // Fundraiser PDA signer seeds: ["fundraiser", maker, bump].
    let bump = [bumps.fundraiser];
    let seeds = [
        Seed::from(b"fundraiser" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    // Pay the whole vault (including any direct donations) to the maker.
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

    // Token conservation: the vault was fully paid out.
    require!(
        accounts.vault.amount() == 0,
        FundraiserError::BalanceMismatch
    );

    Ok(())
}
