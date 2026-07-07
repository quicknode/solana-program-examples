use quasar_lang::prelude::*;

use crate::errors::BettingError;

pub const USER_SEED: &[u8] = b"user";

/// A bettor can hold at most this many OPEN positions at once (one per outcome
/// they currently back). Re-betting an outcome adds to the existing Bet, and
/// closing a Bet removes its entry, so this caps concurrent positions, not
/// lifetime bets.
pub const MAX_BETS_PER_USER: usize = 32;

/// Byte length of the packed open-bet index: `MAX_BETS_PER_USER` 32-byte
/// addresses.
pub const USER_BETS_BYTES: usize = MAX_BETS_PER_USER * 32;

/// Per-wallet index of a bettor's open Bet accounts, so a client can list a
/// wallet's positions without scanning every Bet account. The authoritative
/// stake state lives in the Bet accounts; this is a convenience index.
///
/// The Anchor build stores the index as a borsh `Vec<Pubkey>`. This port packs
/// the addresses into a fixed `[u8; 1024]` buffer plus `bet_count`, keeping the
/// account fixed-size (no realloc on each bet, a plain in-place `set_inner`).
///
/// PDA: `["user", authority]`.
#[account(discriminator = 5, set_inner)]
#[seeds(b"user", authority: Address)]
pub struct User {
    pub authority: Address,
    pub bump: u8,
    pub bet_count: u8,
    pub bets: [u8; USER_BETS_BYTES],
}

fn read_bet(bets: &[u8; USER_BETS_BYTES], index: usize) -> Address {
    let start = index * 32;
    let mut key = [0u8; 32];
    key.copy_from_slice(&bets[start..start + 32]);
    Address::from(key)
}

fn write_bet(bets: &mut [u8; USER_BETS_BYTES], index: usize, bet_key: &Address) {
    let start = index * 32;
    bets[start..start + 32].copy_from_slice(bet_key.as_ref());
}

fn position_of(bets: &[u8; USER_BETS_BYTES], bet_count: u8, bet_key: &Address) -> Option<usize> {
    (0..bet_count as usize).find(|&index| read_bet(bets, index) == *bet_key)
}

/// Append a Bet key. Returns `TooManyBets` when the index is full. Callers only
/// add on a genuinely new position, so a duplicate is not expected; if one
/// slips in it is a silent no-op.
pub fn add_bet(
    bets: &mut [u8; USER_BETS_BYTES],
    bet_count: &mut u8,
    bet_key: &Address,
) -> Result<(), ProgramError> {
    if position_of(bets, *bet_count, bet_key).is_some() {
        return Ok(());
    }
    let count = *bet_count as usize;
    if count >= MAX_BETS_PER_USER {
        return Err(BettingError::TooManyBets.into());
    }
    write_bet(bets, count, bet_key);
    *bet_count = (count + 1) as u8;
    Ok(())
}

/// Drop a closed Bet's entry by swapping in the last entry (order within the
/// index is not meaningful). Errors if the key isn't tracked, matching the
/// Anchor build's `BetNotInUserIndex`.
pub fn remove_bet(
    bets: &mut [u8; USER_BETS_BYTES],
    bet_count: &mut u8,
    bet_key: &Address,
) -> Result<(), ProgramError> {
    let position =
        position_of(bets, *bet_count, bet_key).ok_or(BettingError::BetNotInUserIndex)?;
    let last = *bet_count as usize - 1;
    if position != last {
        let moved = read_bet(bets, last);
        write_bet(bets, position, &moved);
    }
    write_bet(bets, last, &Address::default());
    *bet_count = last as u8;
    Ok(())
}

pub fn snapshot_user(user: &Account<User>) -> UserInner {
    UserInner {
        authority: user.authority,
        bump: user.bump,
        bet_count: user.bet_count,
        bets: user.bets,
    }
}
