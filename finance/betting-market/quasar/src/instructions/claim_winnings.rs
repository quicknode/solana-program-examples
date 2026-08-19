use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::BettingError;
use crate::state::{remove_bet, snapshot_user, Bet, Event, EventStatus, EventVaultPda, User};

use super::transfer_from_vault;

#[derive(Accounts)]
pub struct ClaimWinningsAccountConstraints {
    #[account(mut)]
    pub bettor: Signer,

    pub token_mint: Account<Mint>,

    #[account(address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,

    // Closing the Bet ends the position: the rent goes back to the bettor and a
    // second claim fails because the account no longer exists.
    #[account(
        mut,
        close(dest = bettor),
        has_one(bettor),
        has_one(event),
    )]
    pub bet: Account<Bet>,

    #[account(mut, address = User::seeds(bettor.address()))]
    pub user: Account<User>,

    #[account(mut)]
    pub bettor_token_account: Account<Token>,

    #[account(mut, address = EventVaultPda::seeds(event.address()))]
    pub vault: InterfaceAccount<Token>,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_claim_winnings(
    accounts: &mut ClaimWinningsAccountConstraints,
) -> Result<(), ProgramError> {
    // Canonical-PDA check for the bet account. The pre-0.1.0 constraint
    // `address = Bet::seeds(&bet.outcome, ...)` is inexpressible in 0.1.0
    // (an Address-typed stored-data seed cannot both feed client codegen and
    // typecheck on-chain), and the generated `Bet::find_address` helper is a
    // const-context/client function whose software SHA-256 exhausts the CU
    // budget on-chain. Verifying against the stored bump costs one sha256
    // syscall and rejects non-canonical bet accounts just the same.
    quasar_lang::pda::verify_program_address(
        &Bet::seeds(&accounts.bet.outcome, accounts.bettor.address())
            .with_bump(accounts.bet.bump)
            .as_slices(),
        &crate::ID,
        accounts.bet.address(),
    )?;

    require!(
        accounts.event.status == EventStatus::Settled as u8,
        BettingError::EventNotSettled
    );
    require!(
        accounts.bet.outcome_index == accounts.event.winning_outcome_index,
        BettingError::NothingToClaim
    );

    let stake = u64::from(accounts.bet.amount);
    let winning_pool = u64::from(accounts.event.winning_pool);
    let distributable_losing_pool = u64::from(accounts.event.distributable_losing_pool);

    // Parimutuel split: winners share the losing pool in proportion to their
    // own stake. Multiply in u128 and divide once (floor a single time) so no
    // precision is thrown away.
    let winnings: u64 = (stake as u128)
        .checked_mul(distributable_losing_pool as u128)
        .ok_or(BettingError::MathOverflow)?
        .checked_div(winning_pool as u128)
        .ok_or(BettingError::MathOverflow)?
        .try_into()
        .map_err(|_| BettingError::MathOverflow)?;

    // Winners always get their own stake back on top of their winnings.
    let payout = stake
        .checked_add(winnings)
        .ok_or(BettingError::MathOverflow)?;

    // Drop the Bet from the bettor's index before the transfer (effects before
    // interactions); the Bet account itself closes when the instruction ends.
    let bet_key = *accounts.bet.address();
    let mut user = snapshot_user(&accounts.user);
    remove_bet(&mut user.bets, &mut user.bet_count, &bet_key)?;
    accounts.user.set_inner(user);

    let event_id = u64::from(accounts.event.event_id);
    let event_bump = accounts.event.bump;
    transfer_from_vault(
        &accounts.token_program,
        &accounts.vault,
        &accounts.token_mint,
        &accounts.bettor_token_account,
        &accounts.event,
        payout,
        accounts.token_mint.decimals,
        event_id,
        event_bump,
    )?;

    Ok(())
}
