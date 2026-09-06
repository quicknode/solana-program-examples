//! quasar-test integration tests: open a venue, write a covered call and a
//! cash-secured put, buy them, exercise or let them expire, collect, and
//! prove every gate shuts (the expiry boundary from both sides, status
//! checks, and who may call what).

use {
    crate::{
        constants::{KIND_CALL, KIND_PUT, STATUS_EXERCISED, STATUS_HELD, STATUS_LISTED},
        cpi::{
            BuyOptionInstruction, CancelOptionInstruction, CollectFeesInstruction,
            CollectProceedsInstruction, ExerciseOptionInstruction, InitializeMarketInstruction,
            ReclaimCollateralInstruction, WriteOptionInstruction,
        },
        errors::OptionsError,
        state::{Market, OptionContract, QuoteVaultPda, UnderlyingVaultPda},
    },
    quasar_test::prelude::*,
};

// Both tokens have 6 decimals: the underlying is NVDAx (tokenized NVIDIA
// stock) and the quote is USDC.
const ONE_TOKEN: u64 = 1_000_000;
// The venue charges 1% of every premium.
const FEE_BPS: u16 = 100;

// The walkthrough's call: 5 contracts, each on 1 NVDAx, strike 180 USDC,
// asking 25 USDC for the option. And the put: strike 150 USDC, asking 20 USDC.
const CONTRACTS: u64 = 5;
const ONE_NVDAX_PER_CONTRACT: u64 = ONE_TOKEN;
const CALL_STRIKE: u64 = 180 * ONE_TOKEN;
const CALL_PREMIUM: u64 = 25 * ONE_TOKEN;
const PUT_STRIKE: u64 = 150 * ONE_TOKEN;
const PUT_PREMIUM: u64 = 20 * ONE_TOKEN;
const CALL_ID: u64 = 1;
const PUT_ID: u64 = 2;

// Every character starts with the standard wallet of 1,000 USDC; the story
// hands the writers 5 NVDAx.
const STANDARD_USDC: u64 = 1_000 * ONE_TOKEN;
const FIVE_NVDAX: u64 = 5 * ONE_TOKEN;

// A fixed unix timestamp the clock is warped to before anything is written,
// so the expiry a week later is deterministic.
const START_TIME: i64 = 1_750_000_000;
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;
const EXPIRY: i64 = START_TIME + 7 * SECONDS_PER_DAY;

// Deterministic addresses.
const MARIA: Pubkey = Pubkey::new_from_array([1; 32]);
const NVDAX_MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const USDC_MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const MARIA_USDC: Pubkey = Pubkey::new_from_array([4; 32]);
const ALICE: Pubkey = Pubkey::new_from_array([5; 32]);
const ALICE_NVDAX: Pubkey = Pubkey::new_from_array([6; 32]);
const ALICE_USDC: Pubkey = Pubkey::new_from_array([7; 32]);
const BOB: Pubkey = Pubkey::new_from_array([8; 32]);
const BOB_NVDAX: Pubkey = Pubkey::new_from_array([9; 32]);
const BOB_USDC: Pubkey = Pubkey::new_from_array([10; 32]);
const CAROL: Pubkey = Pubkey::new_from_array([11; 32]);
const CAROL_NVDAX: Pubkey = Pubkey::new_from_array([12; 32]);
const CAROL_USDC: Pubkey = Pubkey::new_from_array([13; 32]);
const DAVE: Pubkey = Pubkey::new_from_array([14; 32]);
const DAVE_NVDAX: Pubkey = Pubkey::new_from_array([15; 32]);
const DAVE_USDC: Pubkey = Pubkey::new_from_array([16; 32]);
const MALLORY: Pubkey = Pubkey::new_from_array([17; 32]);
const MALLORY_NVDAX: Pubkey = Pubkey::new_from_array([18; 32]);
const MALLORY_USDC: Pubkey = Pubkey::new_from_array([19; 32]);
// A second USDC account of Alice's, for the self-purchase test.
const ALICE_OTHER_USDC: Pubkey = Pubkey::new_from_array([20; 32]);

struct Env {
    market: Pubkey,
    underlying_vault: Pubkey,
    quote_vault: Pubkey,
}

/// A character with a wallet and both token accounts.
struct Person {
    wallet: Pubkey,
    nvdax: Pubkey,
    usdc: Pubkey,
}

const fn person(wallet: Pubkey, nvdax: Pubkey, usdc: Pubkey) -> Person {
    Person {
        wallet,
        nvdax,
        usdc,
    }
}

const ALICE_P: Person = person(ALICE, ALICE_NVDAX, ALICE_USDC);
const BOB_P: Person = person(BOB, BOB_NVDAX, BOB_USDC);
const CAROL_P: Person = person(CAROL, CAROL_NVDAX, CAROL_USDC);
const DAVE_P: Person = person(DAVE, DAVE_NVDAX, DAVE_USDC);
const MALLORY_P: Person = person(MALLORY, MALLORY_NVDAX, MALLORY_USDC);

fn add_person(test: &mut Test, who: &Person, nvdax: u64, usdc: u64) {
    test.add(Wallet::new().at(who.wallet));
    test.add(
        TokenAccount::new(NVDAX_MINT, who.wallet)
            .at(who.nvdax)
            .amount(nvdax),
    );
    test.add(
        TokenAccount::new(USDC_MINT, who.wallet)
            .at(who.usdc)
            .amount(usdc),
    );
}

fn initialize_market(test: &mut Test, fee_bps: u16, quote_mint: Pubkey) -> Outcome {
    test.send(InitializeMarketInstruction {
        admin: MARIA,
        underlying_mint: NVDAX_MINT,
        quote_mint,
        fee_bps,
    })
}

/// Mints, the clock at `START_TIME`, Maria's wallet, and a venue at
/// `FEE_BPS`. Alice and Dave hold 5 NVDAx; everyone holds 1,000 USDC.
fn setup_with_fee(test: &mut Test, fee_bps: u16) -> Env {
    test.add(Wallet::new().at(MARIA));
    test.add(TokenAccount::new(USDC_MINT, MARIA).at(MARIA_USDC).amount(0));
    test.add(Mint::new(MARIA).at(NVDAX_MINT).decimals(6));
    test.add(Mint::new(MARIA).at(USDC_MINT).decimals(6));
    test.warp_to_timestamp(START_TIME);
    add_person(test, &ALICE_P, FIVE_NVDAX, STANDARD_USDC);
    add_person(test, &BOB_P, 0, STANDARD_USDC);
    add_person(test, &CAROL_P, 0, STANDARD_USDC);
    add_person(test, &DAVE_P, FIVE_NVDAX, STANDARD_USDC);
    add_person(test, &MALLORY_P, 0, STANDARD_USDC);
    initialize_market(test, fee_bps, USDC_MINT).succeeds();
    let market = test.derive_pda(Market::seeds(&NVDAX_MINT, &USDC_MINT));
    Env {
        market,
        underlying_vault: test.derive_pda(UnderlyingVaultPda::seeds(&market)),
        quote_vault: test.derive_pda(QuoteVaultPda::seeds(&market)),
    }
}

fn setup(test: &mut Test) -> Env {
    setup_with_fee(test, FEE_BPS)
}

#[allow(clippy::too_many_arguments)]
fn write_option(
    test: &mut Test,
    env: &Env,
    writer: &Person,
    id: u64,
    kind: u8,
    contracts: u64,
    underlying_per_contract: u64,
    strike_per_contract: u64,
    premium: u64,
    expiry: i64,
) -> Outcome {
    test.send(WriteOptionInstruction {
        writer: writer.wallet,
        underlying_mint: NVDAX_MINT,
        quote_mint: USDC_MINT,
        underlying_vault: env.underlying_vault,
        quote_vault: env.quote_vault,
        writer_underlying: writer.nvdax,
        writer_quote: writer.usdc,
        id,
        kind,
        contracts,
        underlying_per_contract,
        strike_per_contract,
        premium,
        expiry,
    })
}

fn option_pda(test: &Test, env: &Env, writer: &Person, id: u64) -> Pubkey {
    test.derive_pda(OptionContract::seeds(&env.market, &writer.wallet, id))
}

/// Alice's call, written and listed.
fn write_call(test: &mut Test, env: &Env) -> Pubkey {
    write_option(
        test,
        env,
        &ALICE_P,
        CALL_ID,
        KIND_CALL,
        CONTRACTS,
        ONE_NVDAX_PER_CONTRACT,
        CALL_STRIKE,
        CALL_PREMIUM,
        EXPIRY,
    )
    .succeeds();
    option_pda(test, env, &ALICE_P, CALL_ID)
}

/// Carol's put, written and listed.
fn write_put(test: &mut Test, env: &Env) -> Pubkey {
    write_option(
        test,
        env,
        &CAROL_P,
        PUT_ID,
        KIND_PUT,
        CONTRACTS,
        ONE_NVDAX_PER_CONTRACT,
        PUT_STRIKE,
        PUT_PREMIUM,
        EXPIRY,
    )
    .succeeds();
    option_pda(test, env, &CAROL_P, PUT_ID)
}

fn buy_option(test: &mut Test, env: &Env, buyer: &Person, writer: &Person, id: u64) -> Outcome {
    test.send(BuyOptionInstruction {
        buyer: buyer.wallet,
        writer: writer.wallet,
        option_id_seed: id,
        underlying_mint: NVDAX_MINT,
        quote_mint: USDC_MINT,
        quote_vault: env.quote_vault,
        buyer_quote: buyer.usdc,
        writer_quote: writer.usdc,
    })
}

fn cancel_option(test: &mut Test, env: &Env, writer: &Person, id: u64) -> Outcome {
    test.send(CancelOptionInstruction {
        writer: writer.wallet,
        option_id_seed: id,
        underlying_mint: NVDAX_MINT,
        quote_mint: USDC_MINT,
        underlying_vault: env.underlying_vault,
        quote_vault: env.quote_vault,
        writer_underlying: writer.nvdax,
        writer_quote: writer.usdc,
    })
}

fn exercise_option(
    test: &mut Test,
    env: &Env,
    holder: &Person,
    writer: &Person,
    id: u64,
) -> Outcome {
    test.send(ExerciseOptionInstruction {
        holder: holder.wallet,
        writer: writer.wallet,
        option_id_seed: id,
        underlying_mint: NVDAX_MINT,
        quote_mint: USDC_MINT,
        underlying_vault: env.underlying_vault,
        quote_vault: env.quote_vault,
        holder_underlying: holder.nvdax,
        holder_quote: holder.usdc,
    })
}

fn collect_proceeds(test: &mut Test, env: &Env, writer: &Person, id: u64) -> Outcome {
    test.send(CollectProceedsInstruction {
        writer: writer.wallet,
        option_id_seed: id,
        underlying_mint: NVDAX_MINT,
        quote_mint: USDC_MINT,
        underlying_vault: env.underlying_vault,
        quote_vault: env.quote_vault,
        writer_underlying: writer.nvdax,
        writer_quote: writer.usdc,
    })
}

fn reclaim_collateral(test: &mut Test, env: &Env, writer: &Person, id: u64) -> Outcome {
    test.send(ReclaimCollateralInstruction {
        writer: writer.wallet,
        option_id_seed: id,
        underlying_mint: NVDAX_MINT,
        quote_mint: USDC_MINT,
        underlying_vault: env.underlying_vault,
        quote_vault: env.quote_vault,
        writer_underlying: writer.nvdax,
        writer_quote: writer.usdc,
    })
}

fn collect_fees(test: &mut Test, env: &Env, admin: Pubkey, admin_quote: Pubkey) -> Outcome {
    test.send(CollectFeesInstruction {
        admin,
        underlying_mint: NVDAX_MINT,
        quote_mint: USDC_MINT,
        underlying_vault: env.underlying_vault,
        quote_vault: env.quote_vault,
        admin_quote,
    })
}

/// The custody invariant: each vault holds exactly what the market owes.
fn assert_vaults_match_ledger(test: &Test, env: &Env) {
    let market = test.read::<Market>(env.market);
    assert_eq!(
        test.tokens(env.underlying_vault),
        u64::from(market.underlying_locked),
        "underlying vault must hold exactly the locked underlying"
    );
    assert_eq!(
        test.tokens(env.quote_vault),
        u64::from(market.quote_locked) + u64::from(market.fees_owed),
        "quote vault must hold exactly the locked quote plus the fees owed"
    );
}

// ===========================================================================
// The call: write, buy, exercise, collect
// ===========================================================================

/// Alice writes 5 covered calls on her 5 NVDAx. The whole 5 NVDAx moves into
/// the vault at once; the option is listed for a 25 USDC premium.
#[quasar_test]
fn write_call_locks_the_underlying(test: &mut Test) {
    let env = setup(test);
    let option = write_call(test, &env);

    assert_eq!(test.tokens(ALICE_NVDAX), 0);
    assert_eq!(test.tokens(env.underlying_vault), FIVE_NVDAX);
    let state = test.read::<OptionContract>(option);
    assert_eq!(state.writer, ALICE);
    assert_eq!(state.holder, Pubkey::default());
    assert_eq!(state.kind, KIND_CALL);
    assert_eq!(state.status, STATUS_LISTED);
    assert_eq!(u64::from(state.contracts), CONTRACTS);
    assert_eq!(u64::from(state.strike_per_contract), CALL_STRIKE);
    assert_eq!(u64::from(state.premium), CALL_PREMIUM);
    assert_eq!(i64::from(state.expiry), EXPIRY);
    assert_eq!(
        u64::from(test.read::<Market>(env.market).underlying_locked),
        FIVE_NVDAX
    );
    assert_vaults_match_ledger(test, &env);
}

/// Bob buys the option. He pays 25 USDC: 1% (0.25 USDC) to the venue, the rest
/// straight to Alice. The 5 NVDAx do not move.
#[quasar_test]
fn buy_option_pays_the_premium_minus_the_fee(test: &mut Test) {
    let env = setup(test);
    let option = write_call(test, &env);

    let fee = 250_000; // 0.25 USDC
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID)
        .succeeds()
        .has_tokens(BOB_USDC, STANDARD_USDC - CALL_PREMIUM)
        .has_tokens(ALICE_USDC, STANDARD_USDC + CALL_PREMIUM - fee)
        .has_tokens(env.quote_vault, fee);
    // The underlying vault is not part of a buy, and did not move.
    assert_eq!(test.tokens(env.underlying_vault), FIVE_NVDAX);

    let state = test.read::<OptionContract>(option);
    assert_eq!(state.holder, BOB);
    assert_eq!(state.status, STATUS_HELD);
    assert_eq!(u64::from(test.read::<Market>(env.market).fees_owed), fee);
    assert_vaults_match_ledger(test, &env);
}

/// NVIDIA rallies past the strike offchain, so Bob exercises: he pays the
/// strike, 5 x 180 = 900 USDC, into the vault and takes the 5 NVDAx.
#[quasar_test]
fn exercise_call_swaps_the_strike_for_the_underlying(test: &mut Test) {
    let env = setup(test);
    let option = write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();

    let strike_total = 900 * ONE_TOKEN;
    exercise_option(test, &env, &BOB_P, &ALICE_P, CALL_ID)
        .succeeds()
        .has_tokens(BOB_NVDAX, FIVE_NVDAX)
        .has_tokens(BOB_USDC, STANDARD_USDC - CALL_PREMIUM - strike_total)
        .has_tokens(env.underlying_vault, 0)
        .has_tokens(env.quote_vault, strike_total + 250_000);

    let market = test.read::<Market>(env.market);
    assert_eq!(u64::from(market.underlying_locked), 0);
    assert_eq!(u64::from(market.quote_locked), strike_total);
    assert_eq!(test.read::<OptionContract>(option).status, STATUS_EXERCISED);
    assert_vaults_match_ledger(test, &env);
}

/// Alice collects the 900 USDC Bob paid, and the option closes.
#[quasar_test]
fn collect_proceeds_pays_the_writer_and_closes_the_option(test: &mut Test) {
    let env = setup(test);
    let option = write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    exercise_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();

    collect_proceeds(test, &env, &ALICE_P, CALL_ID)
        .succeeds()
        .has_tokens(
            ALICE_USDC,
            STANDARD_USDC + 900 * ONE_TOKEN + CALL_PREMIUM - 250_000,
        )
        .has_tokens(ALICE_NVDAX, 0)
        .is_closed(option);

    let market = test.read::<Market>(env.market);
    assert_eq!(u64::from(market.quote_locked), 0);
    assert_eq!(u64::from(market.fees_owed), 250_000);
    assert_vaults_match_ledger(test, &env);
}

/// Maria sweeps the venue's fee. Only the 0.25 USDC of fees leaves the
/// vault; the strike payment sitting beside it stays locked to Alice.
#[quasar_test]
fn collect_fees_pays_only_the_fees_owed(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    exercise_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();

    collect_fees(test, &env, MARIA, MARIA_USDC)
        .succeeds()
        .has_tokens(MARIA_USDC, 250_000)
        .has_tokens(env.quote_vault, 900 * ONE_TOKEN);
    assert_eq!(u64::from(test.read::<Market>(env.market).fees_owed), 0);
    assert_vaults_match_ledger(test, &env);

    // Nothing left to sweep.
    collect_fees(test, &env, MARIA, MARIA_USDC).fails_with(OptionsError::NothingToCollect);
}

// ===========================================================================
// The put, and the option that expires unexercised
// ===========================================================================

/// Carol writes 5 cash-secured puts at a 150 strike: 750 USDC of collateral.
/// Dave buys them for 20 USDC, then delivers his 5 NVDAx for the 750 USDC.
#[quasar_test]
fn put_lifecycle_delivers_the_underlying_for_the_strike(test: &mut Test) {
    let env = setup(test);
    let option = write_put(test, &env);
    let collateral = 750 * ONE_TOKEN;
    assert_eq!(test.tokens(CAROL_USDC), STANDARD_USDC - collateral);
    assert_eq!(test.tokens(env.quote_vault), collateral);
    assert_vaults_match_ledger(test, &env);

    let fee = 200_000; // 1% of 20 USDC
    buy_option(test, &env, &DAVE_P, &CAROL_P, PUT_ID)
        .succeeds()
        .has_tokens(CAROL_USDC, STANDARD_USDC - collateral + PUT_PREMIUM - fee)
        .has_tokens(DAVE_USDC, STANDARD_USDC - PUT_PREMIUM);
    assert_vaults_match_ledger(test, &env);

    exercise_option(test, &env, &DAVE_P, &CAROL_P, PUT_ID)
        .succeeds()
        .has_tokens(DAVE_NVDAX, 0)
        .has_tokens(DAVE_USDC, STANDARD_USDC - PUT_PREMIUM + collateral)
        .has_tokens(env.underlying_vault, FIVE_NVDAX)
        .has_tokens(env.quote_vault, fee);
    let market = test.read::<Market>(env.market);
    assert_eq!(u64::from(market.underlying_locked), FIVE_NVDAX);
    assert_eq!(u64::from(market.quote_locked), 0);
    assert_vaults_match_ledger(test, &env);

    collect_proceeds(test, &env, &CAROL_P, PUT_ID)
        .succeeds()
        .has_tokens(CAROL_NVDAX, FIVE_NVDAX)
        .is_closed(option);
    assert_eq!(
        u64::from(test.read::<Market>(env.market).underlying_locked),
        0
    );
    assert_vaults_match_ledger(test, &env);
}

/// Bob never exercises. Once the expiry passes, Alice takes her 5 NVDAx back
/// and keeps the premium.
#[quasar_test]
fn reclaim_collateral_after_expiry_returns_it_to_the_writer(test: &mut Test) {
    let env = setup(test);
    let option = write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();

    test.warp_to_timestamp(EXPIRY);
    reclaim_collateral(test, &env, &ALICE_P, CALL_ID)
        .succeeds()
        .has_tokens(ALICE_NVDAX, FIVE_NVDAX)
        .has_tokens(ALICE_USDC, STANDARD_USDC + CALL_PREMIUM - 250_000)
        .is_closed(option);
    // Bob is not part of the reclaim: he is left with nothing to claim.
    assert_eq!(test.tokens(BOB_USDC), STANDARD_USDC - CALL_PREMIUM);
    assert_eq!(
        u64::from(test.read::<Market>(env.market).underlying_locked),
        0
    );
    assert_vaults_match_ledger(test, &env);
}

// ===========================================================================
// The expiry boundary, from both sides
// ===========================================================================

/// The holder may exercise while now < expiry: one second before, yes; at
/// expiry, no.
#[quasar_test]
fn exercise_is_allowed_up_to_but_not_at_expiry(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();

    test.warp_to_timestamp(EXPIRY);
    exercise_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).fails_with(OptionsError::OptionExpired);

    test.warp_to_timestamp(EXPIRY - 1);
    exercise_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
}

/// The writer may reclaim once now >= expiry, and not one second earlier.
#[quasar_test]
fn reclaim_is_refused_before_expiry(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();

    test.warp_to_timestamp(EXPIRY - 1);
    reclaim_collateral(test, &env, &ALICE_P, CALL_ID).fails_with(OptionsError::OptionNotExpired);

    test.warp_to_timestamp(EXPIRY);
    reclaim_collateral(test, &env, &ALICE_P, CALL_ID).succeeds();
}

/// An expired option cannot be bought.
#[quasar_test]
fn buy_is_refused_after_expiry(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    test.warp_to_timestamp(EXPIRY);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).fails_with(OptionsError::OptionExpired);
}

// ===========================================================================
// Cancel: the writer's exit from an unsold option
// ===========================================================================

#[quasar_test]
fn cancel_unsold_option_returns_the_collateral(test: &mut Test) {
    let env = setup(test);
    let option = write_call(test, &env);

    cancel_option(test, &env, &ALICE_P, CALL_ID)
        .succeeds()
        .has_tokens(ALICE_NVDAX, FIVE_NVDAX)
        .has_tokens(env.underlying_vault, 0)
        .is_closed(option);
    assert_vaults_match_ledger(test, &env);
}

/// An unsold option that expired is still the writer's to cancel.
#[quasar_test]
fn cancel_unsold_option_works_after_expiry(test: &mut Test) {
    let env = setup(test);
    write_put(test, &env);
    test.warp_to_timestamp(EXPIRY + SECONDS_PER_DAY);
    cancel_option(test, &env, &CAROL_P, PUT_ID)
        .succeeds()
        .has_tokens(CAROL_USDC, STANDARD_USDC);
    assert_eq!(u64::from(test.read::<Market>(env.market).quote_locked), 0);
    assert_vaults_match_ledger(test, &env);
}

/// Once sold, the collateral belongs to the deal.
#[quasar_test]
fn cancel_is_refused_once_sold(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    cancel_option(test, &env, &ALICE_P, CALL_ID).fails_with(OptionsError::OptionNotListed);
    assert_eq!(test.tokens(env.underlying_vault), FIVE_NVDAX);
}

// ===========================================================================
// Who may do what
// ===========================================================================

#[quasar_test]
fn buy_is_refused_once_sold(test: &mut Test) {
    let env = setup(test);
    let option = write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    buy_option(test, &env, &CAROL_P, &ALICE_P, CALL_ID).fails_with(OptionsError::OptionNotListed);
    assert_eq!(test.read::<OptionContract>(option).holder, BOB);
}

/// A writer cannot buy their own option: their address would sit in the `buyer`
/// and `writer` slots at once, which the runtime refuses before the handler
/// runs, whichever of their token accounts the premium is paid from.
#[quasar_test]
fn writer_cannot_buy_their_own_option(test: &mut Test) {
    let env = setup(test);
    let option = write_call(test, &env);
    assert!(buy_option(test, &env, &ALICE_P, &ALICE_P, CALL_ID).is_err());

    test.add(
        TokenAccount::new(USDC_MINT, ALICE)
            .at(ALICE_OTHER_USDC)
            .amount(STANDARD_USDC),
    );
    let alice_from_other_account = person(ALICE, ALICE_NVDAX, ALICE_OTHER_USDC);
    assert!(buy_option(test, &env, &alice_from_other_account, &ALICE_P, CALL_ID).is_err());
    assert_eq!(test.read::<OptionContract>(option).status, STATUS_LISTED);
    assert_eq!(test.tokens(ALICE_OTHER_USDC), STANDARD_USDC);
}

/// A buyer cannot route the premium to their own account by passing it as
/// the writer's.
#[quasar_test]
fn buy_refuses_a_premium_account_the_writer_does_not_own(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    let mut instruction: Instruction = BuyOptionInstruction {
        buyer: BOB,
        writer: ALICE,
        option_id_seed: CALL_ID,
        underlying_mint: NVDAX_MINT,
        quote_mint: USDC_MINT,
        quote_vault: env.quote_vault,
        buyer_quote: BOB_USDC,
        writer_quote: ALICE_USDC,
    }
    .into();
    // Account order matches the `#[derive(Accounts)]` struct: writer_quote is
    // the last account before the token program.
    let writer_quote_index = instruction
        .accounts
        .iter()
        .position(|meta| meta.pubkey == ALICE_USDC)
        .unwrap();
    instruction.accounts[writer_quote_index].pubkey = MALLORY_USDC;
    test.send(instruction)
        .fails_with(OptionsError::InvalidParameter);
    assert_eq!(test.tokens(MALLORY_USDC), STANDARD_USDC);
}

/// Only the holder can exercise.
#[quasar_test]
fn exercise_is_refused_for_anyone_but_the_holder(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    // Unsold: the holder field is all zeroes, which no signer can match.
    assert!(exercise_option(test, &env, &MALLORY_P, &ALICE_P, CALL_ID).is_err());
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    assert!(exercise_option(test, &env, &MALLORY_P, &ALICE_P, CALL_ID).is_err());
    assert_eq!(test.tokens(env.underlying_vault), FIVE_NVDAX);
}

/// Proceeds exist only after exercise, and only the writer may collect them.
#[quasar_test]
fn collect_proceeds_needs_an_exercised_option_and_the_writer(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    collect_proceeds(test, &env, &ALICE_P, CALL_ID).fails_with(OptionsError::OptionNotExercised);

    exercise_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    // A non-writer's signature derives a different option PDA, so the
    // account check fails before the handler runs.
    assert!(collect_proceeds(test, &env, &BOB_P, CALL_ID).is_err());
    assert_eq!(test.tokens(env.quote_vault), 900 * ONE_TOKEN + 250_000);
}

/// An exercised option has no collateral left to reclaim, whatever the clock
/// says.
#[quasar_test]
fn reclaim_is_refused_after_exercise(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    exercise_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    test.warp_to_timestamp(EXPIRY + 1);
    reclaim_collateral(test, &env, &ALICE_P, CALL_ID).fails_with(OptionsError::OptionNotHeld);
}

#[quasar_test]
fn collect_fees_is_refused_for_anyone_but_the_admin(test: &mut Test) {
    let env = setup(test);
    write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID).succeeds();
    assert!(collect_fees(test, &env, MALLORY, MALLORY_USDC).is_err());
    assert_eq!(
        u64::from(test.read::<Market>(env.market).fees_owed),
        250_000
    );
}

// ===========================================================================
// Parameter validation
// ===========================================================================

#[quasar_test]
fn write_option_rejects_zero_quantities_and_a_free_premium(test: &mut Test) {
    let env = setup(test);
    let attempts = [
        (0, ONE_NVDAX_PER_CONTRACT, CALL_STRIKE, CALL_PREMIUM),
        (CONTRACTS, 0, CALL_STRIKE, CALL_PREMIUM),
        (CONTRACTS, ONE_NVDAX_PER_CONTRACT, 0, CALL_PREMIUM),
        (CONTRACTS, ONE_NVDAX_PER_CONTRACT, CALL_STRIKE, 0),
    ];
    for (offset, (contracts, per_contract, strike, premium)) in attempts.into_iter().enumerate() {
        write_option(
            test,
            &env,
            &ALICE_P,
            10 + offset as u64,
            KIND_CALL,
            contracts,
            per_contract,
            strike,
            premium,
            EXPIRY,
        )
        .fails_with(OptionsError::InvalidParameter);
    }
    assert_eq!(test.tokens(ALICE_NVDAX), FIVE_NVDAX);
}

#[quasar_test]
fn write_option_rejects_an_unknown_kind(test: &mut Test) {
    let env = setup(test);
    write_option(
        test,
        &env,
        &ALICE_P,
        20,
        2,
        CONTRACTS,
        ONE_NVDAX_PER_CONTRACT,
        CALL_STRIKE,
        CALL_PREMIUM,
        EXPIRY,
    )
    .fails_with(OptionsError::InvalidParameter);
}

/// An expiry at or before now would be an option nobody could ever exercise.
#[quasar_test]
fn write_option_rejects_an_expiry_that_has_passed(test: &mut Test) {
    let env = setup(test);
    for expiry in [START_TIME, START_TIME - SECONDS_PER_DAY] {
        write_option(
            test,
            &env,
            &ALICE_P,
            30,
            KIND_CALL,
            CONTRACTS,
            ONE_NVDAX_PER_CONTRACT,
            CALL_STRIKE,
            CALL_PREMIUM,
            expiry,
        )
        .fails_with(OptionsError::ExpiryInPast);
    }
}

/// An option whose collateral would overflow is refused before anyone pays for it.
#[quasar_test]
fn write_option_rejects_a_lot_whose_collateral_overflows(test: &mut Test) {
    let env = setup(test);
    write_option(
        test,
        &env,
        &ALICE_P,
        40,
        KIND_CALL,
        u64::MAX,
        2,
        CALL_STRIKE,
        CALL_PREMIUM,
        EXPIRY,
    )
    .fails_with(OptionsError::MathOverflow);
}

#[quasar_test]
fn initialize_market_rejects_a_full_fee(test: &mut Test) {
    test.add(Wallet::new().at(MARIA));
    test.add(Mint::new(MARIA).at(NVDAX_MINT).decimals(6));
    test.add(Mint::new(MARIA).at(USDC_MINT).decimals(6));
    initialize_market(test, 10_000, USDC_MINT).fails_with(OptionsError::InvalidParameter);
}

/// One mint on both sides is refused by the runtime before the handler's own
/// check runs: the same account cannot be loaded into two slots.
#[quasar_test]
fn initialize_market_rejects_the_same_mint_on_both_sides(test: &mut Test) {
    test.add(Wallet::new().at(MARIA));
    test.add(Mint::new(MARIA).at(NVDAX_MINT).decimals(6));
    assert!(initialize_market(test, FEE_BPS, NVDAX_MINT).is_err());
}

/// A venue run at cost is a valid choice: the writer receives the whole
/// premium and no fee transfer is attempted.
#[quasar_test]
fn zero_fee_venue_pays_the_writer_the_whole_premium(test: &mut Test) {
    let env = setup_with_fee(test, 0);
    write_call(test, &env);
    buy_option(test, &env, &BOB_P, &ALICE_P, CALL_ID)
        .succeeds()
        .has_tokens(ALICE_USDC, STANDARD_USDC + CALL_PREMIUM);
    assert_eq!(u64::from(test.read::<Market>(env.market).fees_owed), 0);
    assert_vaults_match_ledger(test, &env);
}
