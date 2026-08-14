//! quasar-test integration tests. They exercise the full lifecycle: pool
//! initialization, liquidity add/remove, opening/closing/liquidating leveraged
//! positions, fee collection, and the oracle/leverage/reserve guard rails.

use {
    crate::{
        cpi::{
            AddLiquidityInstruction, ClosePositionInstruction, CollectFeesInstruction,
            InitializePoolInstruction, LiquidatePositionInstruction, OpenPositionInstruction,
            RemoveLiquidityInstruction, SetFundingRateInstruction,
        },
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

/// Pin the Clock sysvar account at `slot`. Clock's bincode layout is the raw
/// little-endian fields: slot, epoch_start_timestamp, epoch,
/// leader_schedule_epoch, unix_timestamp.
fn set_clock_at(test: &mut Test, slot: u64) {
    let mut data = Vec::with_capacity(40);
    data.extend_from_slice(&slot.to_le_bytes());
    data.extend_from_slice(&0i64.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0i64.to_le_bytes());
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
    funding_rate_per_slot: u64,
) -> Outcome {
    test.send(InitializePoolInstruction {
        authority: ADMIN,
        collateral_mint: COLLATERAL_MINT,
        oracle_feed: FEED,
        oracle_scale: ORACLE_SCALE,
        funding_rate_per_slot,
        open_fee_bps: 10,
        close_fee_bps,
        max_leverage: 10,
        maintenance_margin_bps,
        liquidation_fee_bps: 100,
        max_confidence_bps: 100,
    })
}

/// The pool and its derived PDAs.
struct Env {
    pool: Pubkey,
    lp_mint: Pubkey,
    custody_vault: Pubkey,
}

/// Build a world with a collateral mint, an oracle feed at $100, and an
/// initialized pool (0.1% open/close fees, 10x max leverage, 5% maintenance
/// margin, 1% liquidation fee, 1% max confidence).
fn setup(test: &mut Test) -> Env {
    setup_with_funding(test, 0)
}

/// Like `setup`, but with a non-zero per-slot funding rate so funding accrues
/// as slots pass.
fn setup_with_funding(test: &mut Test, funding_rate_per_slot: u64) -> Env {
    test.add(Wallet::new().at(ADMIN));
    test.add(Mint::new(ADMIN).at(COLLATERAL_MINT).decimals(6));
    set_feed(test, dollars(100), 0);
    init_pool_with_funding(test, 500, 10, funding_rate_per_slot).succeeds();

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
    assert!(test.account(env.custody_vault).is_some());
    assert!(test.account(env.lp_mint).is_some());
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
fn remove_liquidity_round_trip_returns_the_deposit(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 10_000 * ONE_USDC);
    add_liquidity(test, &env, 10_000 * ONE_USDC).succeeds();

    let shares = test.tokens(PROVIDER_LP);
    remove_liquidity(test, &env, shares)
        .succeeds()
        // Sole provider reclaims the full deposit.
        .has_tokens(PROVIDER_COLLATERAL, 10_000 * ONE_USDC);
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
    set_clock_at(test, 10);
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
fn open_rejects_excess_leverage(test: &mut Test) {
    let env = setup(test);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();

    fund(test, TRADER, TRADER_COLLATERAL, 1_000 * ONE_USDC);
    // 11x exceeds the 10x maximum.
    assert!(
        open_position(test, &env, 0, 1_000 * ONE_USDC, 11_000 * ONE_USDC).is_err(),
        "11x leverage must be rejected"
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

/// The funding rate is quoted per slot, so what a position costs per hour also
/// depends on the cluster's slot time. When the protocol shortens the slot, the
/// pool authority retunes the rate, and the retune settles the slots already
/// elapsed at the old rate rather than repricing them at the new one.
///
/// Both halves below hold the same position for the same slots at the same
/// price, so the size and price scaling cancels and only the rates differ: the
/// spanning position pays one window at the old rate plus one at the new (3
/// window-rates), and the position opened afterwards pays one window wholly at
/// the new rate (2 window-rates).
#[quasar_test]
fn set_funding_rate_settles_at_the_old_rate_first(test: &mut Test) {
    let rate = 5_000;
    let window = 2_000;
    let size = 5_000 * ONE_USDC;
    let collateral = 1_000 * ONE_USDC;
    let fees = 2 * (size / 1_000); // open and close, 0.1% of notional each

    let env = setup_with_funding(test, rate);
    fund(test, PROVIDER, PROVIDER_COLLATERAL, 100_000 * ONE_USDC);
    add_liquidity(test, &env, 100_000 * ONE_USDC).succeeds();
    fund(test, TRADER, TRADER_COLLATERAL, 10_000 * ONE_USDC);

    // A position held across the retune: one window at `rate`, one at `rate * 2`.
    let before_spanning = test.tokens(TRADER_COLLATERAL);
    open_position(test, &env, 0, collateral, size).succeeds();
    set_clock_at(test, window);
    test.send(SetFundingRateInstruction {
        authority: ADMIN,
        collateral_mint: COLLATERAL_MINT,
        oracle_feed: FEED,
        funding_rate_per_slot: rate * 2,
    })
    .succeeds();
    set_clock_at(test, 2 * window);
    set_feed_at_slot(test, dollars(100), 2 * window, 0);
    close_position(test, &env).succeeds();
    let spanning = (before_spanning - test.tokens(TRADER_COLLATERAL)) - fees;

    // A fresh position over one window, now wholly at the doubled rate.
    let before_doubled = test.tokens(TRADER_COLLATERAL);
    open_position(test, &env, 0, collateral, size).succeeds();
    set_clock_at(test, 3 * window);
    set_feed_at_slot(test, dollars(100), 3 * window, 0);
    close_position(test, &env).succeeds();
    let doubled = (before_doubled - test.tokens(TRADER_COLLATERAL)) - fees;

    assert!(doubled > 0, "the doubled-rate window must charge some funding");
    assert_eq!(
        spanning * 2,
        doubled * 3,
        "a position spanning the retune should pay 1.5x one doubled window: \
         spanning {spanning}, doubled {doubled}"
    );
}

#[quasar_test]
fn only_the_authority_can_set_the_funding_rate(test: &mut Test) {
    let env = setup_with_funding(test, 5_000);
    let _ = env;
    fund(test, TRADER, TRADER_COLLATERAL, ONE_USDC);
    assert!(
        test.send(SetFundingRateInstruction {
            authority: TRADER,
            collateral_mint: COLLATERAL_MINT,
            oracle_feed: FEED,
            funding_rate_per_slot: 1,
        })
        .is_err(),
        "a non-authority must not be able to retune the funding rate"
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
    // profit is capped at the reserved notional (`size`).
    set_feed(test, dollars(300), 0);

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
    test.add(Wallet::new().at(ADMIN));
    test.add(Mint::new(ADMIN).at(COLLATERAL_MINT).decimals(6));
    set_feed(test, dollars(100), 0);
    assert!(
        init_pool(test, 500, 600).is_err(),
        "close_fee_bps >= maintenance_margin_bps must be rejected"
    );
}
