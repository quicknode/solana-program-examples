use quasar_lang::cpi::Seed;
use {
    crate::{
        error::FundraiserError,
        state::{fundraiser_deadline, Fundraiser},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CloseFundraiserAccountConstraints {
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

/// Closes a finished fundraiser and its vault so the maker can raise again.
///
/// The fundraiser PDA is derived from the maker's address alone, so while a
/// fundraiser account exists the maker cannot initialize another one. It
/// closes once no contributor account written for it is still open: after a
/// claim, once `close_contributor` has closed each one; after a failed raise,
/// once the deadline has passed and `refund` has closed each one.
#[inline(always)]
pub fn handle_close_fundraiser(
    accounts: &mut CloseFundraiserAccountConstraints,
    bumps: &CloseFundraiserAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    if !bool::from(accounts.fundraiser.claimed) {
        // Closing an unclaimed fundraiser is allowed only after it has ended
        // (now >= start + duration).
        let now: i64 = Clock::get()?.unix_timestamp.into();
        let deadline = fundraiser_deadline(
            accounts.fundraiser.time_started.into(),
            accounts.fundraiser.duration.into(),
        )?;
        require!(now >= deadline, FundraiserError::FundraiserNotEnded);

        // A raise that met its target closes after the maker claims it.
        let current_amount: u64 = accounts.fundraiser.current_amount.into();
        let amount_to_raise: u64 = accounts.fundraiser.amount_to_raise.into();
        require!(current_amount < amount_to_raise, FundraiserError::TargetMet);

        // Closing the vault while contributions remain would strand the
        // refunds, so every contributor must have been refunded first.
        require!(current_amount == 0, FundraiserError::RefundsOutstanding);
    }

    // A contributor account left open would be read as a contribution to the
    // next fundraiser at this address.
    let open_contributor_accounts: u32 = accounts.fundraiser.open_contributor_accounts.into();
    require!(
        open_contributor_accounts == 0,
        FundraiserError::ContributorAccountsOpen
    );

    // Fundraiser PDA signer seeds: ["fundraiser", maker, bump].
    let bump = [bumps.fundraiser];
    let seeds = [
        Seed::from(b"fundraiser" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    // The claim or the refunds have already paid out every tracked
    // contribution, so anything left in the vault is a direct donation; pay
    // it to the maker rather than burn it with the account.
    let vault_amount = accounts.vault.amount();
    if vault_amount > 0 {
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
    }

    // Close the empty vault, returning its rent to the maker. The
    // `close(dest = maker)` constraint then closes the fundraiser account.
    accounts
        .token_program
        .close_account(&accounts.vault, &accounts.maker, &accounts.fundraiser)
        .invoke_signed(&seeds)?;

    Ok(())
}
