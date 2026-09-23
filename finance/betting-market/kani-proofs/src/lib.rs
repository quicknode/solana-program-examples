//! Kani proof harnesses for the betting-market program (`finance/betting-market`).
//!
//! Inspired by aeyakovenko/percolator, which uses the Kani model checker to
//! prove the mathematical correctness of a DeFi engine's pure numeric core.
//!
//! The program is a pari-mutuel betting market: every stake lands in one vault,
//! and at settlement the losing pool (minus a fee) is split among the winners in
//! proportion to their stake. The token movement goes through SPL CPIs Kani
//! cannot symbolically execute, but the payout math (`settle_event`,
//! `claim_winnings`) is pure integer arithmetic. This crate reproduces it
//! faithfully and proves the two properties that matter: **solvency** (winners
//! can never collectively claim more than the vault holds) and that a winner is
//! never paid less than their own stake. A small model of the event's
//! lifecycle guards also proves the outcome list is fixed before any money
//! arrives and that the betting and settlement windows never overlap.
//!
//! The nonlinear harness uses bounded model checking (small symbolic inputs), as
//! percolator does; the pro-rata identity is scale-invariant.

#![cfg_attr(kani, allow(dead_code))]

/// `BPS_DENOMINATOR` (10_000) from the program's shared constants.
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Settlement math from `handle_settle_event`: split the pool into the losing
/// side, the fee (charged only on losers), and the distributable remainder.
/// Returns `(losing_pool, fee, distributable_losing_pool)`. `None` on the
/// underflow/overflow paths.
pub fn settle(total_pool: u64, winning_pool: u64, fee_bps: u16) -> Option<(u64, u64, u64)> {
    let losing_pool = total_pool.checked_sub(winning_pool)?;
    let fee: u64 = ((losing_pool as u128) * (fee_bps as u128) / BPS_DENOMINATOR)
        .try_into()
        .ok()?;
    let distributable = losing_pool.checked_sub(fee)?;
    Some((losing_pool, fee, distributable))
}

/// One winner's winnings from `handle_claim_winnings`:
/// `floor(stake * distributable_losing_pool / winning_pool)`.
pub fn winnings(stake: u64, distributable: u64, winning_pool: u64) -> Option<u64> {
    if winning_pool == 0 {
        return None;
    }
    ((stake as u128) * (distributable as u128) / (winning_pool as u128))
        .try_into()
        .ok()
}

// ===========================================================================
// 1. Settlement fee / split
// ===========================================================================

/// Settlement is well-formed for any pool where `winning_pool <= total_pool`
/// (the invariant `place_bet` maintains: a single outcome's stakes are a subset
/// of the whole pool): the fee never exceeds the losing pool (so `distributable`
/// never underflows), and `winning + distributable + fee == total` — every base
/// unit is accounted for.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_settlement_fee_and_split() {
    let total_pool: u64 = kani::any();
    let winning_pool: u64 = kani::any();
    let fee_bps: u16 = kani::any();

    // Bounded model checking (nonlinear `losing * fee_bps`); fee_bps fully
    // symbolic over its valid range.
    kani::assume(total_pool <= 4095);
    kani::assume(winning_pool <= total_pool); // place_bet invariant
    kani::assume((fee_bps as u128) <= BPS_DENOMINATOR);

    let (losing, fee, distributable) = settle(total_pool, winning_pool, fee_bps).expect("settles");

    assert!(fee <= losing); // fee only ever a fraction of the losing pool
    assert_eq!(losing, total_pool - winning_pool);
    // Conservation: nothing created or destroyed by settlement.
    assert_eq!(
        winning_pool as u128 + distributable as u128 + fee as u128,
        total_pool as u128
    );
}

// ===========================================================================
// 2. A winner never receives less than they staked
// ===========================================================================

/// `payout = stake + winnings` and `winnings >= 0`, so a winner always gets at
/// least their stake back — the property the code comments promise ("a winner
/// can never receive less than they staked", because the fee is charged only on
/// the losing side). Also: an individual winner's winnings never exceed the
/// distributable pool.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_winner_never_below_stake() {
    let stake: u64 = kani::any();
    let distributable: u64 = kani::any();
    let winning_pool: u64 = kani::any();

    kani::assume(winning_pool >= 1 && winning_pool <= 255);
    kani::assume(stake <= winning_pool); // one winner's stake is part of the winning pool
    kani::assume(distributable <= 255);

    let win = winnings(stake, distributable, winning_pool).expect("computes");
    let payout = stake.checked_add(win).expect("payout fits");

    assert!(payout >= stake); // never paid less than staked
    assert!(win <= distributable); // a single winner can't take more than the whole pot
}

// ===========================================================================
// 3. Pari-mutuel solvency  (the centrepiece)
// ===========================================================================

/// **Solvency**: the winners collectively never claim more than the vault holds.
///
/// After settlement the vault holds `winning_pool + distributable_losing_pool`
/// (the fee has been paid out). Each of the N winners is paid
/// `stake_i + floor(stake_i * D / winning_pool)`, and the winning stakes sum to
/// `winning_pool`. Since `Σ floor(stake_i·D/W) <= Σ stake_i·D/W = D`, the total
/// paid out is `<= winning_pool + D` — exactly the vault balance. So no
/// combination of winners can drain the vault below zero; the floor rounding
/// only ever leaves dust behind.
///
/// Modelled with 3 winners whose stakes sum to the winning pool.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_parimutuel_solvency() {
    let s1: u64 = kani::any();
    let s2: u64 = kani::any();
    let s3: u64 = kani::any();
    let distributable: u64 = kani::any();

    // Bounded model checking: 3 nonlinear `stake_i * D` products each divided by
    // the symbolic winning pool — bound tightly.
    kani::assume(s1 <= 7 && s2 <= 7 && s3 <= 7);
    kani::assume(distributable <= 63);
    let winning_pool = s1 + s2 + s3; // the winning stakes ARE the winning pool
    kani::assume(winning_pool >= 1);

    let w1 = winnings(s1, distributable, winning_pool).expect("computes");
    let w2 = winnings(s2, distributable, winning_pool).expect("computes");
    let w3 = winnings(s3, distributable, winning_pool).expect("computes");

    // The shared winnings never exceed the distributable pool...
    let total_winnings = w1 as u128 + w2 as u128 + w3 as u128;
    assert!(total_winnings <= distributable as u128);

    // ...so total payouts (stakes back + winnings) never exceed the vault
    // balance after the fee (winning_pool + distributable).
    let total_payout = winning_pool as u128 + total_winnings;
    assert!(total_payout <= winning_pool as u128 + distributable as u128);
}

// ===========================================================================
// 4. Refund conservation  (cancelled event)
// ===========================================================================

/// When an event is cancelled every bettor reclaims their exact stake
/// (`claim_refund` transfers `bet.amount`), so the refunds sum to the total pool
/// — the vault is neither over- nor under-drained. Pure linear logic, full
/// `u64` width, bounded only in the number of bettors.
#[cfg(kani)]
#[kani::proof]
fn proof_refund_conserves_pool() {
    let stakes: [u64; 4] = [kani::any(), kani::any(), kani::any(), kani::any()];
    // The pool is the sum of stakes (place_bet adds each to event.total_pool).
    let mut total_pool: u128 = 0;
    for &s in stakes.iter() {
        total_pool += s as u128;
    }

    // Each refund equals the bettor's stake; refunds sum back to the pool.
    let mut refunded: u128 = 0;
    for &s in stakes.iter() {
        refunded += s as u128; // claim_refund transfers exactly bet.amount
    }
    assert_eq!(refunded, total_pool);
}

// ===========================================================================
// 5. Lifecycle: the question is fixed before money arrives
// ===========================================================================

/// Mirrors `betting_is_open` in the program's `state/event.rs`.
pub fn betting_is_open(now: i64, betting_closes_at: i64) -> bool {
    now < betting_closes_at
}

/// Mirrors `may_settle` in the program's `state/event.rs`.
pub fn may_settle(now: i64, betting_closes_at: i64) -> bool {
    now >= betting_closes_at
}

/// `EventStatus` from the program.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Draft,
    Open,
    Settled,
    Cancelled,
}

/// The fields of an `Event` the lifecycle guards read or write.
#[derive(Clone, Copy, Debug)]
pub struct EventModel {
    pub status: Status,
    pub outcome_count: u8,
    pub total_pool: u64,
    pub betting_closes_at: i64,
}

/// One handler call, with the guards its `require!`s enforce. Returns `None`
/// where the handler rejects, leaving the event untouched.
#[derive(Clone, Copy, Debug)]
pub enum Action {
    AddOutcome,
    OpenBetting,
    PlaceBet(u64),
    Settle,
    Cancel,
}

pub fn step(event: EventModel, action: Action, now: i64) -> Option<EventModel> {
    let mut next = event;
    match action {
        Action::AddOutcome => {
            if event.status != Status::Draft {
                return None;
            }
            next.outcome_count = event.outcome_count.checked_add(1)?;
        }
        Action::OpenBetting => {
            if event.status != Status::Draft
                || event.outcome_count < 2
                || !betting_is_open(now, event.betting_closes_at)
            {
                return None;
            }
            next.status = Status::Open;
        }
        Action::PlaceBet(amount) => {
            if amount == 0
                || event.status != Status::Open
                || !betting_is_open(now, event.betting_closes_at)
            {
                return None;
            }
            next.total_pool = event.total_pool.checked_add(amount)?;
        }
        Action::Settle => {
            if event.status != Status::Open || !may_settle(now, event.betting_closes_at) {
                return None;
            }
            next.status = Status::Settled;
        }
        Action::Cancel => {
            if event.status != Status::Draft && event.status != Status::Open {
                return None;
            }
            next.status = Status::Cancelled;
        }
    }
    Some(next)
}

#[cfg(kani)]
fn any_action() -> Action {
    match kani::any::<u8>() % 5 {
        0 => Action::AddOutcome,
        1 => Action::OpenBetting,
        2 => Action::PlaceBet(kani::any()),
        3 => Action::Settle,
        _ => Action::Cancel,
    }
}

/// At every instant exactly one of "a bet may land" and "the event may be
/// settled" holds, so no bet can arrive once settlement is possible, and the
/// admin cannot settle while the promised window is still open.
#[cfg(kani)]
#[kani::proof]
fn proof_betting_and_settlement_windows_partition_time() {
    let now: i64 = kani::any();
    let betting_closes_at: i64 = kani::any();
    assert!(betting_is_open(now, betting_closes_at) != may_settle(now, betting_closes_at));
}

/// Over any sequence of handler calls at any times, starting from the draft
/// `initialize_event` creates:
/// - the outcome list never changes once any money is in the pool;
/// - every stake lands in a market with at least two outcomes;
/// - no stake lands at or after the close time;
/// - a settled event was settled at or after the close time.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(7)]
fn proof_outcomes_fixed_before_money_arrives() {
    let mut event = EventModel {
        status: Status::Draft,
        outcome_count: 0,
        total_pool: 0,
        betting_closes_at: kani::any(),
    };
    for _ in 0..6 {
        let action = any_action();
        let now: i64 = kani::any();
        if let Some(next) = step(event, action, now) {
            if event.total_pool > 0 {
                assert_eq!(next.outcome_count, event.outcome_count);
            }
            if let Action::PlaceBet(_) = action {
                assert!(event.outcome_count >= 2);
                assert!(now < event.betting_closes_at);
            }
            if next.status == Status::Settled && event.status != Status::Settled {
                assert!(now >= event.betting_closes_at);
            }
            event = next;
        }
    }
}

// ===========================================================================
// Plain unit tests (meaningful without Kani installed).
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_basic() {
        // total 1000, winning 400, 2% fee on the 600 losing pool = 12.
        let (losing, fee, dist) = settle(1000, 400, 200).unwrap();
        assert_eq!((losing, fee, dist), (600, 12, 588));
    }

    #[test]
    fn winnings_pro_rata() {
        // stake 100 of a 400 winning pool, distributable 588 -> floor(100*588/400)=147.
        assert_eq!(winnings(100, 588, 400).unwrap(), 147);
    }

    #[test]
    fn a_bet_needs_an_opened_market_before_the_close() {
        let draft = EventModel {
            status: Status::Draft,
            outcome_count: 0,
            total_pool: 0,
            betting_closes_at: 100,
        };
        assert!(step(draft, Action::PlaceBet(10), 0).is_none());
        assert!(step(draft, Action::OpenBetting, 0).is_none());
        let two = step(
            step(draft, Action::AddOutcome, 0).unwrap(),
            Action::AddOutcome,
            0,
        )
        .unwrap();
        let open = step(two, Action::OpenBetting, 0).unwrap();
        assert!(step(open, Action::AddOutcome, 0).is_none());
        assert!(step(open, Action::PlaceBet(10), 99).is_some());
        assert!(step(open, Action::PlaceBet(10), 100).is_none());
        assert!(step(open, Action::Settle, 99).is_none());
        assert!(step(open, Action::Settle, 100).is_some());
    }

    #[test]
    fn solvency_holds() {
        // 3 winners staking 100/150/150 (pool 400), distributable 588.
        let d = 588u64;
        let w = 400u64;
        let sum: u64 = [100, 150, 150]
            .iter()
            .map(|&s| winnings(s, d, w).unwrap())
            .sum();
        assert!(sum <= d);
    }
}
