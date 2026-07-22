use quasar_lang::cpi::Seed;
use {
    crate::{
        error::FundraiserError,
        state::{fundraiser_deadline, Contributor, Fundraiser},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct RefundAccountConstraints {
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
        close(dest = contributor),
        address = Contributor::seeds(fundraiser.address(), contributor.address()),
    )]
    pub contributor_account: Account<Contributor>,

    #[account(mut)]
    pub contributor_ta: Account<Token>,

    #[account(mut)]
    pub vault: Account<Token>,

    // Bound to fundraiser.mint_to_raise by has_one above; carries the decimals
    // that transfer_checked validates against the vault and contributor_ta.
    pub mint_to_raise: Account<Mint>,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_refund(accounts: &mut RefundAccountConstraints, bumps: &RefundAccountConstraintsBumps) -> Result<(), ProgramError> {
    // Refunds are allowed only after the deadline (now >= start + duration).
    let now: i64 = Clock::get()?.unix_timestamp.into();
    let deadline = fundraiser_deadline(
        accounts.fundraiser.time_started.into(),
        accounts.fundraiser.duration.into(),
    )?;
    require!(now >= deadline, FundraiserError::FundraiserNotEnded);

    // Refunds are allowed only when the target was not met. A successful
    // fundraiser pays out to the maker via check_contributions instead.
    let current_amount: u64 = accounts.fundraiser.current_amount.into();
    let amount_to_raise: u64 = accounts.fundraiser.amount_to_raise.into();
    require!(current_amount < amount_to_raise, FundraiserError::TargetMet);

    let refund_amount: u64 = accounts.contributor_account.amount.into();

    // Update state before the transfer CPI (checks-effects-interactions).
    accounts.fundraiser.current_amount = PodU64::from(
        current_amount
            .checked_sub(refund_amount)
            .ok_or(FundraiserError::MathOverflow)?,
    );
    accounts.contributor_account.amount = PodU64::from(0);

    // Fundraiser PDA signer seeds: ["fundraiser", maker, bump].
    let bump = [bumps.fundraiser];
    let seeds = [
        Seed::from(b"fundraiser" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    let vault_balance_before = accounts.vault.amount();

    accounts
        .token_program
        .transfer_checked(
            &accounts.vault,
            &accounts.mint_to_raise,
            &accounts.contributor_ta,
            &accounts.fundraiser,
            refund_amount,
            accounts.mint_to_raise.decimals(),
        )
        .invoke_signed(&seeds)?;

    // Token conservation: the vault lost exactly the refunded amount.
    let expected_vault_balance = vault_balance_before
        .checked_sub(refund_amount)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        accounts.vault.amount() == expected_vault_balance,
        FundraiserError::BalanceMismatch
    );

    Ok(())
}
