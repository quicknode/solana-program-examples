//! quasar-test integration tests: initialize a market, stock inventory, swap
//! against the oracle-anchored quote, and exercise every guard rail (operator
//! gating, slippage, staleness, confidence, pause, inventory limits).

use {
    crate::{
        cpi::{
            DepositInventoryInstruction, InitializeMarketInstruction, SetQuoteInstruction,
            SwapInstruction, WithdrawInventoryInstruction,
        },
        state::Market,
        BaseVaultPda, QuoteVaultPda,
    },
    quasar_test::prelude::*,
};

// Both tokens have 6 decimals: base is NVDAx (tokenized NVIDIA stock), quote
// is USDC.
const ONE_TOKEN: u64 = 1_000_000;
// The oracle quotes prices with 8 decimals, so $165 is 165 * 10^8.
const ORACLE_SCALE: u32 = 8;
const SPREAD_BPS: u16 = 10;
const MAX_CONFIDENCE_BPS: u16 = 100;

const DIRECTION_BUY_BASE: u8 = 0;
const DIRECTION_SELL_BASE: u8 = 1;

// A fixed current slot well above the staleness bound, so tests can write
// feed accounts that are fresh (slot = SLOT) or stale (slot older than the
// 150-slot bound). quasar-test has no slot control, so the Clock sysvar
// ACCOUNT is overridden directly — the SVM fills its sysvar cache from
// provided accounts before falling back to defaults.
const SLOT: u64 = 1_000;

// Deterministic addresses.
const OPERATOR: Pubkey = Pubkey::new_from_array([1; 32]);
const BASE_MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const QUOTE_MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const FEED: Pubkey = Pubkey::new_from_array([4; 32]);
const OPERATOR_BASE: Pubkey = Pubkey::new_from_array([5; 32]);
const OPERATOR_QUOTE: Pubkey = Pubkey::new_from_array([6; 32]);
const TRADER: Pubkey = Pubkey::new_from_array([7; 32]);
const TRADER_BASE: Pubkey = Pubkey::new_from_array([8; 32]);
const TRADER_QUOTE: Pubkey = Pubkey::new_from_array([9; 32]);
const MALLORY: Pubkey = Pubkey::new_from_array([10; 32]);
const MALLORY_BASE: Pubkey = Pubkey::new_from_array([11; 32]);
const MALLORY_QUOTE: Pubkey = Pubkey::new_from_array([12; 32]);

fn dollars(whole: i128) -> i128 {
    whole * 10i128.pow(ORACLE_SCALE)
}

/// Pin the Clock sysvar account at `SLOT`. Clock's bincode layout is the raw
/// little-endian fields: slot, epoch_start_timestamp, epoch,
/// leader_schedule_epoch, unix_timestamp.
fn set_clock(test: &mut Test) {
    let mut data = Vec::with_capacity(40);
    data.extend_from_slice(&SLOT.to_le_bytes());
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

/// A feed account in this program's layout: price (i128), scale (u32),
/// last_update_slot (u64), confidence (u64). The tests own this; production
/// reads a real feed.
fn set_feed_at_slot(test: &mut Test, price: i128, slot: u64, confidence: u64) {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&ORACLE_SCALE.to_le_bytes());
    data.extend_from_slice(&slot.to_le_bytes());
    data.extend_from_slice(&confidence.to_le_bytes());
    test.set_account(Account::new(FEED, system_program::ID, 1_000_000, data));
}

fn set_feed(test: &mut Test, price: i128, confidence: u64) {
    set_feed_at_slot(test, price, SLOT, confidence);
}

/// Write the feed with an update slot older than the 150-slot staleness bound
/// (the Clock sysvar sits at `SLOT`).
fn make_price_stale(test: &mut Test) {
    set_feed_at_slot(test, dollars(165), SLOT - 151, 0);
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

fn init_market(test: &mut Test, spread_bps: u16) -> Outcome {
    test.send(InitializeMarketInstruction {
        operator: OPERATOR,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        oracle_feed: FEED,
        oracle_scale: ORACLE_SCALE,
        spread_bps,
        max_confidence_bps: MAX_CONFIDENCE_BPS,
    })
}

struct Env {
    market: Pubkey,
    base_vault: Pubkey,
    quote_vault: Pubkey,
}

/// Mints, feed at $165, clock at `SLOT`, funded operator inventory accounts,
/// and an initialized (but unstocked) market at `spread_bps`.
fn base_world(test: &mut Test, spread_bps: u16) -> (Env, Outcome) {
    test.add(Wallet::new().at(OPERATOR));
    test.add(Mint::new(OPERATOR).at(BASE_MINT).decimals(6));
    test.add(Mint::new(OPERATOR).at(QUOTE_MINT).decimals(6));
    set_feed(test, dollars(165), 0);
    set_clock(test);
    test.add(
        TokenAccount::new(BASE_MINT, OPERATOR)
            .at(OPERATOR_BASE)
            .amount(10_000 * ONE_TOKEN),
    );
    test.add(
        TokenAccount::new(QUOTE_MINT, OPERATOR)
            .at(OPERATOR_QUOTE)
            .amount(10_000_000 * ONE_TOKEN),
    );
    let outcome = init_market(test, spread_bps);
    let market = test.derive_pda(Market::seeds(&BASE_MINT, &QUOTE_MINT));
    (
        Env {
            market,
            base_vault: test.derive_pda(BaseVaultPda::seeds(&market)),
            quote_vault: test.derive_pda(QuoteVaultPda::seeds(&market)),
        },
        outcome,
    )
}

fn deposit_inventory(
    test: &mut Test,
    env: &Env,
    signer: Pubkey,
    signer_base: Pubkey,
    signer_quote: Pubkey,
    base: u64,
    quote: u64,
) -> Outcome {
    test.send(DepositInventoryInstruction {
        operator: signer,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        base_vault: env.base_vault,
        quote_vault: env.quote_vault,
        operator_base: signer_base,
        operator_quote: signer_quote,
        base_amount: base,
        quote_amount: quote,
    })
}

/// Market with a 10 bps spread and 1,000 NVDAx + 200,000 USDC of operator
/// inventory deposited.
fn setup(test: &mut Test) -> Env {
    let (env, outcome) = base_world(test, SPREAD_BPS);
    outcome.succeeds();
    deposit_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        1_000 * ONE_TOKEN,
        200_000 * ONE_TOKEN,
    )
    .succeeds();
    env
}

fn fund_trader(
    test: &mut Test,
    wallet: Pubkey,
    base_account: Pubkey,
    quote_account: Pubkey,
    base: u64,
    quote: u64,
) {
    test.add(Wallet::new().at(wallet));
    test.add(
        TokenAccount::new(BASE_MINT, wallet)
            .at(base_account)
            .amount(base),
    );
    test.add(
        TokenAccount::new(QUOTE_MINT, wallet)
            .at(quote_account)
            .amount(quote),
    );
}

fn set_quote(test: &mut Test, signer: Pubkey, spread_bps: u16, paused: u8) -> Outcome {
    test.send(SetQuoteInstruction {
        operator: signer,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        spread_bps,
        paused,
    })
}

fn swap(
    test: &mut Test,
    env: &Env,
    trader: Pubkey,
    trader_base: Pubkey,
    trader_quote: Pubkey,
    direction: u8,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Outcome {
    test.send(SwapInstruction {
        trader,
        oracle_feed: FEED,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        base_vault: env.base_vault,
        quote_vault: env.quote_vault,
        trader_base,
        trader_quote,
        direction,
        amount_in,
        minimum_amount_out,
    })
}

fn withdraw_inventory(
    test: &mut Test,
    env: &Env,
    signer: Pubkey,
    signer_base: Pubkey,
    signer_quote: Pubkey,
    base: u64,
    quote: u64,
) -> Outcome {
    test.send(WithdrawInventoryInstruction {
        operator: signer,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        base_vault: env.base_vault,
        quote_vault: env.quote_vault,
        operator_base: signer_base,
        operator_quote: signer_quote,
        base_amount: base,
        quote_amount: quote,
    })
}

#[quasar_test]
fn initialize_market_creates_market_and_stocked_vaults(test: &mut Test) {
    let env = setup(test);
    // The market and both vaults were created, and the inventory landed.
    assert!(test.account(env.market).is_some());
    assert_eq!(test.tokens(env.base_vault), 1_000 * ONE_TOKEN);
    assert_eq!(test.tokens(env.quote_vault), 200_000 * ONE_TOKEN);
}

/// Alice buys 5 NVDAx. At $165 with a 10 bps spread the ask is $165.165, so
/// 5 NVDAx costs exactly 825.825 USDC.
#[quasar_test]
fn swap_buys_base_at_the_ask(test: &mut Test) {
    let env = setup(test);
    let quote_in = 825_825_000;
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, quote_in);

    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        quote_in,
        5 * ONE_TOKEN,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, 5 * ONE_TOKEN)
    .has_tokens(TRADER_QUOTE, 0)
    // Conservation: the vaults moved by exactly the two legs of the fill.
    .has_tokens(env.base_vault, 995 * ONE_TOKEN)
    .has_tokens(env.quote_vault, 200_000 * ONE_TOKEN + quote_in);
}

/// Bob sells 5 NVDAx. At $165 with a 10 bps spread the bid is $164.835, so
/// he receives exactly 824.175 USDC.
#[quasar_test]
fn swap_sells_base_at_the_bid(test: &mut Test) {
    let env = setup(test);
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 5 * ONE_TOKEN, 0);

    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_SELL_BASE,
        5 * ONE_TOKEN,
        824_175_000,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, 0)
    .has_tokens(TRADER_QUOTE, 824_175_000);
}

/// A buy immediately followed by a sell of the same 5 NVDAx costs exactly
/// the round-trip spread: 1.65 USDC, all of which stays in the inventory.
#[quasar_test]
fn round_trip_costs_exactly_the_spread(test: &mut Test) {
    let env = setup(test);
    let quote_in = 825_825_000;
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, quote_in);

    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        quote_in,
        0,
    )
    .succeeds();
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_SELL_BASE,
        5 * ONE_TOKEN,
        0,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, 0)
    .has_tokens(TRADER_QUOTE, quote_in - 1_650_000)
    .has_tokens(env.quote_vault, 200_000 * ONE_TOKEN + 1_650_000);
}

/// When the oracle reprices, the quote follows instantly. At $170 the ask is
/// $170.17, so 5 NVDAx costs exactly 850.85 USDC.
#[quasar_test]
fn quote_follows_the_oracle(test: &mut Test) {
    let env = setup(test);
    set_feed(test, dollars(170), 0);

    let quote_in = 850_850_000;
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, quote_in);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        quote_in,
        5 * ONE_TOKEN,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, 5 * ONE_TOKEN);
}

/// The operator re-quotes to a 50 bps spread; the next fill prices at
/// $165.825, so 5 NVDAx costs exactly 829.125 USDC.
#[quasar_test]
fn set_quote_changes_the_spread(test: &mut Test) {
    let env = setup(test);
    set_quote(test, OPERATOR, 50, 0).succeeds();

    let quote_in = 829_125_000;
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, quote_in);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        quote_in,
        5 * ONE_TOKEN,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, 5 * ONE_TOKEN);
}

/// The operator can withdraw every token in both vaults at any time — its
/// capital, its exit. Afterwards swaps fail rather than misprice.
#[quasar_test]
fn operator_can_withdraw_everything_and_swaps_then_fail(test: &mut Test) {
    let env = setup(test);
    withdraw_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        1_000 * ONE_TOKEN,
        200_000 * ONE_TOKEN,
    )
    .succeeds()
    .has_tokens(env.base_vault, 0)
    .has_tokens(env.quote_vault, 0);

    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, 825_825_000);
    assert!(
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            825_825_000,
            0
        )
        .is_err(),
        "a swap against an empty inventory must fail"
    );
}

#[quasar_test]
fn withdraw_more_than_inventory_fails(test: &mut Test) {
    let env = setup(test);
    assert!(
        withdraw_inventory(
            test,
            &env,
            OPERATOR,
            OPERATOR_BASE,
            OPERATOR_QUOTE,
            1_001 * ONE_TOKEN,
            0
        )
        .is_err(),
        "withdrawing more than the vault holds must fail"
    );
}

#[quasar_test]
fn deposit_rejects_non_operator(test: &mut Test) {
    let env = setup(test);
    fund_trader(
        test,
        MALLORY,
        MALLORY_BASE,
        MALLORY_QUOTE,
        ONE_TOKEN,
        ONE_TOKEN,
    );
    assert!(
        deposit_inventory(
            test,
            &env,
            MALLORY,
            MALLORY_BASE,
            MALLORY_QUOTE,
            ONE_TOKEN,
            0
        )
        .is_err(),
        "deposit_inventory must reject a non-operator signer"
    );
}

#[quasar_test]
fn withdraw_rejects_non_operator(test: &mut Test) {
    let env = setup(test);
    fund_trader(test, MALLORY, MALLORY_BASE, MALLORY_QUOTE, 0, 0);
    assert!(
        withdraw_inventory(
            test,
            &env,
            MALLORY,
            MALLORY_BASE,
            MALLORY_QUOTE,
            ONE_TOKEN,
            0
        )
        .is_err(),
        "withdraw_inventory must reject a non-operator signer"
    );
}

#[quasar_test]
fn set_quote_rejects_non_operator(test: &mut Test) {
    setup(test);
    test.add(Wallet::new().at(MALLORY));
    assert!(
        set_quote(test, MALLORY, 500, 1).is_err(),
        "set_quote must reject a non-operator signer"
    );
}

/// A fill below the caller's minimum is rejected, not filled worse.
#[quasar_test]
fn swap_rejects_slippage(test: &mut Test) {
    let env = setup(test);
    let quote_in = 825_825_000;
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, quote_in);
    // The fill would be exactly 5 NVDAx; demand one minor unit more.
    assert!(
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            quote_in,
            5 * ONE_TOKEN + 1
        )
        .is_err(),
        "a fill below minimum_amount_out must be rejected"
    );
}

/// An oracle price older than the staleness bound cannot be traded against:
/// a lagging quote is a free option for arbitrageurs.
#[quasar_test]
fn swap_rejects_stale_price(test: &mut Test) {
    let env = setup(test);
    make_price_stale(test);
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, 825_825_000);
    assert!(
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            825_825_000,
            0
        )
        .is_err(),
        "a stale oracle price must be rejected"
    );
}

/// A cluster restart passes hours of wall-clock time in zero slots, so a
/// price published before the halt can still look fresh by slot count. The
/// market must refuse to quote against it until the publisher posts again.
#[quasar_test]
fn swap_rejects_price_from_before_a_restart(test: &mut Test) {
    let env = setup(test);
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, 825_825_000 * 2);

    // The feed is stamped at `SLOT - 5`: fresh by the 150-slot staleness
    // bound, but published before a restart at `SLOT - 3`, so only the
    // restart check can catch it.
    set_feed_at_slot(test, dollars(165), SLOT - 5, 0);
    set_last_restart_slot(test, SLOT - 3);
    assert!(
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            825_825_000,
            0
        )
        .is_err(),
        "a pre-restart price must be rejected even inside the staleness bound"
    );

    // Publishing after the restart (at `SLOT`) reopens the market.
    set_feed(test, dollars(165), 0);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        825_825_000,
        0,
    )
    .succeeds();
}

/// A price the oracle itself is unsure about is rejected: the confidence band
/// (about 1.2% here) exceeds the market's 1% limit.
#[quasar_test]
fn swap_rejects_wide_confidence(test: &mut Test) {
    let env = setup(test);
    set_feed(test, dollars(165), 200_000_000);
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, 825_825_000);
    assert!(
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            825_825_000,
            0
        )
        .is_err(),
        "a confidence band wider than max_confidence_bps must be rejected"
    );
}

/// While the operator has pulled its quotes, nobody can swap; unpausing
/// restores the exact same quote.
#[quasar_test]
fn swap_rejects_when_paused(test: &mut Test) {
    let env = setup(test);
    set_quote(test, OPERATOR, SPREAD_BPS, 1).succeeds();

    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, 825_825_000);
    assert!(
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            825_825_000,
            0
        )
        .is_err(),
        "a paused market must reject swaps"
    );

    set_quote(test, OPERATOR, SPREAD_BPS, 0).succeeds();
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        825_825_000,
        5 * ONE_TOKEN,
    )
    .succeeds();
}

#[quasar_test]
fn swap_rejects_zero_amount(test: &mut Test) {
    let env = setup(test);
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, ONE_TOKEN);
    assert!(
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            0,
            0
        )
        .is_err(),
        "a zero-amount swap must be rejected"
    );
}

/// A buy bigger than the base inventory is rejected whole — a prop AMM never
/// partially fills, and never prices what it cannot deliver.
#[quasar_test]
fn swap_rejects_insufficient_inventory(test: &mut Test) {
    let env = setup(test);
    // 1,100 NVDAx at $165.165 ≈ 181,681.50 USDC — affordable for the trader,
    // but the vault only holds 1,000 NVDAx.
    let quote_in = 181_681_500_000;
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, quote_in);
    assert!(
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            quote_in,
            0
        )
        .is_err(),
        "a swap larger than the inventory must be rejected"
    );
}

#[quasar_test]
fn initialize_market_rejects_zero_spread(test: &mut Test) {
    let (_env, outcome) = base_world(test, 0);
    assert!(outcome.is_err(), "a zero spread must be rejected");
}

#[quasar_test]
fn initialize_market_rejects_full_spread(test: &mut Test) {
    let (_env, outcome) = base_world(test, 10_000);
    assert!(outcome.is_err(), "a 100% spread must be rejected");
}

#[quasar_test]
fn set_quote_rejects_invalid_spread(test: &mut Test) {
    setup(test);
    assert!(set_quote(test, OPERATOR, 0, 0).is_err());
    assert!(set_quote(test, OPERATOR, 10_000, 0).is_err());
}
