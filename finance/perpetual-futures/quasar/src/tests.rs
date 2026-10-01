//! quasar-test integration tests. They exercise the full lifecycle: pool
//! initialization, liquidity add/remove, opening/closing/liquidating leveraged
//! positions, fee collection, the price average and its band, and the
//! oracle/margin/reserve checks.

use {
    crate::{
        constants::{MAX_FUNDING_RATE_PER_SECOND, SIDE_LONG, SIDE_SHORT},
        cpi::{
            AddLiquidityInstruction, ClosePositionInstruction, CollectFeesInstruction,
            InitializePoolInstruction, LiquidatePositionInstruction, OpenPositionInstruction,
            RemoveLiquidityInstruction, UpdatePriceAverageInstruction,
        },
        instructions::shared::error,
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

// Matches `PRICE_AVERAGE_WINDOW_SECONDS`: one fold after this many seconds
// replaces the pool's average price with the oracle price.
const PRICE_AVERAGE_WINDOW_SECONDS: i64 = 600;

// Ten years, in seconds.
const TEN_YEARS: i64 = 315_360_000;

fn dollars(whole: i128) -> i128 {
    whole * 10i128.pow(ORACLE_SCALE)
}

/// A feed account in this program's layout: price (i128), scale (u32),
/// last_update_slot (u64), confidence (u64). The tests own this; production
/// reads a real feed.
fn set_feed(test: &mut Test, price: i128, confidence: u64) {
    set_feed_at_slot(test, price, SLOT, confidence);
}

fn set_feed_at_slot(test: &mut Test, price: i128, slot: u64, confidence: u64) {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&ORACLE_SCALE.to_le_bytes());
    data.extend_from_slice(&slot.to_le_bytes());
    data.extend_from_slice(&confidence.to_le_bytes());
    test.set_account(Account::new(FEED, system_program::ID, 1_000_000, data));
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
    let clock_id: Pubkey = "SysvarC1ock11111111111111111111111111111111"
        .parse()
        .unwrap();
    let sysvar_owner: Pubkey = "Sysvar1111111111111111111111111111111111111"
        .parse()
        .unwrap();
    test.set_account(Account::new(clock_id, sysvar_owner, 1_169_280, data));
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
/// close fees, a 10% initial margin (10x leverage), a 5% maintenance margin, a
/// 1% liquidation fee, a 1% maximum confidence band, a 20% price band around
/// the pool's average price, and no funding.
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
    add_pool_prerequisites(test);
    init_pool_with_funding(test, 500, 10, funding_rate_per_second).succeeds();

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
    test.send(OpenPositionInstruction {
        owner: TRADER,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: TRADER_COLLATERAL,
        side,
        collateral_amount: collateral,
        size,
        acceptable_price: 0,
    })
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
    test.send(ClosePositionInstruction {
        owner: TRADER,
        oracle_feed: FEED,
        collateral_mint: COLLATERAL_MINT,
        custody_vault: env.custody_vault,
        trader_collateral: TRADER_COLLATERAL,
        minimum_payout: 0,
    })
}

#[quasar_test]
fn initialize_pool_creates_pool_vault_and_lp_mint(test: &mut Test) {
    let env = setup(test);
    // The pool, vault, and liquidity-provider mint were created.
    assert!(test.account(env.pool).is_some());
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
    // The steepest rate a pool may have, held for ten years. The position is
    // tiny because a pool holding 1_001 can back only 1_001 of notional.
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
    assert!(
        open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC).is_err(),
        "a pre-restart price must be rejected even inside the staleness bound"
    );

    // Publishing after the restart (slot 10) reopens the pool.
    set_feed_at_slot(test, dollars(100), 10, 0);
    open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC).succeeds();
}

#[quasar_test]
fn close_long_in_profit_pays_collateral_plus_pnl_minus_fees(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    let size = 5_000 * ONE_USDC;
    open_position(test, &env, 0, 1_000 * ONE_USDC, size).succeeds();

    // Price rises 20%: a $5,000 long earns $1,000.
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
    // The open fee (0.1% of notional) was swept to the admin.
    .has_tokens(ADMIN_COLLATERAL, size / 1_000);
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

    assert!(
        flat > 0,
        "the flat run must pay some funding to compare against"
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
    let expected = size * MAX_FUNDING_RATE_PER_SECOND * one_hour as u64 / 1_000_000_000;
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
    assert!(
        open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC).is_err(),
        "a confidence band wider than max_confidence_bps must be rejected"
    );
}

#[quasar_test]
fn open_rejects_when_pool_cannot_back_it(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 3_000 * ONE_USDC);
    add_liquidity(test, &env, 3_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    // A 5,000 position must reserve 5,000, but the pool only holds 3,000.
    assert!(
        open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC).is_err(),
        "a position larger than the pool's free liquidity must be rejected"
    );
}

#[quasar_test]
fn profit_is_capped_at_the_reserved_notional(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    let collateral = 2_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    fund(test, TRADER, TRADER_COLLATERAL, collateral);
    open_position(test, &env, 0, collateral, size).succeeds();

    // Price triples: uncapped profit would be 2x the notional, but recoverable
    // profit is capped at the reserved notional (`size`). A move this large is
    // far outside the price band, so the average has to catch up before the
    // position can close: one update records $300, and a second a full window
    // later credits that window to $300, replacing the average.
    set_feed(test, dollars(300), 0);
    update_average_after(test, &env, 0, dollars(300)).succeeds();
    update_average_after(test, &env, PRICE_AVERAGE_WINDOW_SECONDS, dollars(300)).succeeds();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let expected = net_collateral + size - close_fee;
    close_position(test, &env)
        .succeeds()
        .has_tokens(TRADER_COLLATERAL, expected);
}

#[quasar_test]
fn remove_liquidity_is_blocked_by_reserved_notional(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 10_000 * ONE_USDC);
    add_liquidity(test, &env, 10_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    open_position(test, &env, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC).succeeds();

    // 5,000 of the 10,000 liquidity is reserved: pulling everything fails, but
    // withdrawing within the free half succeeds.
    let shares = test.tokens(PROVIDER_LP);
    assert!(
        remove_liquidity(test, &env, shares).is_err(),
        "withdrawing reserved liquidity must fail"
    );
    remove_liquidity(test, &env, shares / 2).succeeds();
}

#[quasar_test]
fn initialize_pool_rejects_close_fee_at_or_above_maintenance_margin(test: &mut Test) {
    // A pool whose close fee reached the maintenance margin could strand a
    // position that is too healthy to liquidate but too poor to pay the fee to
    // close, so initialize_pool refuses the configuration.
    add_pool_prerequisites(test);
    init_pool(test, 500, 600).fails_with(error::INVALID_PARAMETER);
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

    // At $115, inside the band, the close goes through and pays the 15% gain.
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
