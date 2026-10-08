use {
    crate::{
        error::FundraiserError,
        state::{fundraiser_deadline, one_major_unit, Contribution, Fundraiser},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct ContributeAccountConstraints {
    #[account(mut)]
    pub contributor: Signer,

    pub maker: UncheckedAccount,

    #[account(
        mut,
        has_one(maker),
        has_one(vault),
        has_one(mint_to_raise),
        address = Fundraiser::seeds(maker.address()),
    )]
    pub fundraiser: Account<Fundraiser>,

    #[account(
        mut,
        init(idempotent),
        payer = contributor,
        address = Contribution::seeds(fundraiser.address(), contributor.address()),
    )]
    pub contribution: Account<Contribution>,

    #[account(mut)]
    pub contributor_ta: Account<Token>,

    #[account(mut)]
    pub vault: Account<Token>,

    // Bound to fundraiser.mint_to_raise by has_one above; carries the decimals
    // that transfer_checked validates against contributor_ta and vault.
    pub mint_to_raise: Account<Mint>,

    pub token_program: Program<TokenProgram>,

    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_contribute(
    accounts: &mut ContributeAccountConstraints,
    amount: u64,
    bumps: &ContributeAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    // The minimum contribution is one major unit, which is 10^decimals minor
    // units.
    require!(
        amount >= one_major_unit(accounts.mint_to_raise.decimals())?,
        FundraiserError::ContributionTooSmall
    );

    // A claimed fundraiser has paid its vault out to the maker, so a later
    // contribution would go to the maker with no refund path.
    require!(
        !bool::from(accounts.fundraiser.claimed),
        FundraiserError::FundraiserClaimed
    );

    // Contributions are allowed while now < start + duration.
    let now: i64 = Clock::get()?.unix_timestamp.into();
    let deadline = fundraiser_deadline(
        accounts.fundraiser.time_started.into(),
        accounts.fundraiser.duration.into(),
    )?;
    require!(now < deadline, FundraiserError::FundraiserEnded);

    let current_amount: u64 = accounts.fundraiser.current_amount.into();
    accounts.fundraiser.current_amount = PodU64::from(
        current_amount
            .checked_add(amount)
            .ok_or(FundraiserError::MathOverflow)?,
    );

    let contributed_so_far: u64 = accounts.contribution.amount.into();
    accounts.contribution.amount = PodU64::from(
        contributed_so_far
            .checked_add(amount)
            .ok_or(FundraiserError::MathOverflow)?,
    );

    // `init(idempotent)` creates the contribution account zeroed and reuses it
    // on later contributions. Every contribution is at least one major unit, so a recorded
    // amount of zero means the account was created by this instruction: save
    // its bump and count it against the fundraiser.
    if contributed_so_far == 0 {
        accounts.contribution.bump = bumps.contribution;
        let open_contributions: u32 = accounts.fundraiser.open_contributions.into();
        accounts.fundraiser.open_contributions = PodU32::from(
            open_contributions
                .checked_add(1)
                .ok_or(FundraiserError::MathOverflow)?,
        );
    }

    let vault_balance_before = accounts.vault.amount();

    accounts
        .token_program
        .transfer_checked(
            &accounts.contributor_ta,
            &accounts.mint_to_raise,
            &accounts.vault,
            &accounts.contributor,
            amount,
            accounts.mint_to_raise.decimals(),
        )
        .invoke()?;

    // Token conservation: the vault gained exactly the contributed amount.
    let expected_vault_balance = vault_balance_before
        .checked_add(amount)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        accounts.vault.amount() == expected_vault_balance,
        FundraiserError::BalanceMismatch
    );

    Ok(())
}
