//! quasar-test integration tests. They exercise the full lifecycle: pool
//! initialization, liquidity add/remove, opening/closing/liquidating leveraged
//! positions, fee collection, the price average and its band, the
//! oracle/margin checks, and the haircut, profit warm-up and insurance fund.

use {
    crate::{
        constants::{MAX_FUNDING_RATE_PER_SECOND, SIDE_LONG, SIDE_SHORT},
        cpi::{
            AddLiquidityInstruction, ClosePositionInstruction, CollectFeesInstruction,
            InitializePoolInstruction, LiquidatePositionInstruction, OpenPositionInstruction,
            RemoveLiquidityInstruction, UpdatePriceAverageInstruction,
        },
        instructions::shared::{basis_points_of, basis_points_of_rounded_down, error},
        state::{Pool, Position},
        LpMintPda, VaultPda,
    },
    quasar_test::prelude::*,
};

const ONE_USDC: u64 = 1_000_000;
const ORACLE_SCALE: u32 = 8;
// quasar-test worlds run at the default slot (0); the feed is stamped with the
// same slot so the staleness check passes.
const SLOT: u64 = 0;
/// How fast the funding tests move the slot while the clock moves: five a
/// second, the network's 200 ms target, so prices age as they would.
const SLOTS_PER_SECOND: u64 = 5;

// Deterministic addresses.
const ADMIN: Pubkey = Pubkey::new_from_array([1; 32]);
const COLLATERAL_MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const FEED: Pubkey = Pubkey::new_from_array([3; 32]);
const PROVIDER: Pubkey = Pubkey::new_from_array([4; 32]);
const PROVIDER_COLLATERAL: Pubkey = Pubkey::new_from_array([5; 32]);
const PROVIDER_LP: Pubkey = Pubkey::new_from_array([6; 32]);
const TRADER: Pubkey = Pubkey::new_from_array([7; 32]);
const TRADER_COLLATERAL: Pubkey = Pubkey::new_from_array([8; 32]);
const LIQUIDATOR: Pubkey = Pubkey::new_from_array([9; 32]);
const LIQUIDATOR_COLLATERAL: Pubkey = Pubkey::new_from_array([10; 32]);
const ADMIN_COLLATERAL: Pubkey = Pubkey::new_from_array([11; 32]);
const VICTIM: Pubkey = Pubkey::new_from_array([12; 32]);
const VICTIM_COLLATERAL: Pubkey = Pubkey::new_from_array([13; 32]);
const VICTIM_LP: Pubkey = Pubkey::new_from_array([14; 32]);
const OPERATOR_WALLET: Pubkey = Pubkey::new_from_array([15; 32]);
const OPERATOR_COLLATERAL: Pubkey = Pubkey::new_from_array([16; 32]);
const KEEPER: Pubkey = Pubkey::new_from_array([17; 32]);
const SECOND_TRADER: Pubkey = Pubkey::new_from_array([18; 32]);
const SECOND_TRADER_COLLATERAL: Pubkey = Pubkey::new_from_array([19; 32]);
// A program that is not the one the pool recorded as its feed's owner.
const OTHER_PROGRAM: Pubkey = Pubkey::new_from_array([20; 32]);

// Matches `PRICE_AVERAGE_WINDOW_SECONDS`: one fold after this many seconds
// replaces the pool's average price with the oracle price.
const PRICE_AVERAGE_WINDOW_SECONDS: i64 = 600;

// Ten years, in seconds.
const TEN_YEARS: i64 = 315_360_000;

// The test pool's profit warm-up: a position can be closed at a profit from
// this many slots after it opened.
const PROFIT_WARMUP_SLOTS: u64 = 10;

// Matches `HAIRCUT_PRECISION`: a haircut ratio of one.
const HAIRCUT_PRECISION: u64 = 1_000_000_000;
// Matches `FUNDING_PRECISION`: the fixed point the funding rate and index are
// carried in, so a position's funding is `size * rate * seconds / this`.
const FUNDING_PRECISION: u64 = 1_000_000_000;

fn dollars(whole: i128) -> i128 {
    whole * 10i128.pow(ORACLE_SCALE)
}

/// A feed account in this program's layout: price (i128), scale (u32),
/// last_update_slot (u64), confidence (u64), owned by the system program,
/// which the pool therefore records as the feed's owning program. The tests
/// own this; production reads a real feed.
fn set_feed(test: &mut Test, price: i128, confidence: u64) {
    set_feed_at_slot(test, price, SLOT, confidence);
}

fn set_feed_at_slot(test: &mut Test, price: i128, slot: u64, confidence: u64) {
    set_feed_owned_by(test, system_program::ID, price, slot, confidence);
}

/// Write the feed account at `FEED` with `owner` as its owning program. The
/// bytes are the same whoever owns it, so a copy owned by another program
/// still decodes as a fresh, confident price at the pinned scale.
fn set_feed_owned_by(test: &mut Test, owner: Pubkey, price: i128, slot: u64, confidence: u64) {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&ORACLE_SCALE.to_le_bytes());
    data.extend_from_slice(&slot.to_le_bytes());
    data.extend_from_slice(&confidence.to_le_bytes());
    test.set_account(Account::new(FEED, owner, 1_000_000, data));
}

/// Pin the Clock sysvar account at `slot` and `unix_timestamp`. Price freshness
/// is counted in the slot; funding is counted in the timestamp. Clock's bincode
/// layout is the raw little-endian fields: slot, epoch_start_timestamp, epoch,
/// leader_schedule_epoch, unix_timestamp.
fn set_clock_at(test: &mut Test, slot: u64, unix_timestamp: i64) {
    let mut data = Vec::with_capacity(40);
    data.extend_from_slice(&slot.to_le_bytes());
    data.extend_from_slice(&0i64.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&unix_timestamp.to_le_bytes());
    let sysvar_owner: Pubkey = "Sysvar1111111111111111111111111111111111111"
        .parse()
        .unwrap();
    test.set_account(Account::new(clock_id(), sysvar_owner, 1_169_280, data));
}

fn clock_id() -> Pubkey {
    "SysvarC1ock11111111111111111111111111111111"
        .parse()
        .unwrap()
}

/// The Clock's `(slot, unix_timestamp)`, as `set_clock_at` last pinned them;
/// the world's default of slot 0 and timestamp 0 before that.
fn clock(test: &Test) -> (u64, i64) {
    match test.account(clock_id()) {
        Some(account) if account.data.len() >= 40 => (
            u64::from_le_bytes(account.data[0..8].try_into().unwrap()),
            i64::from_le_bytes(account.data[32..40].try_into().unwrap()),
        ),
        _ => (0, 0),
    }
}

/// Move the slot forward by the profit warm-up, leaving the timestamp where it
/// is, so a position opened in the current slot can be closed at a profit.
/// The feed stays fresh: it is far fewer slots than the staleness bound.
fn pass_warmup(test: &mut Test) {
    let (slot, unix_timestamp) = clock(test);
    set_clock_at(test, slot + PROFIT_WARMUP_SLOTS, unix_timestamp);
}

/// Pin the LastRestartSlot sysvar account, simulating a cluster restart at
/// `slot`: prices stamped at or before it must be rejected until the
/// publisher posts again. The sysvar's whole data is one little-endian u64.
fn set_last_restart_slot(test: &mut Test, slot: u64) {
    let sysvar_id: Pubkey = "SysvarLastRestartS1ot1111111111111111111111"
        .parse()
        .unwrap();
    let sysvar_owner: Pubkey = "Sysvar1111111111111111111111111111111111111"
        .parse()
        .unwrap();
    test.set_account(Account::new(
        sysvar_id,
        sysvar_owner,
        1_169_280,
        slot.to_le_bytes().to_vec(),
    ));
}

fn init_pool(test: &mut Test, maintenance_margin_bps: u16, close_fee_bps: u16) -> Outcome {
    init_pool_with_funding(test, maintenance_margin_bps, close_fee_bps, 0)
}

fn init_pool_with_funding(
    test: &mut Test,
    maintenance_margin_bps: u16,
    close_fee_bps: u16,
    funding_rate_per_second: u64,
) -> Outcome {
    test.send(InitializePoolInstruction {
        maintenance_margin_bps,
        close_fee_bps,
        funding_rate_per_second,
        ..default_initialize_pool()
    })
}

/// The pool every test uses unless it overrides a parameter: 0.1% open and
/// close fees, half of each fee paid into the insurance fund, a 10% initial
/// margin (10x leverage), a 5% maintenance margin, a 1% liquidation fee, a 1%
/// maximum confidence band, a 20% price band around the pool's average price,
/// a 10-slot profit warm-up, and no funding.
fn default_initialize_pool() -> InitializePoolInstruction {
    InitializePoolInstruction {
        authority: ADMIN,
        collateral_mint: COLLATERAL_MINT,
        oracle_feed: FEED,
        oracle_scale: ORACLE_SCALE,
        funding_rate_per_second: 0,
        open_fee_bps: 10,
        close_fee_bps: 10,
        initial_margin_bps: 1_000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        max_confidence_bps: 100,
        max_price_deviation_bps: 2_000,
        insurance_fee_bps: 5_000,
        profit_warmup_slots: PROFIT_WARMUP_SLOTS,
    }
}

/// The world `initialize_pool` needs: the admin, the collateral mint, and a
/// feed at $100.
fn add_pool_prerequisites(test: &mut Test) {
    test.add(Wallet::new().at(ADMIN));
    test.add(Mint::new(ADMIN).at(COLLATERAL_MINT).decimals(6));
    set_feed(test, dollars(100), 0);
}

/// The pool and its derived PDAs.
struct Env {
    pool: Pubkey,
    lp_mint: Pubkey,
    custody_vault: Pubkey,
}

/// Build a world with a collateral mint, an oracle feed at $100, and an
/// initialized pool with the parameters in `default_initialize_pool`.
fn setup(test: &mut Test) -> Env {
    setup_with_funding(test, 0)
}

/// Like `setup`, but with a non-zero per-second funding rate so funding accrues
/// as time passes.
fn setup_with_funding(test: &mut Test, funding_rate_per_second: u64) -> Env {
    setup_with(
        test,
        InitializePoolInstruction {
            funding_rate_per_second,
            ..default_initialize_pool()
        },
    )
}

/// Like `setup`, but initializing the pool with `instruction`.
fn setup_with(test: &mut Test, instruction: InitializePoolInstruction) -> Env {
    add_pool_prerequisites(test);
    test.send(instruction).succeeds();

    let pool = test.derive_pda(Pool::seeds(&COLLATERAL_MINT, &FEED));
    Env {
        pool,
        lp_mint: test.derive_pda(LpMintPda::seeds(&pool)),
        custody_vault: test.derive_pda(VaultPda::seeds(&pool)),
    }
}

/// Fund a wallet with a collateral token account.
fn fund(test: &mut Test, wallet: Pubkey, collateral_account: Pubkey, collateral: u64) {
    test.add(Wallet::new().at(wallet));
    test.add(
        TokenAccount::new(COLLATERAL_MINT, wallet)
            .at(collateral_account)
            .amount(collateral),
    );
}

fn add_liquidity(test: &mut Test, env: &Env, amount: u64) -> Outcome {
    test.send(AddLiquidityInstruction {
        provider: PROVIDER,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        provider_collateral: PROVIDER_COLLATERAL,
        provider_lp: PROVIDER_LP,
        amount,
        minimum_shares_out: 0,
    })
}

fn remove_liquidity(test: &mut Test, env: &Env, shares: u64) -> Outcome {
    test.send(RemoveLiquidityInstruction {
        provider: PROVIDER,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        provider_collateral: PROVIDER_COLLATERAL,
        provider_lp: PROVIDER_LP,
        shares,
        minimum_amount_out: 0,
    })
}

fn open_position(test: &mut Test, env: &Env, side: u8, collateral: u64, size: u64) -> Outcome {
    open_position_for(test, env, TRADER, TRADER_COLLATERAL, side, collateral, size)
}

fn open_position_for(
    test: &mut Test,
    env: &Env,
    owner: Pubkey,
    owner_collateral: Pubkey,
    side: u8,
    collateral: u64,
    size: u64,
) -> Outcome {
    test.send(OpenPositionInstruction {
        owner,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: owner_collateral,
        side,
        collateral_amount: collateral,
        size,
        acceptable_price: 0,
    })
}

/// `LIQUIDATOR` liquidates `TRADER`'s position.
fn liquidate(test: &mut Test, env: &Env) -> Outcome {
    if test.account(LIQUIDATOR).is_none() {
        test.add(Wallet::new().at(LIQUIDATOR));
    }
    test.send(LiquidatePositionInstruction {
        liquidator: LIQUIDATOR,
        owner: TRADER,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: TRADER_COLLATERAL,
        liquidator_collateral: LIQUIDATOR_COLLATERAL,
    })
}

/// Assert the custody vault holds exactly what the pool's ledger says it
/// does: liquidity, open positions' collateral, program fees and the
/// insurance fund.
fn assert_vault_matches_ledger(test: &Test, env: &Env) {
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(
        test.tokens(env.custody_vault),
        u64::from(pool.liquidity)
            + u64::from(pool.total_collateral)
            + u64::from(pool.program_fees)
            + u64::from(pool.insurance_fund)
    );
}

/// The pool's `(average_price, last_oracle_price, average_price_timestamp)`.
fn pool_state(test: &Test, env: &Env) -> (u64, u64, i64) {
    let pool = test.read::<Pool>(env.pool);
    (
        u64::from(pool.average_price),
        u64::from(pool.last_oracle_price),
        i64::from(pool.average_price_timestamp),
    )
}

/// Move the clock `seconds` past the pool's last average fold, publish `price`
/// at the new slot, and call `update_price_average`, which credits those
/// seconds to the price seen at the previous read and records `price`.
fn update_average_after(test: &mut Test, env: &Env, seconds: i64, price: i128) -> Outcome {
    let (_, _, last_fold) = pool_state(test, env);
    let timestamp = last_fold + seconds;
    let slot = timestamp as u64 * SLOTS_PER_SECOND;
    set_clock_at(test, slot, timestamp);
    set_feed_at_slot(test, price, slot, 0);
    if test.account(KEEPER).is_none() {
        test.add(Wallet::new().at(KEEPER));
    }
    test.send(UpdatePriceAverageInstruction {
        caller: KEEPER,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
    })
}

fn close_position(test: &mut Test, env: &Env) -> Outcome {
    close_position_for(test, env, TRADER, TRADER_COLLATERAL)
}

fn close_position_for(
    test: &mut Test,
    env: &Env,
    owner: Pubkey,
    owner_collateral: Pubkey,
) -> Outcome {
    test.send(ClosePositionInstruction {
        owner,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: owner_collateral,
        minimum_payout: 0,
    })
}

#[quasar_test]
fn initialize_pool_creates_pool_vault_and_lp_mint(test: &mut Test) {
    let env = setup(test);
    // The pool, vault, and liquidity-provider mint were created, and the pool
    // recorded the program that owned the feed.
    assert!(test.account(env.pool).is_some());
    assert_eq!(
        test.read::<Pool>(env.pool).price_feed_program,
        system_program::ID
    );
    let custody_vault = test.account(env.custody_vault).unwrap();
    let lp_mint = test.account(env.lp_mint).unwrap();

    // The pool account itself owns the custody vault and is the LP mint's
    // authority; there is no separate signing PDA. A token account keeps its
    // owner at bytes 32..64, and a mint keeps its authority at bytes 4..36
    // behind a four-byte `COption` tag.
    let vault_owner = Pubkey::new_from_array(custody_vault.data[32..64].try_into().unwrap());
    assert_eq!(vault_owner, env.pool);
    let mint_authority = Pubkey::new_from_array(lp_mint.data[4..36].try_into().unwrap());
    assert_eq!(mint_authority, env.pool);
}

#[quasar_test]
fn add_liquidity_deposits_and_mints_shares(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 10_000 * ONE_USDC);

    add_liquidity(test, &env, 10_000 * ONE_USDC)
        .succeeds()
        // The vault holds the deposit and the provider received shares
        // (minus the withheld minimum liquidity).
        .has_tokens(env.custody_vault, 10_000 * ONE_USDC)
        .has_tokens(PROVIDER_LP, 10_000 * ONE_USDC - 1_000);
}

/// The first deposit must exceed the 1_000 withheld minimum: one base unit
/// short is refused, and one over mints a single share.
#[quasar_test]
fn first_deposit_below_minimum_fails(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 10_000);
    add_liquidity(test, &env, 999).fails_with(error::DEPOSIT_TOO_SMALL);
    add_liquidity(test, &env, 1_001)
        .succeeds()
        .has_tokens(PROVIDER_LP, 1);
}

#[quasar_test]
fn remove_liquidity_round_trip_returns_the_deposit_less_the_minimum(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 10_000 * ONE_USDC);
    add_liquidity(test, &env, 10_000 * ONE_USDC).succeeds();

    let shares = test.tokens(PROVIDER_LP);
    remove_liquidity(test, &env, shares)
        .succeeds()
        // Even the sole provider leaves the withheld minimum behind: those
        // 1_000 shares belong to nobody, and their 1_000 stays in the vault.
        .has_tokens(PROVIDER_COLLATERAL, 10_000 * ONE_USDC - 1_000)
        .has_tokens(env.custody_vault, 1_000);

    // The next deposit is priced against the minimum's slice rather than
    // bootstrapped: 5_000 * (0 + 1_000) / 1_000 = 5_000 shares.
    add_liquidity(test, &env, 5_000)
        .succeeds()
        .has_tokens(PROVIDER_LP, 5_000);
}

/// First-depositor share inflation without a donation. Tokens sent straight to
/// the vault move nothing, because shares are priced against `liquidity`, but
/// `liquidity` grows with every funding payment and trader loss, and a
/// provider can also be the pool's only trader. The attacker opens the pool
/// with 1 share, pays funding on a small long of their own until `liquidity`
/// is large, and waits for a deposit. The withheld minimum counts as shares in
/// both directions, so the attacker's share is 1 of 1_001 and what they paid
/// in is spread across shares nobody can redeem.
#[quasar_test]
fn inflating_liquidity_through_own_trades_does_not_pay(test: &mut Test) {
    // The steepest rate a pool may have, held for ten years.
    let env = setup_with_funding(test, MAX_FUNDING_RATE_PER_SECOND);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 1_001);
    add_liquidity(test, &env, 1_001)
        .succeeds()
        .has_tokens(PROVIDER_LP, 1);

    // The attacker's trading key: a 1_000 long, heavily collateralized.
    fund(test, TRADER, TRADER_COLLATERAL, 2_000 * ONE_USDC);
    open_position(test, &env, 0, 2_000 * ONE_USDC, 1_000).succeeds();
    let ten_years_slots = TEN_YEARS as u64 * SLOTS_PER_SECOND;
    set_clock_at(test, ten_years_slots, TEN_YEARS);
    set_feed_at_slot(test, dollars(100), ten_years_slots, 0);
    close_position(test, &env).succeeds();
    // Spent: the 1_001 deposit, the funding, and a 1-unit fee each way. All
    // but the two fees is now `liquidity`.
    let attacker_spent = 1_001 + 2_000 * ONE_USDC - test.tokens(TRADER_COLLATERAL);
    let pumped_liquidity = attacker_spent - 2;
    assert!(pumped_liquidity > 50 * 1_001);

    // Just under twice the pumped liquidity: dividing by the bare supply of 1
    // would mint a single share, and the attacker's share would redeem half.
    let victim_deposit = 2 * pumped_liquidity - 1;
    fund(test, VICTIM, VICTIM_COLLATERAL, victim_deposit);
    test.send(AddLiquidityInstruction {
        provider: VICTIM,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        provider_collateral: VICTIM_COLLATERAL,
        provider_lp: VICTIM_LP,
        amount: victim_deposit,
        minimum_shares_out: 0,
    })
    .succeeds();
    let victim_shares = test.tokens(VICTIM_LP);

    remove_liquidity(test, &env, 1).succeeds();
    let attacker_back = test.tokens(PROVIDER_COLLATERAL);
    assert!(
        attacker_back * 100 < attacker_spent,
        "attacker spent {attacker_spent} and got back {attacker_back}"
    );

    test.send(RemoveLiquidityInstruction {
        provider: VICTIM,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        provider_collateral: VICTIM_COLLATERAL,
        provider_lp: VICTIM_LP,
        shares: victim_shares,
        minimum_amount_out: 0,
    })
    .succeeds();
    let victim_back = test.tokens(VICTIM_COLLATERAL);
    assert!(
        victim_back * 1_000 >= victim_deposit * 999,
        "victim deposited {victim_deposit} and got back {victim_back}"
    );
}

#[quasar_test]
fn open_long_position_creates_the_position(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC).succeeds();

    let position = test.derive_pda(Position::seeds(&env.pool, &TRADER));
    assert!(test.account(position).is_some());
}

/// A cluster restart passes hours of wall-clock time in zero slots, so a
/// price published before the halt can still look fresh by slot count. With
/// leverage a stale price is amplified into a market-wide equity error, so
/// the pool must refuse it until the publisher posts again.
#[quasar_test]
fn open_rejects_price_from_before_a_restart(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);

    // The feed sits at slot 5, fresh by the 150-slot staleness bound, but the
    // cluster restarted at slot 7: only the restart check can catch the
    // pre-halt price.
    set_clock_at(test, 10, 0);
    set_feed_at_slot(test, dollars(100), 5, 0);
    set_last_restart_slot(test, 7);
    open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC)
        .fails_with(error::PRICE_PREDATES_RESTART);

    // Publishing after the restart (slot 10) reopens the pool.
    set_feed_at_slot(test, dollars(100), 10, 0);
    open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC).succeeds();
}

/// The pool records the program that owns its feed at creation and refuses a
/// price from a feed account owned by any other program, however well its
/// bytes decode. The feed is swapped for a byte-identical copy owned by an
/// unrelated program, and the refusal is by the owner alone: the same bytes
/// owned by the recorded program again are accepted.
#[quasar_test]
fn open_rejects_price_feed_from_another_program(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);

    set_feed_owned_by(test, OTHER_PROGRAM, dollars(100), SLOT, 0);
    open_position(test, &env, SIDE_LONG, 1_000 * ONE_USDC, 5_000 * ONE_USDC)
        .fails_with(error::PRICE_FEED_NOT_FROM_ORACLE);

    set_feed(test, dollars(100), 0);
    open_position(test, &env, SIDE_LONG, 1_000 * ONE_USDC, 5_000 * ONE_USDC).succeeds();
}

#[quasar_test]
fn close_long_in_profit_pays_collateral_plus_pnl_minus_fees(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    let size = 5_000 * ONE_USDC;
    open_position(test, &env, 0, 1_000 * ONE_USDC, size).succeeds();

    // Price rises 20%: a $5,000 long earns $1,000, paid once the warm-up has
    // passed.
    pass_warmup(test);
    set_feed(test, dollars(120), 0);

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = 1_000 * ONE_USDC - open_fee;
    let profit = size / 5;
    let expected = net_collateral + profit - close_fee;
    close_position(test, &env)
        .succeeds()
        .has_tokens(TRADER_COLLATERAL, expected);
}

#[quasar_test]
fn open_rejects_position_below_initial_margin(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 2_000 * ONE_USDC);

    // The initial margin is 10% of notional. 1,000 USDC of collateral less
    // the 11 USDC open fee leaves 989 USDC, short of the 1,100 USDC an 11,000
    // USDC position needs.
    open_position(test, &env, SIDE_LONG, 1_000 * ONE_USDC, 11_000 * ONE_USDC)
        .fails_with(error::INITIAL_MARGIN_NOT_MET);

    // A 10,000 USDC position needs 1,000 USDC net of its 10 USDC open fee.
    // One minor unit short of 1,010 USDC is refused, and exactly 1,010 USDC
    // opens at 10x.
    let size = 10_000 * ONE_USDC;
    let exact_collateral = 1_010 * ONE_USDC;
    open_position(test, &env, SIDE_LONG, exact_collateral - 1, size)
        .fails_with(error::INITIAL_MARGIN_NOT_MET);
    open_position(test, &env, SIDE_LONG, exact_collateral, size).succeeds();
    assert_eq!(
        u64::from(test.read::<Pool>(env.pool).total_collateral),
        size / 10
    );
}

#[quasar_test]
fn liquidate_underwater_long_pays_the_liquidator_and_closes(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    fund(test, TRADER, TRADER_COLLATERAL, 1_100 * ONE_USDC);
    let size = 10_000 * ONE_USDC;
    open_position(test, &env, 0, 1_100 * ONE_USDC, size).succeeds();

    // Price falls 9%: a $10,000 long loses $900, dropping below maintenance.
    set_feed(test, dollars(91), 0);
    test.add(Wallet::new().at(LIQUIDATOR));

    let position = test.derive_pda(Position::seeds(&env.pool, &TRADER));
    test.send(LiquidatePositionInstruction {
        liquidator: LIQUIDATOR,
        owner: TRADER,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: TRADER_COLLATERAL,
        liquidator_collateral: LIQUIDATOR_COLLATERAL,
    })
    .succeeds()
    .is_closed(position);

    assert!(
        test.tokens(LIQUIDATOR_COLLATERAL) > 0,
        "liquidator should earn the liquidation fee"
    );
}

#[quasar_test]
fn collect_fees_sweeps_the_open_fee_to_the_admin(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    let size = 5_000 * ONE_USDC;
    open_position(test, &env, 0, 1_000 * ONE_USDC, size).succeeds();

    test.send(CollectFeesInstruction {
        authority: ADMIN,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        authority_collateral: ADMIN_COLLATERAL,
    })
    .succeeds()
    // The program's half of the open fee (0.1% of notional) was swept to the
    // admin; the other half is in the insurance fund.
    .has_tokens(ADMIN_COLLATERAL, size / 1_000 / 2);
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.program_fees), 0);
    assert_eq!(u64::from(pool.insurance_fund), size / 1_000 / 2);
}

/// Funding is quoted per second of wall-clock time, so slots passing without
/// the clock moving charge nothing. A million extra slots halfway through the
/// window, as a much shorter slot would produce, leave the funding unchanged.
///
/// Both halves hold the same position for the same seconds at the same price,
/// so they must pay the same funding; only the second sees the extra slots.
#[quasar_test]
fn funding_follows_seconds_not_slots(test: &mut Test) {
    let rate = MAX_FUNDING_RATE_PER_SECOND;
    let window = 2_000;
    let size = 5_000 * ONE_USDC;
    let collateral = 1_000 * ONE_USDC;
    let fees = 2 * (size / 1_000); // open and close, 0.1% of notional each
    let window_slots = window as u64 * SLOTS_PER_SECOND;
    let extra_slots = 1_000_000;

    let env = setup_with_funding(test, rate);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 10_000 * ONE_USDC);

    // Two windows at the network's slot pace.
    let before_flat = test.tokens(TRADER_COLLATERAL);
    open_position(test, &env, 0, collateral, size).succeeds();
    set_clock_at(test, 2 * window_slots, 2 * window);
    set_feed_at_slot(test, dollars(100), 2 * window_slots, 0);
    close_position(test, &env).succeeds();
    let flat = (before_flat - test.tokens(TRADER_COLLATERAL)) - fees;

    // Two more windows, with a million extra slots passing at the midpoint
    // while the clock stands still.
    let before_extra = test.tokens(TRADER_COLLATERAL);
    open_position(test, &env, 0, collateral, size).succeeds();
    set_clock_at(test, 3 * window_slots + extra_slots, 3 * window);
    set_clock_at(test, 4 * window_slots + extra_slots, 4 * window);
    set_feed_at_slot(test, dollars(100), 4 * window_slots + extra_slots, 0);
    close_position(test, &env).succeeds();
    let with_extra_slots = (before_extra - test.tokens(TRADER_COLLATERAL)) - fees;

    // The flat run paid exactly the funding for the seconds it was open.
    assert_eq!(
        flat,
        size * MAX_FUNDING_RATE_PER_SECOND * (2 * window) as u64 / FUNDING_PRECISION
    );
    assert_eq!(with_extra_slots, flat);
}

#[quasar_test]
fn initialize_pool_rejects_funding_rate_above_the_maximum(test: &mut Test) {
    // The rate is fixed at creation, so this is the only place it is checked.
    add_pool_prerequisites(test);
    init_pool_with_funding(test, 500, 10, MAX_FUNDING_RATE_PER_SECOND + 1)
        .fails_with(error::INVALID_PARAMETER);
    init_pool_with_funding(test, 500, 10, MAX_FUNDING_RATE_PER_SECOND).succeeds();
}

/// The pool operator trading against their own pool. The lighter side of open
/// interest is paid funding out of `liquidity`, so an operator who could raise
/// the rate at will could open a small position on the lighter side, raise the
/// rate, and close it to take the liquidity providers' deposits. The rate is
/// fixed when the pool is created and capped, so a wallet the operator
/// controls earns exactly what any trader on that side would: at most the
/// maximum rate, here just under 0.1% of the position's size over an hour.
#[quasar_test]
fn operator_on_the_lighter_side_earns_only_the_fixed_rate(test: &mut Test) {
    let env = setup_with_funding(test, MAX_FUNDING_RATE_PER_SECOND);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    // Longs are the heavier side, so they pay and shorts are paid.
    fund(test, TRADER, TRADER_COLLATERAL, 2_000 * ONE_USDC);
    open_position(test, &env, SIDE_LONG, 2_000 * ONE_USDC, 10_000 * ONE_USDC).succeeds();

    let collateral = 200 * ONE_USDC;
    let size = 1_000 * ONE_USDC;
    fund(test, OPERATOR_WALLET, OPERATOR_COLLATERAL, collateral);
    test.send(OpenPositionInstruction {
        owner: OPERATOR_WALLET,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: OPERATOR_COLLATERAL,
        side: SIDE_SHORT,
        collateral_amount: collateral,
        size,
        acceptable_price: 0,
    })
    .succeeds();

    let one_hour = 3_600;
    let one_hour_slots = one_hour as u64 * SLOTS_PER_SECOND;
    set_clock_at(test, one_hour_slots, one_hour);
    set_feed_at_slot(test, dollars(100), one_hour_slots, 0);
    test.send(ClosePositionInstruction {
        owner: OPERATOR_WALLET,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: OPERATOR_COLLATERAL,
        minimum_payout: 0,
    })
    .succeeds();

    let fees = 2 * (size / 1_000); // open and close, 0.1% of notional each
    let funding_received = test.tokens(OPERATOR_COLLATERAL) - (collateral - fees);
    let expected = size * MAX_FUNDING_RATE_PER_SECOND * one_hour as u64 / FUNDING_PRECISION;
    assert_eq!(funding_received, expected);
    assert!(
        funding_received * 1_000 < size,
        "under 0.1% of size in an hour"
    );
}

#[quasar_test]
fn wide_oracle_confidence_is_rejected(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);

    // The pool tolerates a 1% confidence band (max_confidence_bps = 100). Widen
    // the feed's band to 2% of the price and the open must be rejected.
    set_feed(test, dollars(100), dollars(2) as u64);
    open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC)
        .fails_with(error::ORACLE_CONFIDENCE_TOO_WIDE);
}

/// Nothing is set aside to back a position's profit, so a position can open
/// against a pool that could not pay its full winnings: here a $10,000 long
/// against $6,000 of liquidity.
#[quasar_test]
fn open_allowed_without_full_backing(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 6_000 * ONE_USDC);
    add_liquidity(test, &env, 6_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_100 * ONE_USDC);
    let size = 10_000 * ONE_USDC;
    open_position(test, &env, SIDE_LONG, 1_100 * ONE_USDC, size).succeeds();

    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u128::from(pool.long_size), size as u128);
    assert_eq!(u64::from(pool.liquidity), 6_000 * ONE_USDC);
    assert_vault_matches_ledger(test, &env);
}

#[quasar_test]
fn profit_runs_uncapped_when_backed(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    let collateral = 2_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    fund(test, TRADER, TRADER_COLLATERAL, collateral);
    open_position(test, &env, SIDE_LONG, collateral, size).succeeds();

    // Price triples, so the long's profit is twice its size. A move this
    // large is far outside the price band, so the average has to catch up
    // before the position can close: one update records $300, and a second a
    // full window later credits that window to $300, replacing the average.
    // That also passes the warm-up. The $100,000 pool backs the whole $10,000
    // profit, so it is paid in full.
    set_feed(test, dollars(300), 0);
    update_average_after(test, &env, 0, dollars(300)).succeeds();
    update_average_after(test, &env, PRICE_AVERAGE_WINDOW_SECONDS, dollars(300)).succeeds();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let profit = 2 * size;
    close_position(test, &env)
        .succeeds()
        .has_tokens(TRADER_COLLATERAL, net_collateral + profit - close_fee);
    assert_eq!(
        u64::from(test.read::<Pool>(env.pool).liquidity),
        100_000 * ONE_USDC - profit
    );
    assert_vault_matches_ledger(test, &env);
}

/// Two longs are owed $1,800 of profit between them, and the pool holds only
/// $900 to pay it with, so each is paid half of their profit: the first to
/// close is paid half of theirs, and the second, closing against what is left,
/// is paid half of theirs too.
#[quasar_test]
fn haircut_scales_profit_when_pool_stressed(test: &mut Test) {
    // No fee goes to the insurance fund here, so the only backing is the $900
    // of liquidity and the first close adds nothing to it.
    let env = setup_with(
        test,
        InitializePoolInstruction {
            insurance_fee_bps: 0,
            ..default_initialize_pool()
        },
    );
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 900 * ONE_USDC);
    add_liquidity(test, &env, 900 * ONE_USDC).succeeds();

    let first_collateral = 1_000 * ONE_USDC;
    let first_size = 6_000 * ONE_USDC;
    fund(test, TRADER, TRADER_COLLATERAL, first_collateral);
    open_position(test, &env, SIDE_LONG, first_collateral, first_size).succeeds();
    let second_collateral = 800 * ONE_USDC;
    let second_size = 4_000 * ONE_USDC;
    fund(
        test,
        SECOND_TRADER,
        SECOND_TRADER_COLLATERAL,
        second_collateral,
    );
    open_position_for(
        test,
        &env,
        SECOND_TRADER,
        SECOND_TRADER_COLLATERAL,
        SIDE_LONG,
        second_collateral,
        second_size,
    )
    .succeeds();

    // At $118 the first long is up $1,080 and the second $720: $1,800 owed
    // against $900 of backing, so h = 900 / 1,800 = 0.5.
    pass_warmup(test);
    set_feed(test, dollars(118), 0);
    let half = HAIRCUT_PRECISION / 2;
    let first_paid = (first_size * 18 / 100) * half / HAIRCUT_PRECISION;
    assert_eq!(first_paid, 540 * ONE_USDC);
    close_position(test, &env).succeeds().has_tokens(
        TRADER_COLLATERAL,
        first_collateral - first_size / 1_000 + first_paid - first_size / 1_000,
    );
    // The $540 withheld from the first long stays with the providers.
    assert_eq!(
        u64::from(test.read::<Pool>(env.pool).liquidity),
        360 * ONE_USDC
    );

    // The second long is now owed $720 against $360: h is still 0.5.
    let second_paid = (second_size * 18 / 100) * half / HAIRCUT_PRECISION;
    assert_eq!(second_paid, 360 * ONE_USDC);
    close_position_for(test, &env, SECOND_TRADER, SECOND_TRADER_COLLATERAL)
        .succeeds()
        .has_tokens(
            SECOND_TRADER_COLLATERAL,
            second_collateral - second_size / 1_000 + second_paid - second_size / 1_000,
        );
    assert_eq!(u64::from(test.read::<Pool>(env.pool).liquidity), 0);
    assert_vault_matches_ledger(test, &env);
}

/// Alice's long is up $1,000 while Bob's short, still open and healthy, is
/// down $900, so traders are owed only $100 in aggregate, and the pool's
/// backing is $300. Sized against the $100 alone the haircut would be one and
/// Alice's $1,000 would exceed the backing; it is sized against her $1,000
/// instead, so she is paid exactly the $300 and the close goes through. Bob's
/// later close settles his loss into the pool in full.
#[quasar_test]
fn winner_offset_by_open_loser_is_paid_not_refused(test: &mut Test) {
    let env = setup(test);
    // $290.50 of liquidity plus the $9.50 the two open fees put in the
    // insurance fund is $300 of backing.
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 290_500_000);
    add_liquidity(test, &env, 290_500_000).succeeds();

    let alice_collateral = 1_100 * ONE_USDC;
    let alice_size = 10_000 * ONE_USDC;
    fund(test, TRADER, TRADER_COLLATERAL, alice_collateral);
    open_position(test, &env, SIDE_LONG, alice_collateral, alice_size).succeeds();
    let bob_collateral = 2_000 * ONE_USDC;
    let bob_size = 9_000 * ONE_USDC;
    fund(
        test,
        SECOND_TRADER,
        SECOND_TRADER_COLLATERAL,
        bob_collateral,
    );
    open_position_for(
        test,
        &env,
        SECOND_TRADER,
        SECOND_TRADER_COLLATERAL,
        SIDE_SHORT,
        bob_collateral,
        bob_size,
    )
    .succeeds();
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(
        u64::from(pool.liquidity) + u64::from(pool.insurance_fund),
        300 * ONE_USDC
    );

    // At $110 Alice is up $1,000 and Bob down $900: h = 300 / 1,000 = 0.3.
    pass_warmup(test);
    set_feed(test, dollars(110), 0);
    let alice_paid = 1_000 * ONE_USDC * (3 * HAIRCUT_PRECISION / 10) / HAIRCUT_PRECISION;
    assert_eq!(alice_paid, 300 * ONE_USDC);
    let alice_fee = alice_size / 1_000;
    close_position(test, &env).succeeds().has_tokens(
        TRADER_COLLATERAL,
        alice_collateral - alice_fee + alice_paid - alice_fee,
    );
    // The whole backing was paid out; the fund then took half of Alice's
    // close fee.
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.liquidity), 0);
    assert_eq!(u64::from(pool.insurance_fund), alice_fee / 2);
    assert_vault_matches_ledger(test, &env);

    // Bob closes at the same price, losing $900 into the pool.
    let bob_fee = bob_size / 1_000;
    let bob_loss = 900 * ONE_USDC;
    close_position_for(test, &env, SECOND_TRADER, SECOND_TRADER_COLLATERAL)
        .succeeds()
        .has_tokens(
            SECOND_TRADER_COLLATERAL,
            bob_collateral - bob_fee - bob_loss - bob_fee,
        );
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.liquidity), bob_loss);
    assert_eq!(u64::from(pool.insurance_fund), (alice_fee + bob_fee) / 2);
    assert_eq!(u64::from(pool.total_collateral), 0);
    assert_vault_matches_ledger(test, &env);
}

/// The haircut counts the insurance fund as backing, so a profit larger than
/// `liquidity` but within `liquidity + insurance_fund` is paid in full: the
/// pool's liquidity first, the insurance fund for the rest.
#[quasar_test]
fn insurance_pays_profit_beyond_liquidity(test: &mut Test) {
    // A 5% open fee, half of which goes to the insurance fund.
    let env = setup_with(
        test,
        InitializePoolInstruction {
            open_fee_bps: 500,
            ..default_initialize_pool()
        },
    );
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 1_700 * ONE_USDC);
    add_liquidity(test, &env, 1_700 * ONE_USDC).succeeds();

    // $500 open fee: $250 to the insurance fund, $1,100 of net collateral.
    let size = 10_000 * ONE_USDC;
    fund(test, TRADER, TRADER_COLLATERAL, 1_600 * ONE_USDC);
    open_position(test, &env, SIDE_LONG, 1_600 * ONE_USDC, size).succeeds();
    assert_eq!(
        u64::from(test.read::<Pool>(env.pool).insurance_fund),
        250 * ONE_USDC
    );

    // At $118 the long is up $1,800: more than the $1,700 of liquidity, within
    // the $1,950 of liquidity plus insurance, so h = 1.
    pass_warmup(test);
    set_feed(test, dollars(118), 0);
    let profit = 1_800 * ONE_USDC;
    let close_fee = size / 1_000;
    close_position(test, &env)
        .succeeds()
        .has_tokens(TRADER_COLLATERAL, 1_100 * ONE_USDC + profit - close_fee);

    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.liquidity), 0);
    // $100 of the profit came from the insurance fund, which then took half
    // of the $10 close fee.
    assert_eq!(
        u64::from(pool.insurance_fund),
        150 * ONE_USDC + close_fee / 2
    );
    assert_vault_matches_ledger(test, &env);
}

/// Shares are priced against assets-under-management, which counts a
/// trader's unrealized loss as the providers' gain, but that loss is still in
/// the trader's collateral. A withdrawal is capped at `liquidity`.
#[quasar_test]
fn remove_liquidity_capped_at_liquidity(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 10_000 * ONE_USDC);
    add_liquidity(test, &env, 10_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    open_position(test, &env, SIDE_LONG, 1_000 * ONE_USDC, 5_000 * ONE_USDC).succeeds();

    // At $80 the long is down $1,000, so assets-under-management is $11,000
    // against $10,000 of liquidity, and each share redeems 1.1 minor units
    // (the provider's shares plus the withheld minimum are 10,000 USDC of
    // shares). 9,090,909,092 shares would redeem 10,000,000,001, one minor
    // unit more than `liquidity`, and are refused.
    set_feed(test, dollars(80), 0);
    remove_liquidity(test, &env, 9_090_909_092).fails_with(error::INSUFFICIENT_LIQUIDITY);

    // One share fewer redeems exactly the pool's liquidity.
    remove_liquidity(test, &env, 9_090_909_091)
        .succeeds()
        .has_tokens(PROVIDER_COLLATERAL, 10_000 * ONE_USDC);
    assert_eq!(u64::from(test.read::<Pool>(env.pool).liquidity), 0);
    assert_vault_matches_ledger(test, &env);
}

/// Open a $5,000 long with $1,000 of collateral against a $100,000 pool and
/// return the slot it opened in.
fn open_long_against_deep_pool(test: &mut Test, env: &Env) -> u64 {
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    open_position(test, env, SIDE_LONG, 1_000 * ONE_USDC, 5_000 * ONE_USDC).succeeds();
    let position = test.read::<Position>(test.derive_pda(Position::seeds(&env.pool, &TRADER)));
    u64::from(position.entry_slot)
}

/// One slot short of the warm-up, a profitable close is refused and the
/// position stays open.
#[quasar_test]
fn profit_blocked_before_maturation(test: &mut Test) {
    let env = setup(test);
    let entry_slot = open_long_against_deep_pool(test, &env);

    let (_, unix_timestamp) = clock(test);
    set_clock_at(test, entry_slot + PROFIT_WARMUP_SLOTS - 1, unix_timestamp);
    set_feed(test, dollars(110), 0);
    close_position(test, &env).fails_with(error::PROFIT_NOT_MATURED);
    assert_eq!(
        u128::from(test.read::<Pool>(env.pool).long_size),
        (5_000 * ONE_USDC) as u128
    );
    assert_eq!(test.tokens(TRADER_COLLATERAL), 0);
}

/// From exactly `entry_slot + profit_warmup_slots`, the profit is paid.
#[quasar_test]
fn profit_realized_after_maturation(test: &mut Test) {
    let env = setup(test);
    let entry_slot = open_long_against_deep_pool(test, &env);

    let (_, unix_timestamp) = clock(test);
    set_clock_at(test, entry_slot + PROFIT_WARMUP_SLOTS, unix_timestamp);
    set_feed(test, dollars(110), 0);
    let size = 5_000 * ONE_USDC;
    let fee = size / 1_000;
    close_position(test, &env)
        .succeeds()
        .has_tokens(TRADER_COLLATERAL, 1_000 * ONE_USDC - fee + size / 10 - fee);
}

/// The warm-up holds back profit only: a losing position closes in the slot
/// it opened.
#[quasar_test]
fn loss_not_gated_by_maturation(test: &mut Test) {
    let env = setup(test);
    let entry_slot = open_long_against_deep_pool(test, &env);

    // Price falls 10% within the same slot: a $500 loss.
    set_feed(test, dollars(90), 0);
    assert_eq!(clock(test).0, entry_slot);
    let size = 5_000 * ONE_USDC;
    let fee = size / 1_000;
    let loss = size / 10;
    close_position(test, &env)
        .succeeds()
        .has_tokens(TRADER_COLLATERAL, 1_000 * ONE_USDC - fee - loss - fee);
    assert_eq!(
        u64::from(test.read::<Pool>(env.pool).liquidity),
        100_000 * ONE_USDC + loss
    );
}

/// `insurance_fee_bps` of each open and close fee goes to the insurance fund,
/// rounded down, and the program keeps the rest, so no minor unit is lost.
#[quasar_test]
fn insurance_fund_funded_by_fees(test: &mut Test) {
    let env = setup_with(
        test,
        InitializePoolInstruction {
            insurance_fee_bps: 3_333,
            ..default_initialize_pool()
        },
    );
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    // A size whose 0.1% fee is 1,234,567.89 minor units, rounded up to
    // 1,234,568: 3,333 basis points of that is 411,481.5, so the insurance
    // fund gets 411,481 (its cut rounds down) and the program the other 823,087.
    let size: u64 = 1_234_567_890;
    let fee = 1_234_568;
    let insurance_cut = 411_481;
    assert_eq!(size.div_ceil(1_000), fee);
    fund(test, TRADER, TRADER_COLLATERAL, 200 * ONE_USDC);
    open_position(test, &env, SIDE_LONG, 200 * ONE_USDC, size).succeeds();
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.insurance_fund), insurance_cut);
    assert_eq!(u64::from(pool.program_fees), fee - insurance_cut);

    // Closing at the open price charges the same fee again.
    close_position(test, &env).succeeds();
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.insurance_fund), 2 * insurance_cut);
    assert_eq!(u64::from(pool.program_fees), 2 * (fee - insurance_cut));
    assert_vault_matches_ledger(test, &env);
}

/// A $1,000 long with $110 of net collateral, liquidated after a 15% fall:
/// its $150 loss leaves equity at -$40.
fn open_long_and_gap_through_zero(test: &mut Test, env: &Env) {
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 160 * ONE_USDC);
    open_position(test, env, SIDE_LONG, 160 * ONE_USDC, 1_000 * ONE_USDC).succeeds();
    set_feed(test, dollars(85), 0);
}

/// A bankrupt position's deficit, its loss beyond its collateral, is paid by
/// the insurance fund when the fund holds enough.
#[quasar_test]
fn insurance_absorbs_bankruptcy_deficit(test: &mut Test) {
    // A 5% open fee, 90% of which goes to the insurance fund: $45 of the $50.
    let env = setup_with(
        test,
        InitializePoolInstruction {
            open_fee_bps: 500,
            insurance_fee_bps: 9_000,
            ..default_initialize_pool()
        },
    );
    open_long_and_gap_through_zero(test, &env);
    assert_eq!(
        u64::from(test.read::<Pool>(env.pool).insurance_fund),
        45 * ONE_USDC
    );
    let liquidity_before = u64::from(test.read::<Pool>(env.pool).liquidity);

    liquidate(test, &env)
        .succeeds()
        .has_tokens(LIQUIDATOR_COLLATERAL, 0);

    // The fund pays the $40 deficit, so the providers keep the $110 of
    // collateral and are credited the full $150 loss.
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.insurance_fund), 5 * ONE_USDC);
    assert_eq!(u64::from(pool.liquidity), liquidity_before + 150 * ONE_USDC);
    assert_vault_matches_ledger(test, &env);
}

/// A position already below zero equity can still be liquidated by anyone.
/// Its equity cannot pay the liquidation fee, so the fee is forgiven and the
/// liquidator receives nothing. The insurance fund pays as much of the deficit
/// as it holds, and the liquidity providers bear only the rest.
#[quasar_test]
fn liquidation_of_bankrupt_position_charges_insurance_before_liquidity(test: &mut Test) {
    // A 5% open fee, half of which goes to the insurance fund: $25 of the $50.
    let env = setup_with(
        test,
        InitializePoolInstruction {
            open_fee_bps: 500,
            ..default_initialize_pool()
        },
    );
    open_long_and_gap_through_zero(test, &env);
    assert_eq!(
        u64::from(test.read::<Pool>(env.pool).insurance_fund),
        25 * ONE_USDC
    );
    let liquidity_before = u64::from(test.read::<Pool>(env.pool).liquidity);

    let position = test.derive_pda(Position::seeds(&env.pool, &TRADER));
    liquidate(test, &env)
        .succeeds()
        .is_closed(position)
        .has_tokens(LIQUIDATOR_COLLATERAL, 0)
        .has_tokens(TRADER_COLLATERAL, 0);

    // The $40 deficit: $25 from the insurance fund, $15 borne by the
    // providers, who keep the $110 of collateral plus the fund's $25.
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.insurance_fund), 0);
    assert_eq!(u64::from(pool.liquidity), liquidity_before + 135 * ONE_USDC);
    assert_eq!(u128::from(pool.long_size), 0);
    assert_eq!(u64::from(pool.total_collateral), 0);
    assert_vault_matches_ledger(test, &env);
}

/// Every fee rounds up, and so does the maintenance requirement, so none is
/// a minor unit short in the trader's favour. A position one base unit over
/// $5,000 pays $5.000001 to open and the same to close: 0.1% of it is
/// 5,000,000.001 base units, rounded up to 5,000,001. The insurance fund's
/// half of a fee rounds down, and the program takes the odd unit. The same
/// position is liquidatable at an equity of 250,000,001 base units, the
/// maintenance requirement 250,000,000.05 rounded up, where a requirement
/// rounded down would have left it one base unit too healthy, and the
/// liquidation fee is 50,000,001.
#[quasar_test]
fn fees_and_maintenance_requirement_round_up(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC + 1;
    let fee = 5_000_001;
    fund(test, TRADER, TRADER_COLLATERAL, 2 * collateral);

    open_position(test, &env, SIDE_LONG, collateral, size).succeeds();
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.insurance_fund), 2_500_000);
    assert_eq!(u64::from(pool.program_fees), 2_500_001);
    assert_eq!(u64::from(pool.total_collateral), collateral - fee);
    let position = test.read::<Position>(test.derive_pda(Position::seeds(&env.pool, &TRADER)));
    assert_eq!(u64::from(position.collateral), collateral - fee);

    // Closing at the entry price settles no profit or loss, so the payout is
    // the net collateral less the close fee.
    close_position(test, &env)
        .succeeds()
        .has_tokens(TRADER_COLLATERAL, 2 * collateral - fee - fee);
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u64::from(pool.insurance_fund), 2 * 2_500_000);
    assert_eq!(u64::from(pool.program_fees), 2 * 2_500_001);

    // The same position again, taken to an equity of exactly the rounded-up
    // maintenance requirement: $85.10000004 loses it 744,999,998 base units.
    open_position(test, &env, SIDE_LONG, collateral, size).succeeds();
    set_feed(test, 8_510_000_004, 0);
    liquidate(test, &env)
        .succeeds()
        .has_tokens(LIQUIDATOR_COLLATERAL, 50_000_001)
        // The trader is refunded the equity less the liquidation fee.
        .has_tokens(
            TRADER_COLLATERAL,
            collateral - fee - fee + (250_000_001 - 50_000_001),
        );
    assert_vault_matches_ledger(test, &env);
}

/// `basis_points_of` rounds up, so an amount that is not an exact multiple
/// rounds to the next base unit and the smallest non-zero amount pays a whole
/// unit; an exact multiple is unchanged. `basis_points_of_rounded_down`
/// splits a fee the pool already holds, so it rounds the other way.
#[test]
fn basis_points_of_rounds_up_and_the_insurance_split_rounds_down() {
    assert_eq!(basis_points_of(5_000 * ONE_USDC, 10).unwrap(), 5 * ONE_USDC);
    assert_eq!(
        basis_points_of(5_000 * ONE_USDC + 1, 10).unwrap(),
        5_000_001
    );
    assert_eq!(basis_points_of(1, 10).unwrap(), 1);
    assert_eq!(basis_points_of(0, 10).unwrap(), 0);
    // Widened to `u128`, so the largest amount does not overflow.
    assert_eq!(basis_points_of(u64::MAX, 10_000).unwrap(), u64::MAX);
    assert_eq!(
        basis_points_of_rounded_down(5_000_001, 5_000).unwrap(),
        2_500_000
    );
    assert_eq!(basis_points_of_rounded_down(1, 5_000).unwrap(), 0);
}

#[quasar_test]
fn initialize_pool_rejects_insurance_fee_at_or_above_full_fee(test: &mut Test) {
    add_pool_prerequisites(test);
    test.send(InitializePoolInstruction {
        insurance_fee_bps: 10_000,
        ..default_initialize_pool()
    })
    .fails_with(error::INVALID_PARAMETER);
    test.send(InitializePoolInstruction {
        insurance_fee_bps: 9_999,
        ..default_initialize_pool()
    })
    .succeeds();
}

#[quasar_test]
fn initialize_pool_rejects_close_fee_at_or_above_maintenance_margin(test: &mut Test) {
    // A pool whose close fee reached the maintenance margin could strand a
    // position that is too healthy to liquidate but too poor to pay the fee to
    // close, so initialize_pool refuses the configuration.
    add_pool_prerequisites(test);
    init_pool(test, 500, 600).fails_with(error::INVALID_PARAMETER);
    init_pool(test, 500, 500).fails_with(error::INVALID_PARAMETER);
    // One basis point below the maintenance margin is accepted.
    init_pool(test, 500, 499).succeeds();
}

#[quasar_test]
fn initialize_pool_records_the_margins_band_and_average(test: &mut Test) {
    let env = setup(test);
    let pool = test.read::<Pool>(env.pool);
    assert_eq!(u16::from(pool.initial_margin_bps), 1_000);
    assert_eq!(u16::from(pool.max_price_deviation_bps), 2_000);
    // The average starts at the oracle price the pool was created against.
    assert_eq!(u64::from(pool.average_price), dollars(100) as u64);
}

#[quasar_test]
fn initialize_pool_rejects_initial_margin_at_or_below_maintenance(test: &mut Test) {
    // An initial margin at or below the 5% maintenance margin would let a
    // position open already liquidatable.
    add_pool_prerequisites(test);
    for initial_margin_bps in [500, 350] {
        test.send(InitializePoolInstruction {
            initial_margin_bps,
            ..default_initialize_pool()
        })
        .fails_with(error::INITIAL_MARGIN_NOT_ABOVE_MAINTENANCE);
    }
    // Above 100% of notional is refused too.
    test.send(InitializePoolInstruction {
        initial_margin_bps: 10_001,
        ..default_initialize_pool()
    })
    .fails_with(error::INVALID_PARAMETER);
    // One basis point above the maintenance margin is accepted.
    test.send(InitializePoolInstruction {
        initial_margin_bps: 501,
        ..default_initialize_pool()
    })
    .succeeds();
}

#[quasar_test]
fn initialize_pool_rejects_price_deviation_outside_range(test: &mut Test) {
    add_pool_prerequisites(test);
    for max_price_deviation_bps in [0, 10_000] {
        test.send(InitializePoolInstruction {
            max_price_deviation_bps,
            ..default_initialize_pool()
        })
        .fails_with(error::INVALID_PRICE_DEVIATION);
    }
    test.send(InitializePoolInstruction {
        max_price_deviation_bps: 9_999,
        ..default_initialize_pool()
    })
    .succeeds();
}

/// A single oracle print far from the pool's average cannot be traded at: the
/// open is refused before the price is folded into the average.
#[quasar_test]
fn open_rejected_when_oracle_jumps_outside_band(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    let size = 5_000 * ONE_USDC;

    // The band is 20% around the $100 average: $125 and $79 are outside it.
    for outside_price in [dollars(125), dollars(79)] {
        set_feed(test, outside_price, 0);
        open_position(test, &env, SIDE_LONG, 1_000 * ONE_USDC, size)
            .fails_with(error::PRICE_OUTSIDE_BAND);
        // The refused open folded nothing into the average.
        assert_eq!(pool_state(test, &env).0, dollars(100) as u64);
    }

    // $118 is inside the band, and opens at that price.
    set_feed(test, dollars(118), 0);
    open_position(test, &env, SIDE_LONG, 1_000 * ONE_USDC, size).succeeds();
    let position = test.read::<Position>(test.derive_pda(Position::seeds(&env.pool, &TRADER)));
    assert_eq!(u64::from(position.entry_price), dollars(118) as u64);
}

#[quasar_test]
fn close_rejected_when_oracle_jumps_outside_band(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    fund(test, TRADER, TRADER_COLLATERAL, collateral);
    open_position(test, &env, SIDE_LONG, collateral, size).succeeds();

    // A jump to $125 would pay the long $1,250, but $125 is 25% from the
    // $100 average, outside the 20% band.
    set_feed(test, dollars(125), 0);
    close_position(test, &env).fails_with(error::PRICE_OUTSIDE_BAND);

    // At $115, inside the band and after the warm-up, the close goes through
    // and pays the 15% gain.
    pass_warmup(test);
    set_feed(test, dollars(115), 0);
    let fee = size / 1_000;
    let profit = size * 15 / 100;
    close_position(test, &env)
        .succeeds()
        .has_tokens(TRADER_COLLATERAL, collateral - fee + profit - fee);
}

/// Liquidation has no band check: a genuine crash is when positions go
/// underwater, so the pool has to be able to liquidate through one.
#[quasar_test]
fn liquidation_runs_outside_band(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_100 * ONE_USDC);
    open_position(test, &env, SIDE_LONG, 1_100 * ONE_USDC, 10_000 * ONE_USDC).succeeds();

    // $75 is 25% below the $100 average, so the owner cannot close there.
    set_feed(test, dollars(75), 0);
    close_position(test, &env).fails_with(error::PRICE_OUTSIDE_BAND);

    test.add(Wallet::new().at(LIQUIDATOR));
    let position = test.derive_pda(Position::seeds(&env.pool, &TRADER));
    test.send(LiquidatePositionInstruction {
        liquidator: LIQUIDATOR,
        owner: TRADER,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: TRADER_COLLATERAL,
        liquidator_collateral: LIQUIDATOR_COLLATERAL,
    })
    .succeeds()
    .is_closed(position);
    assert_eq!(u128::from(test.read::<Pool>(env.pool).long_size), 0);
}

#[quasar_test]
fn liquidity_changes_rejected_when_oracle_jumps_outside_band(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 15_000 * ONE_USDC);
    add_liquidity(test, &env, 10_000 * ONE_USDC).succeeds();
    let shares = test.tokens(PROVIDER_LP);

    // $76 is 24% below the $100 average.
    set_feed(test, dollars(76), 0);
    add_liquidity(test, &env, 5_000 * ONE_USDC).fails_with(error::PRICE_OUTSIDE_BAND);
    remove_liquidity(test, &env, shares).fails_with(error::PRICE_OUTSIDE_BAND);
}

/// After a genuine move outside the band, anyone can walk the average toward
/// the new price with `update_price_average`, and trading resumes once the
/// price is back inside the band.
#[quasar_test]
fn price_average_catches_up_after_genuine_move(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    fund(test, TRADER, TRADER_COLLATERAL, collateral);

    // NVDAx reprices from $100 to $130, 30% away from the average.
    let new_price = dollars(130);
    set_feed(test, new_price, 0);
    open_position(test, &env, SIDE_LONG, collateral, size).fails_with(error::PRICE_OUTSIDE_BAND);

    // Every two minutes the keeper calls `update_price_average`. Each call
    // credits the two minutes since the previous read to the price that read
    // saw, a fifth of the window. The first call credits $100, the price
    // before the move, and records $130; each later call moves the average a
    // fifth of the remaining gap to $130: $100, then $106, then $110.80. $130
    // is within 20% of any average from $108.34 up, so the third update
    // reopens trading.
    let mut updates = 0;
    loop {
        update_average_after(test, &env, 120, new_price).succeeds();
        updates += 1;
        let opened = open_position(test, &env, SIDE_LONG, collateral, size);
        if opened.is_ok() {
            break;
        }
        opened.fails_with(error::PRICE_OUTSIDE_BAND);
        assert!(updates < 10, "the average never caught up");
    }
    assert_eq!(updates, 3);
    let (average_price, last_oracle_price, _) = pool_state(test, &env);
    assert_eq!(average_price, 11_080_000_000);
    assert_eq!(last_oracle_price, new_price as u64);
}

#[quasar_test]
fn single_update_moves_average_by_elapsed_fraction(test: &mut Test) {
    let env = setup(test);
    let (_, _, created_at) = pool_state(test, &env);

    // The first update after the oracle moves to $115 credits the four
    // minutes since creation to $100, the price seen at creation, so the
    // average stays at $100 and $115 is recorded for the next read.
    update_average_after(test, &env, 240, dollars(115)).succeeds();
    assert_eq!(
        pool_state(test, &env),
        (dollars(100) as u64, dollars(115) as u64, created_at + 240)
    );

    // Four more minutes at $115 are 240 of the 600-second window, so the next
    // update moves the average 240/600 of the way from $100 to $115: to $106.
    update_average_after(test, &env, 240, dollars(115)).succeeds();
    assert_eq!(
        pool_state(test, &env),
        (dollars(106) as u64, dollars(115) as u64, created_at + 480)
    );

    // Fifteen minutes is more than a full window, so the next update replaces
    // the average with $115, the price at the previous read, and records the
    // fall to $97. One more update credits $97 for a full window.
    update_average_after(test, &env, 900, dollars(97)).succeeds();
    assert_eq!(
        pool_state(test, &env),
        (dollars(115) as u64, dollars(97) as u64, created_at + 1_380)
    );
    update_average_after(test, &env, 900, dollars(97)).succeeds();
    assert_eq!(pool_state(test, &env).0, dollars(97) as u64);
}

/// A pool left idle for more than a window cannot have its average set by one
/// read of a manipulated price. The read only records the price; the interval
/// before it is credited to the price seen at the read before. Once a read of
/// the real price replaces it, the manipulated price has moved the average
/// only by the seconds between the two reads.
#[quasar_test]
fn one_manipulated_read_after_idle_does_not_move_average(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    let collateral = 1_000 * ONE_USDC;
    fund(test, TRADER, TRADER_COLLATERAL, collateral);

    // Fifteen idle minutes, then the oracle is pushed to $160 and
    // `update_price_average` is called. The average stays at $100.
    update_average_after(test, &env, 900, dollars(160)).succeeds();
    let (average_price, last_oracle_price, _) = pool_state(test, &env);
    assert_eq!(average_price, dollars(100) as u64);
    assert_eq!(last_oracle_price, dollars(160) as u64);

    // Six seconds later the oracle is back at $100 and is read again. The six
    // seconds are credited to $160: the average moves 6/600 of the $60 gap,
    // to $100.60, and $100 replaces $160 as the latest observation.
    update_average_after(test, &env, 6, dollars(100)).succeeds();
    let (average_price, last_oracle_price, last_fold) = pool_state(test, &env);
    assert_eq!(average_price, 10_060_000_000);
    assert_eq!(last_oracle_price, dollars(100) as u64);

    // An open at $160 is still refused.
    set_feed_at_slot(test, dollars(160), last_fold as u64 * SLOTS_PER_SECOND, 0);
    open_position(test, &env, SIDE_LONG, collateral, 5_000 * ONE_USDC)
        .fails_with(error::PRICE_OUTSIDE_BAND);
}
