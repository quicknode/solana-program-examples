//! quasar-test integration tests: initialize a market, stock inventory, swap
//! against the oracle-anchored quote, and exercise every guard rail (operator
//! gating, slippage, staleness, confidence, pause, inventory limits).

use {
    crate::{
        cpi::{
            CloseMarketInstruction, DepositInventoryInstruction, InitializeMarketInstruction,
            SetQuoteInstruction, SwapInstruction, WithdrawInventoryInstruction,
        },
        instructions::shared::error,
        state::Market,
        BaseVaultPda, QuoteVaultPda,
    },
    quasar_lang::error::QuasarError,
    quasar_test::prelude::*,
};

// The base is NVDAx (tokenized NVIDIA stock), which has 8 decimals; the quote
// is USDC, which has 6. The program reads both from the mints, so nothing in
// the quote math assumes they match.
const NVDAX_DECIMALS: u8 = 8;
const ONE_NVDAX: u64 = 100_000_000;
const USDC_DECIMALS: u8 = 6;
const ONE_USDC: u64 = 1_000_000;

// The walkthrough trade: at $165 with a 10 bps spread the ask is $165.165 and
// the bid $164.835, so 5 NVDAx costs 825.825 USDC and sells for 824.175. Both
// are exact in USDC's six decimals because the ask and bid have only three
// decimal places of a dollar, and 5 is a whole number of NVDAx whatever the
// token's decimals.
const FIVE_NVDAX: u64 = 5 * ONE_NVDAX;
const FIVE_NVDAX_AT_THE_ASK: u64 = 825_825_000; // 825.825 USDC
const FIVE_NVDAX_AT_THE_BID: u64 = 824_175_000; // 824.175 USDC
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
// A program that is not the one the market recorded as its feed's owner.
const OTHER_PROGRAM: Pubkey = Pubkey::new_from_array([13; 32]);

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
/// last_update_slot (u64), confidence (u64), owned by the system program,
/// which the market therefore records as the feed's owning program. The tests
/// own this; production reads a real feed.
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
    test.add(Mint::new(OPERATOR).at(BASE_MINT).decimals(NVDAX_DECIMALS));
    test.add(Mint::new(OPERATOR).at(QUOTE_MINT).decimals(USDC_DECIMALS));
    set_feed(test, dollars(165), 0);
    set_clock(test);
    test.add(
        TokenAccount::new(BASE_MINT, OPERATOR)
            .at(OPERATOR_BASE)
            .amount(10_000 * ONE_NVDAX),
    );
    test.add(
        TokenAccount::new(QUOTE_MINT, OPERATOR)
            .at(OPERATOR_QUOTE)
            .amount(10_000_000 * ONE_USDC),
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
        1_000 * ONE_NVDAX,
        200_000 * ONE_USDC,
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

#[allow(clippy::too_many_arguments)]
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

fn close_market(test: &mut Test, env: &Env, signer: Pubkey) -> Outcome {
    test.send(CloseMarketInstruction {
        operator: signer,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        base_vault: env.base_vault,
        quote_vault: env.quote_vault,
    })
}

/// A plain SPL Token `transfer_checked` (instruction 12) of `amount` minor
/// units from `from_account` (owned by `sender`) straight into `vault`.
/// Nothing in the market program runs: this is a third party donating tokens
/// to a vault, not the operator's `deposit_inventory`.
fn donate_to_vault(
    test: &mut Test,
    sender: Pubkey,
    from_account: Pubkey,
    mint: Pubkey,
    decimals: u8,
    vault: Pubkey,
    amount: u64,
) {
    let before = test.tokens(vault);
    let mut data = vec![12u8];
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(decimals);
    test.send(Instruction {
        program_id: SPL_TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(from_account, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(sender, true),
        ],
        data,
    })
    .succeeds()
    .has_tokens(vault, before + amount);
}

/// Write raw bytes as the feed account, owned by the program the market
/// recorded, so only the layout and value checks can refuse it.
fn set_feed_data(test: &mut Test, data: Vec<u8>) {
    test.set_account(Account::new(FEED, system_program::ID, 1_000_000, data));
}

#[quasar_test]
fn initialize_market_creates_market_and_stocked_vaults(test: &mut Test) {
    let env = setup(test);
    // The market and both vaults were created, and the inventory landed.
    assert!(test.account(env.market).is_some());
    assert_eq!(test.tokens(env.base_vault), 1_000 * ONE_NVDAX);
    assert_eq!(test.tokens(env.quote_vault), 200_000 * ONE_USDC);
    // The market account itself is the token authority of both vaults (the
    // owner field is bytes 32..64 of the SPL Token account layout).
    let vault_owner =
        |address: Pubkey| Pubkey::try_from(&test.account(address).unwrap().data[32..64]).unwrap();
    assert_eq!(vault_owner(env.base_vault), env.market);
    assert_eq!(vault_owner(env.quote_vault), env.market);
}

/// Alice buys 5 NVDAx. At $165 with a 10 bps spread the ask is $165.165, so
/// 5 NVDAx costs exactly 825.825 USDC.
#[quasar_test]
fn swap_buys_base_at_the_ask(test: &mut Test) {
    let env = setup(test);
    let quote_in = FIVE_NVDAX_AT_THE_ASK;
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, quote_in);

    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        quote_in,
        FIVE_NVDAX,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, FIVE_NVDAX)
    .has_tokens(TRADER_QUOTE, 0)
    // Conservation: the vaults moved by exactly the two legs of the fill.
    .has_tokens(env.base_vault, 995 * ONE_NVDAX)
    .has_tokens(env.quote_vault, 200_000 * ONE_USDC + quote_in);
}

/// Bob sells 5 NVDAx. At $165 with a 10 bps spread the bid is $164.835, so
/// he receives exactly 824.175 USDC.
#[quasar_test]
fn swap_sells_base_at_the_bid(test: &mut Test) {
    let env = setup(test);
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, FIVE_NVDAX, 0);

    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_SELL_BASE,
        FIVE_NVDAX,
        FIVE_NVDAX_AT_THE_BID,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, 0)
    .has_tokens(TRADER_QUOTE, FIVE_NVDAX_AT_THE_BID)
    .has_tokens(env.base_vault, 1_005 * ONE_NVDAX)
    .has_tokens(env.quote_vault, 200_000 * ONE_USDC - FIVE_NVDAX_AT_THE_BID);
}

/// A buy immediately followed by a sell of the same 5 NVDAx costs exactly
/// the round-trip spread: 1.65 USDC, all of which stays in the inventory.
#[quasar_test]
fn round_trip_costs_exactly_the_spread(test: &mut Test) {
    let env = setup(test);
    let quote_in = FIVE_NVDAX_AT_THE_ASK;
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
        FIVE_NVDAX,
        0,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, 0)
    // 825.825 in, 824.175 back: the market kept 1.65 USDC.
    .has_tokens(TRADER_QUOTE, quote_in - 1_650_000)
    .has_tokens(env.base_vault, 1_000 * ONE_NVDAX)
    .has_tokens(env.quote_vault, 200_000 * ONE_USDC + 1_650_000);
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
        FIVE_NVDAX,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, FIVE_NVDAX);
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
        FIVE_NVDAX,
    )
    .succeeds()
    .has_tokens(TRADER_BASE, FIVE_NVDAX);
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
        1_000 * ONE_NVDAX,
        200_000 * ONE_USDC,
    )
    .succeeds()
    .has_tokens(env.base_vault, 0)
    .has_tokens(env.quote_vault, 0)
    .has_tokens(OPERATOR_BASE, 10_000 * ONE_NVDAX)
    .has_tokens(OPERATOR_QUOTE, 10_000_000 * ONE_USDC);

    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails_with(error::INSUFFICIENT_INVENTORY);
}

/// Maria withdraws every token, then closes the market. The market account
/// and both vaults are gone, and the three rents she paid at
/// `initialize_market` come back to her to the lamport.
#[quasar_test]
fn close_market_returns_all_three_rents(test: &mut Test) {
    let env = setup(test);
    withdraw_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        1_000 * ONE_NVDAX,
        200_000 * ONE_USDC,
    )
    .succeeds();

    let operator_before = test.lamports(OPERATOR);
    let rents =
        test.lamports(env.market) + test.lamports(env.base_vault) + test.lamports(env.quote_vault);
    assert!(rents > 0);

    close_market(test, &env, OPERATOR)
        .succeeds()
        .is_closed(env.market)
        .is_closed(env.base_vault)
        .is_closed(env.quote_vault)
        .has_lamports(OPERATOR, operator_before + rents);
    // The inventory went back through `withdraw_inventory`, so the operator
    // holds every token it started with.
    assert_eq!(test.tokens(OPERATOR_BASE), 10_000 * ONE_NVDAX);
    assert_eq!(test.tokens(OPERATOR_QUOTE), 10_000_000 * ONE_USDC);
}

/// The market cannot close while either vault holds a single minor unit: the
/// operator withdraws first. Each vault's check is exercised on its own.
#[quasar_test]
fn close_market_refuses_while_a_vault_holds_tokens(test: &mut Test) {
    let env = setup(test);
    close_market(test, &env, OPERATOR).fails_with(error::INVENTORY_NOT_EMPTY);

    // Base vault empty, quote vault still stocked.
    withdraw_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        1_000 * ONE_NVDAX,
        0,
    )
    .succeeds();
    close_market(test, &env, OPERATOR).fails_with(error::INVENTORY_NOT_EMPTY);

    // Quote vault empty, one minor unit of base back in the base vault.
    withdraw_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        0,
        200_000 * ONE_USDC,
    )
    .succeeds();
    deposit_inventory(test, &env, OPERATOR, OPERATOR_BASE, OPERATOR_QUOTE, 1, 0).succeeds();
    close_market(test, &env, OPERATOR).fails_with(error::INVENTORY_NOT_EMPTY);
    assert!(test.account(env.market).is_some());

    withdraw_inventory(test, &env, OPERATOR, OPERATOR_BASE, OPERATOR_QUOTE, 1, 0).succeeds();
    close_market(test, &env, OPERATOR)
        .succeeds()
        .is_closed(env.market);
}

/// Nobody can wedge the close or slip tokens past it by sending them straight
/// to a vault. After Maria withdraws everything, a stranger sends one minor
/// unit of NVDAx into the base vault with a plain token transfer, not
/// `deposit_inventory`, and the close is refused; then one minor unit of USDC
/// into the quote vault, and the close is refused again. Maria withdraws each
/// donation like any other inventory and the market closes.
#[quasar_test]
fn close_market_refuses_tokens_sent_straight_to_a_vault(test: &mut Test) {
    let env = setup(test);
    withdraw_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        1_000 * ONE_NVDAX,
        200_000 * ONE_USDC,
    )
    .succeeds();
    fund_trader(test, MALLORY, MALLORY_BASE, MALLORY_QUOTE, 1, 1);

    donate_to_vault(
        test,
        MALLORY,
        MALLORY_BASE,
        BASE_MINT,
        NVDAX_DECIMALS,
        env.base_vault,
        1,
    );
    close_market(test, &env, OPERATOR).fails_with(error::INVENTORY_NOT_EMPTY);
    assert!(test.account(env.market).is_some());
    withdraw_inventory(test, &env, OPERATOR, OPERATOR_BASE, OPERATOR_QUOTE, 1, 0).succeeds();

    donate_to_vault(
        test,
        MALLORY,
        MALLORY_QUOTE,
        QUOTE_MINT,
        USDC_DECIMALS,
        env.quote_vault,
        1,
    );
    close_market(test, &env, OPERATOR).fails_with(error::INVENTORY_NOT_EMPTY);
    assert!(test.account(env.market).is_some());
    withdraw_inventory(test, &env, OPERATOR, OPERATOR_BASE, OPERATOR_QUOTE, 0, 1).succeeds();

    close_market(test, &env, OPERATOR)
        .succeeds()
        .is_closed(env.market);
    // The operator now holds its own inventory plus both donated units.
    assert_eq!(test.tokens(OPERATOR_BASE), 10_000 * ONE_NVDAX + 1);
    assert_eq!(test.tokens(OPERATOR_QUOTE), 10_000_000 * ONE_USDC + 1);
}

/// A closed market cannot fill. Its account is gone, so a swap naming it
/// fails before any token moves, and the trader keeps every token.
#[quasar_test]
fn swap_against_a_closed_market_fails(test: &mut Test) {
    let env = setup(test);
    withdraw_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        1_000 * ONE_NVDAX,
        200_000 * ONE_USDC,
    )
    .succeeds();
    close_market(test, &env, OPERATOR)
        .succeeds()
        .is_closed(env.market);

    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );
    // The closed market's address is an empty system account, so the
    // runtime's owner check refuses it as `IllegalOwner` before the handler
    // runs.
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails(ProgramError::Runtime("IllegalOwner".into()));
    assert_eq!(test.tokens(TRADER_BASE), 0);
    assert_eq!(test.tokens(TRADER_QUOTE), FIVE_NVDAX_AT_THE_ASK);
}

#[quasar_test]
fn close_market_rejects_non_operator(test: &mut Test) {
    let env = setup(test);
    withdraw_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        1_000 * ONE_NVDAX,
        200_000 * ONE_USDC,
    )
    .succeeds();
    test.add(Wallet::new().at(MALLORY));
    close_market(test, &env, MALLORY).fails_with(QuasarError::HasOneMismatch);
    assert!(test.account(env.market).is_some());
}

#[quasar_test]
fn withdraw_more_than_inventory_fails(test: &mut Test) {
    let env = setup(test);
    withdraw_inventory(
        test,
        &env,
        OPERATOR,
        OPERATOR_BASE,
        OPERATOR_QUOTE,
        1_001 * ONE_NVDAX,
        0,
    )
    .fails_with(error::INSUFFICIENT_INVENTORY);
}

/// `has_one(operator)` on the market is the whole access control, so an
/// imposter is refused by Quasar's constraint check before the handler runs.
#[quasar_test]
fn deposit_rejects_non_operator(test: &mut Test) {
    let env = setup(test);
    fund_trader(
        test,
        MALLORY,
        MALLORY_BASE,
        MALLORY_QUOTE,
        ONE_NVDAX,
        ONE_USDC,
    );
    deposit_inventory(
        test,
        &env,
        MALLORY,
        MALLORY_BASE,
        MALLORY_QUOTE,
        ONE_NVDAX,
        0,
    )
    .fails_with(QuasarError::HasOneMismatch);
}

#[quasar_test]
fn withdraw_rejects_non_operator(test: &mut Test) {
    let env = setup(test);
    fund_trader(test, MALLORY, MALLORY_BASE, MALLORY_QUOTE, 0, 0);
    withdraw_inventory(
        test,
        &env,
        MALLORY,
        MALLORY_BASE,
        MALLORY_QUOTE,
        ONE_NVDAX,
        0,
    )
    .fails_with(QuasarError::HasOneMismatch);
}

#[quasar_test]
fn set_quote_rejects_non_operator(test: &mut Test) {
    setup(test);
    test.add(Wallet::new().at(MALLORY));
    set_quote(test, MALLORY, 500, 1).fails_with(QuasarError::HasOneMismatch);
}

/// A fill below the caller's minimum is rejected, not filled worse.
#[quasar_test]
fn swap_rejects_slippage(test: &mut Test) {
    let env = setup(test);
    let quote_in = FIVE_NVDAX_AT_THE_ASK;
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, quote_in);
    // The fill would be exactly 5 NVDAx; demand one minor unit more.
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        quote_in,
        FIVE_NVDAX + 1,
    )
    .fails_with(error::SLIPPAGE_EXCEEDED);
}

/// An oracle price older than the staleness bound cannot be traded against:
/// a lagging quote is a free option for arbitrageurs.
#[quasar_test]
fn swap_rejects_stale_price(test: &mut Test) {
    let env = setup(test);
    make_price_stale(test);
    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails_with(error::STALE_PRICE);
}

/// A cluster restart passes hours of wall-clock time in zero slots, so a
/// price published before the halt can still look fresh by slot count. The
/// market must refuse to quote against it until the publisher posts again.
#[quasar_test]
fn swap_rejects_price_from_before_a_restart(test: &mut Test) {
    let env = setup(test);
    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK * 2,
    );

    // The feed is stamped at `SLOT - 5`: fresh by the 150-slot staleness
    // bound, but published before a restart at `SLOT - 3`, so only the
    // restart check can catch it.
    set_feed_at_slot(test, dollars(165), SLOT - 5, 0);
    set_last_restart_slot(test, SLOT - 3);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails_with(error::PRICE_PREDATES_RESTART);

    // Publishing after the restart (at `SLOT`) reopens the market.
    set_feed(test, dollars(165), 0);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .succeeds();
}

/// The market records the program that owns its feed at creation and refuses
/// a price from a feed account owned by any other program, however well its
/// bytes decode. The feed is swapped for a byte-identical copy owned by an
/// unrelated program, and the refusal is by the owner alone: the same bytes
/// owned by the recorded program again are accepted.
#[quasar_test]
fn swap_rejects_price_feed_from_another_program(test: &mut Test) {
    let env = setup(test);
    assert_eq!(
        test.read::<Market>(env.market).price_feed_program,
        system_program::ID
    );
    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );

    set_feed_owned_by(test, OTHER_PROGRAM, dollars(165), SLOT, 0);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails_with(error::PRICE_FEED_NOT_FROM_ORACLE);

    set_feed(test, dollars(165), 0);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
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
    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails_with(error::ORACLE_CONFIDENCE_TOO_WIDE);
}

/// A zero or negative oracle price is not a price. The market refuses to
/// quote against either rather than divide by it or flip the spread.
#[quasar_test]
fn swap_rejects_non_positive_price(test: &mut Test) {
    let env = setup(test);
    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );
    for price in [0, -dollars(165)] {
        set_feed(test, price, 0);
        swap(
            test,
            &env,
            TRADER,
            TRADER_BASE,
            TRADER_QUOTE,
            DIRECTION_BUY_BASE,
            FIVE_NVDAX_AT_THE_ASK,
            0,
        )
        .fails_with(error::NON_POSITIVE_PRICE);
    }
}

/// A market pinned to a feed with 8 decimals of scale refuses a feed that
/// reports 6: read at the wrong scale, $165 would be $1.65.
#[quasar_test]
fn swap_rejects_oracle_scale_mismatch(test: &mut Test) {
    let env = setup(test);
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&(165i128 * 10i128.pow(6)).to_le_bytes());
    data.extend_from_slice(&(ORACLE_SCALE - 2).to_le_bytes());
    data.extend_from_slice(&SLOT.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    set_feed_data(test, data);
    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails_with(error::ORACLE_SCALE_MISMATCH);
}

/// A feed account owned by the recorded oracle program but too short to hold
/// the price layout is refused before a byte of it is decoded.
#[quasar_test]
fn swap_rejects_oracle_data_too_short(test: &mut Test) {
    let env = setup(test);
    // The layout needs 36 bytes; keep the price and the scale.
    let mut data = Vec::with_capacity(20);
    data.extend_from_slice(&dollars(165).to_le_bytes());
    data.extend_from_slice(&ORACLE_SCALE.to_le_bytes());
    set_feed_data(test, data);
    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails_with(error::ORACLE_DATA_TOO_SHORT);
}

/// One minor unit of USDC (0.000001) at the $165.165 ask buys 0.0000000060546
/// NVDAx, which floors to zero minor units. The market refuses rather than
/// take the trader's input for nothing.
#[quasar_test]
fn swap_rejects_amount_that_rounds_to_zero(test: &mut Test) {
    let env = setup(test);
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, 1);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        1,
        0,
    )
    .fails_with(error::AMOUNT_ROUNDS_TO_ZERO);
    assert_eq!(test.tokens(TRADER_QUOTE), 1);
}

/// While the operator has pulled its quotes, nobody can swap; unpausing
/// restores the exact same quote.
#[quasar_test]
fn swap_rejects_when_paused(test: &mut Test) {
    let env = setup(test);
    set_quote(test, OPERATOR, SPREAD_BPS, 1).succeeds();

    fund_trader(
        test,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        0,
        FIVE_NVDAX_AT_THE_ASK,
    );
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        0,
    )
    .fails_with(error::MARKET_PAUSED);

    set_quote(test, OPERATOR, SPREAD_BPS, 0).succeeds();
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        FIVE_NVDAX_AT_THE_ASK,
        FIVE_NVDAX,
    )
    .succeeds();
}

#[quasar_test]
fn swap_rejects_zero_amount(test: &mut Test) {
    let env = setup(test);
    fund_trader(test, TRADER, TRADER_BASE, TRADER_QUOTE, 0, ONE_USDC);
    swap(
        test,
        &env,
        TRADER,
        TRADER_BASE,
        TRADER_QUOTE,
        DIRECTION_BUY_BASE,
        0,
        0,
    )
    .fails_with(error::ZERO_AMOUNT);
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
    .fails_with(error::INSUFFICIENT_INVENTORY);
}

#[quasar_test]
fn initialize_market_rejects_zero_spread(test: &mut Test) {
    let (_env, outcome) = base_world(test, 0);
    outcome.fails_with(error::INVALID_PARAMETER);
}

#[quasar_test]
fn initialize_market_rejects_full_spread(test: &mut Test) {
    let (_env, outcome) = base_world(test, 10_000);
    outcome.fails_with(error::INVALID_PARAMETER);
}

#[quasar_test]
fn set_quote_rejects_invalid_spread(test: &mut Test) {
    setup(test);
    set_quote(test, OPERATOR, 0, 0).fails_with(error::INVALID_PARAMETER);
    set_quote(test, OPERATOR, 10_000, 0).fails_with(error::INVALID_PARAMETER);
}
