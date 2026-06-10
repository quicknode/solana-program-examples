use {
    crate::{
        error::FundraiserError,
        state::{fundraiser_deadline, Contributor, Fundraiser},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct Contribute {
    #[account(mut)]
    pub contributor: Signer,

    pub maker: UncheckedAccount,

    #[account(
        mut,
        has_one(maker),
        has_one(vault),
        address = Fundraiser::seeds(maker.address()),
    )]
    pub fundraiser: Account<Fundraiser>,

    #[account(
        mut,
        init(idempotent),
        payer = contributor,
        address = Contributor::seeds(fundraiser.address(), contributor.address()),
    )]
    pub contributor_account: Account<Contributor>,

    #[account(mut)]
    pub contributor_ta: Account<Token>,

    #[account(mut)]
    pub vault: Account<Token>,

    pub token_program: Program<TokenProgram>,

    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_contribute(
    accounts: &mut Contribute,
    amount: u64,
    bumps: &ContributeBumps,
) -> Result<(), ProgramError> {
    require!(amount > 0, FundraiserError::InvalidAmount);

    // Contributions are allowed while now < start + duration.
    let now: i64 = Clock::get()?.unix_timestamp.into();
    let deadline = fundraiser_deadline(
        accounts.fundraiser.time_started.into(),
        accounts.fundraiser.duration.into(),
    )?;
    require!(now < deadline, FundraiserError::FundraiserEnded);

    // Update state before the transfer CPI (checks-effects-interactions).
    let current_amount: u64 = accounts.fundraiser.current_amount.into();
    accounts.fundraiser.current_amount = PodU64::from(
        current_amount
            .checked_add(amount)
            .ok_or(FundraiserError::MathOverflow)?,
    );

    let contributed_so_far: u64 = accounts.contributor_account.amount.into();
    accounts.contributor_account.amount = PodU64::from(
        contributed_so_far
            .checked_add(amount)
            .ok_or(FundraiserError::MathOverflow)?,
    );
    accounts.contributor_account.bump = bumps.contributor_account;

    let vault_balance_before = accounts.vault.amount();

    accounts
        .token_program
        .transfer(
            &accounts.contributor_ta,
            &accounts.vault,
            &accounts.contributor,
            amount,
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
