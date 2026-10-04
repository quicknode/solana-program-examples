//! quasar-test integration tests for the constant-product AMM: config and
//! pool creation (which takes the creator's first deposit), deposits
//! (including the ratio-clamp regression tests and the front-run that the
//! first deposit closes), withdrawals, swaps, admin-fee claims, and every
//! slippage guard rail.

use {
    crate::{
        cpi::{
            ClaimAdminFeesInstruction, DepositLiquidityInstruction, InitializeConfigInstruction,
            InitializePoolInstruction, SwapTokensInstruction, WithdrawLiquidityInstruction,
        },
        error::AmmError,
        state::Config,
        ConfigPda, LiquidityMintPda, PoolAPda, PoolBPda, PoolPda,
    },
    quasar_test::prelude::*,
};

/// `amount * numerator / denominator` in u128 with checked ops, narrowed back
/// to u64. Mirrors the program's ratio math for computing expected values.
fn mul_div(amount: u64, numerator: u64, denominator: u64) -> u64 {
    u64::try_from(
        (amount as u128)
            .checked_mul(numerator as u128)
            .expect("mul_div: product overflow")
            .checked_div(denominator as u128)
            .expect("mul_div: divide by zero"),
    )
    .expect("mul_div: result exceeds u64")
}

/// Constant-product quote mirroring the program's swap math, on effective
/// reserves: output = taxed_input * pool_out / (pool_in + taxed_input), where
/// taxed_input = input - input * fee_bps / 10_000. All products in u128.
fn expected_swap_output(input: u64, fee_bps: u64, pool_in: u64, pool_out: u64) -> u64 {
    let fee_amount = mul_div(input, fee_bps, crate::BASIS_POINTS_DIVISOR);
    let taxed_input = input.checked_sub(fee_amount).expect("fee exceeds input");
    let divisor = pool_in.checked_add(taxed_input).expect("reserve overflow");
    mul_div(taxed_input, pool_out, divisor)
}

/// Trading fee passed to `initialize_config`, in basis points.
const POOL_FEE_BPS: u64 = 30;
/// Admin's share of the trading fee, in basis points.
const ADMIN_SHARE_BPS: u16 = 1_667;

// Deterministic addresses.
const ADMIN: Pubkey = Pubkey::new_from_array([1; 32]);
const PAYER: Pubkey = Pubkey::new_from_array([2; 32]);
const MINT_A: Pubkey = Pubkey::new_from_array([3; 32]);
const MINT_B: Pubkey = Pubkey::new_from_array([4; 32]);
// The pool's creator, whose first deposit opens it in `initialize_pool`.
const CREATOR: Pubkey = Pubkey::new_from_array([7; 32]);
const CREATOR_TOKEN_A: Pubkey = Pubkey::new_from_array([8; 32]);
const CREATOR_TOKEN_B: Pubkey = Pubkey::new_from_array([9; 32]);
const CREATOR_LP: Pubkey = Pubkey::new_from_array([10; 32]);
// A second, independent depositor.
const DEPOSITOR: Pubkey = Pubkey::new_from_array([11; 32]);
const DEPOSITOR_TOKEN_A: Pubkey = Pubkey::new_from_array([12; 32]);
const DEPOSITOR_TOKEN_B: Pubkey = Pubkey::new_from_array([13; 32]);
const DEPOSITOR_LP: Pubkey = Pubkey::new_from_array([14; 32]);
// A trader and the withdraw/claim destinations.
const TRADER: Pubkey = Pubkey::new_from_array([15; 32]);
const TRADER_TOKEN_A: Pubkey = Pubkey::new_from_array([16; 32]);
const TRADER_TOKEN_B: Pubkey = Pubkey::new_from_array([17; 32]);
const RECV_A: Pubkey = Pubkey::new_from_array([18; 32]);
const RECV_B: Pubkey = Pubkey::new_from_array([19; 32]);
const ADMIN_TOKEN_A: Pubkey = Pubkey::new_from_array([20; 32]);
const ADMIN_TOKEN_B: Pubkey = Pubkey::new_from_array([21; 32]);
const BAD_ACTOR: Pubkey = Pubkey::new_from_array([22; 32]);
const BAD_TOKEN_A: Pubkey = Pubkey::new_from_array([23; 32]);
const BAD_TOKEN_B: Pubkey = Pubkey::new_from_array([24; 32]);

/// The pool's address, derived as the program derives it.
fn pool_config_address(test: &Test) -> Pubkey {
    let config = test.derive_pda(ConfigPda::seeds());
    test.derive_pda(PoolPda::seeds(&config, &MINT_A, &MINT_B))
}

/// The pool's token A reserve, a PDA of the pool.
fn pool_a(test: &Test) -> Pubkey {
    test.derive_pda(PoolAPda::seeds(&pool_config_address(test)))
}

/// The pool's token B reserve, a PDA of the pool.
fn pool_b(test: &Test) -> Pubkey {
    test.derive_pda(PoolBPda::seeds(&pool_config_address(test)))
}

struct PoolEnv {
    pool_config: Pubkey,
    lp_mint: Pubkey,
}

fn initialize_config(test: &mut Test, fee: u16, admin_share_bps: u16) -> Outcome {
    test.add(Wallet::new().at(PAYER));
    test.send(InitializeConfigInstruction {
        admin: ADMIN,
        payer: PAYER,
        fee,
        admin_share_bps,
    })
}

/// Creates the config and the two mints: everything a pool needs except
/// itself, so a test can build an `initialize_pool` the pool refuses.
fn setup_config_and_mints(test: &mut Test) {
    initialize_config(test, POOL_FEE_BPS as u16, ADMIN_SHARE_BPS).succeeds();

    // Pre-populate mint accounts (no onchain minting needed for tests).
    test.add(Mint::new(PAYER).at(MINT_A).decimals(6));
    test.add(Mint::new(PAYER).at(MINT_B).decimals(6));
}

/// `initialize_pool` with the creator's first deposit of `amount_a` /
/// `amount_b`, taken from `CREATOR_TOKEN_A` / `CREATOR_TOKEN_B`, which must
/// already hold them. The pool_config, LP-mint and reserve PDAs are all
/// derived by the builder; the reserves are owned by pool_config.
fn send_initialize_pool(test: &mut Test, amount_a: u64, amount_b: u64) -> Outcome {
    test.send(InitializePoolInstruction {
        mint_a: MINT_A,
        mint_b: MINT_B,
        creator: CREATOR,
        creator_token_a: CREATOR_TOKEN_A,
        creator_token_b: CREATOR_TOKEN_B,
        liquidity_provider_token: CREATOR_LP,
        payer: PAYER,
        amount_a,
        amount_b,
    })
}

/// Creates config + two mints + a pool opened with the creator's deposit of
/// exactly `amount_a` / `amount_b`. The creator's LP tokens are at
/// `CREATOR_LP`.
fn setup_pool(test: &mut Test, amount_a: u64, amount_b: u64) -> PoolEnv {
    setup_config_and_mints(test);
    fund(
        test,
        CREATOR,
        CREATOR_TOKEN_A,
        CREATOR_TOKEN_B,
        amount_a,
        amount_b,
    );
    send_initialize_pool(test, amount_a, amount_b).succeeds();

    let config = test.derive_pda(ConfigPda::seeds());
    PoolEnv {
        pool_config: test.derive_pda(PoolPda::seeds(&config, &MINT_A, &MINT_B)),
        lp_mint: test.derive_pda(LiquidityMintPda::seeds(&config, &MINT_A, &MINT_B)),
    }
}

/// Overwrites an SPL token account's balance in place (the `amount` is the
/// u64 at bytes 64..72), for states no instruction can produce.
fn set_token_balance(test: &mut Test, address: Pubkey, amount: u64) {
    let mut account = test.account(address).expect("token account missing");
    account.data[64..72].copy_from_slice(&amount.to_le_bytes());
    test.set_account(account);
}

/// Fund a depositor wallet with token A/B accounts holding the given amounts.
fn fund(
    test: &mut Test,
    wallet: Pubkey,
    token_a: Pubkey,
    token_b: Pubkey,
    amount_a: u64,
    amount_b: u64,
) {
    test.add(Wallet::new().at(wallet));
    test.add(
        TokenAccount::new(MINT_A, wallet)
            .at(token_a)
            .amount(amount_a),
    );
    test.add(
        TokenAccount::new(MINT_B, wallet)
            .at(token_b)
            .amount(amount_b),
    );
}

#[allow(clippy::too_many_arguments)]
fn deposit(
    test: &mut Test,
    depositor: Pubkey,
    token_a: Pubkey,
    token_b: Pubkey,
    lp_token: Pubkey,
    amount_a: u64,
    amount_b: u64,
    minimum_lp_tokens_out: u64,
) -> Outcome {
    test.send(DepositLiquidityInstruction {
        depositor,
        mint_a: MINT_A,
        mint_b: MINT_B,
        pool_a: pool_a(test),
        pool_b: pool_b(test),
        liquidity_provider_token: lp_token,
        token_a,
        token_b,
        payer: PAYER,
        amount_a,
        amount_b,
        minimum_lp_tokens_out,
    })
}

fn swap(
    test: &mut Test,
    trader: Pubkey,
    token_a: Pubkey,
    token_b: Pubkey,
    input_is_token_a: bool,
    input_amount: u64,
    min_output_amount: u64,
) -> Outcome {
    test.send(SwapTokensInstruction {
        trader,
        mint_a: MINT_A,
        mint_b: MINT_B,
        pool_a: pool_a(test),
        pool_b: pool_b(test),
        token_a,
        token_b,
        payer: PAYER,
        input_is_token_a,
        input_amount,
        min_output_amount,
    })
}

fn claim_fees(
    test: &mut Test,
    admin: Pubkey,
    admin_token_a: Pubkey,
    admin_token_b: Pubkey,
) -> Outcome {
    test.send(ClaimAdminFeesInstruction {
        mint_a: MINT_A,
        mint_b: MINT_B,
        pool_a: pool_a(test),
        pool_b: pool_b(test),
        admin,
        admin_token_a,
        admin_token_b,
    })
}

// ─── initialize_config ───────────────────────────────────────────────────────────

#[quasar_test]
fn initialize_config_records_admin_and_fees(test: &mut Test) {
    initialize_config(test, 30, 1_667).succeeds();

    let config = test.derive_pda(ConfigPda::seeds());
    let state = test.read::<Config>(config);
    assert_eq!(state.admin, ADMIN);
    assert_eq!(u16::from(state.fee), 30);
    assert_eq!(u16::from(state.admin_share_bps), 1_667);
}

#[quasar_test]
fn initialize_config_rejects_invalid_fee(test: &mut Test) {
    // fee >= 10_000 → invalid.
    let outcome = initialize_config(test, 10_000, 1_667);
    assert!(
        outcome.is_err(),
        "initialize_config should have failed with invalid fee"
    );
}

#[quasar_test]
fn initialize_config_rejects_invalid_admin_share(test: &mut Test) {
    // admin_share_bps >= 10_000 → invalid.
    let outcome = initialize_config(test, 30, 10_000);
    assert!(
        outcome.is_err(),
        "initialize_config should have failed with admin_share_bps >= 10000"
    );
}

// ─── initialize_pool ─────────────────────────────────────────────────────────────

#[quasar_test]
fn initialize_pool_creates_pool_config_and_lp_mint(test: &mut Test) {
    let env = setup_pool(test, 4_000_000, 1_000_000);
    // The pool_config PDA must now exist and be owned by our program.
    let pc = test
        .account(env.pool_config)
        .expect("pool_config missing after initialize_pool");
    assert_eq!(pc.owner, test.program_id());
    // LP mint PDA must be a valid SPL mint (82 bytes, owned by token program).
    let lp = test.account(env.lp_mint).expect("lp_mint missing");
    assert_eq!(lp.data.len(), 82, "LP mint should be 82 bytes");

    // pool_config is the authority for everything the pool holds: the LP
    // mint's mint authority (SPL mint layout: a 4-byte COption tag, 1 = Some,
    // then the 32-byte pubkey) and the owner of both reserves (SPL token
    // account layout: mint at 0..32, owner at 32..64).
    let expected: &[u8] = env.pool_config.as_ref();
    assert_eq!(
        &lp.data[0..4],
        &[1, 0, 0, 0],
        "LP mint must have an authority"
    );
    assert_eq!(
        &lp.data[4..36],
        expected,
        "LP mint authority must be pool_config"
    );
    for reserve in [pool_a(test), pool_b(test)] {
        let account = test.account(reserve).expect("reserve missing");
        assert_eq!(
            &account.data[32..64],
            expected,
            "reserve owner must be pool_config"
        );
    }
}

/// The pool opens with the creator's deposit: both reserves hold it, the
/// creator's balances fell by it, the creator holds
/// `sqrt(a * b) - MINIMUM_LIQUIDITY` LP tokens, and that is the whole LP
/// supply, so the floor is minted to nobody.
#[quasar_test]
fn initialize_pool_takes_first_deposit(test: &mut Test) {
    setup_config_and_mints(test);
    fund(
        test,
        CREATOR,
        CREATOR_TOKEN_A,
        CREATOR_TOKEN_B,
        5_000_000,
        2_000_000,
    );
    let config = test.derive_pda(ConfigPda::seeds());
    let lp_mint = test.derive_pda(LiquidityMintPda::seeds(&config, &MINT_A, &MINT_B));

    send_initialize_pool(test, 4_000_000, 1_000_000)
        .succeeds()
        .has_tokens(pool_a(test), 4_000_000)
        .has_tokens(pool_b(test), 1_000_000)
        .has_tokens(CREATOR_TOKEN_A, 5_000_000 - 4_000_000)
        .has_tokens(CREATOR_TOKEN_B, 2_000_000 - 1_000_000)
        // sqrt(4_000_000 * 1_000_000) = 2_000_000, minus the 100 floor.
        .has_tokens(CREATOR_LP, 2_000_000 - crate::MINIMUM_LIQUIDITY)
        .has_supply(lp_mint, 2_000_000 - crate::MINIMUM_LIQUIDITY);
}

/// A pool cannot open empty on either side: the first deposit sets the
/// price, and a zero reserve has none. Nothing is created when the
/// instruction fails.
#[quasar_test]
fn initialize_pool_rejects_zero_amount_a(test: &mut Test) {
    setup_config_and_mints(test);
    fund(
        test,
        CREATOR,
        CREATOR_TOKEN_A,
        CREATOR_TOKEN_B,
        4_000_000,
        1_000_000,
    );
    send_initialize_pool(test, 0, 1_000_000).fails_with(AmmError::EmptyInitialDeposit);
    assert!(
        test.account(pool_config_address(test)).is_none(),
        "no pool must exist after the refused initialize_pool"
    );
}

#[quasar_test]
fn initialize_pool_rejects_zero_amount_b(test: &mut Test) {
    setup_config_and_mints(test);
    fund(
        test,
        CREATOR,
        CREATOR_TOKEN_A,
        CREATOR_TOKEN_B,
        4_000_000,
        1_000_000,
    );
    send_initialize_pool(test, 4_000_000, 0).fails_with(AmmError::EmptyInitialDeposit);
    assert!(
        test.account(pool_config_address(test)).is_none(),
        "no pool must exist after the refused initialize_pool"
    );
}

// ─── deposit_liquidity ───────────────────────────────────────────────────────

/// The front-run. Maria (the creator) opens an ACME/USDC pool with 400 ACME
/// and 900 USDC, a price of 2.25 USDC per ACME, in one `initialize_pool`
/// call; the pool is created, funded and priced before anyone else can
/// touch it. Mallory then calls `deposit_liquidity` at once with a hostile
/// ratio, 400 ACME and 100 USDC (0.25 USDC per ACME). Her deposit is clamped
/// to Maria's ratio: her 100 USDC binds, only 44.444444 of her 400 ACME is
/// taken, and the pool still prices at 2.25 afterwards. The last part of the
/// test checks the refusal in code: a `deposit_liquidity` against a pool whose
/// effective reserves are zero fails with `EmptyPoolReserve`, so no
/// instruction lets a pool with no reserves take a depositor's ratio as its
/// price. (`initialize_pool` is the only instruction that creates a pool, and
/// `initialize_pool_rejects_zero_amount_a` / `_b` show it refuses a zero
/// side.)
#[quasar_test]
fn pool_creation_cannot_be_front_run(test: &mut Test) {
    let one: u64 = 10u64.pow(6); // both mints have 6 decimals
    setup_config_and_mints(test);
    // ACME is `MINT_A`, USDC is `MINT_B`.
    fund(
        test,
        CREATOR,
        CREATOR_TOKEN_A,
        CREATOR_TOKEN_B,
        400 * one,
        900 * one,
    );
    send_initialize_pool(test, 400 * one, 900 * one)
        .succeeds()
        .has_tokens(pool_a(test), 400 * one)
        .has_tokens(pool_b(test), 900 * one);

    // Mallory, with 400 ACME and 100 USDC, deposits all of both at once.
    fund(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        400 * one,
        100 * one,
    );
    deposit(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        DEPOSITOR_LP,
        400 * one,
        100 * one,
        0,
    )
    .succeeds();

    // 400 ACME would need 900 USDC at Maria's price, more than Mallory
    // offered, so her 100 USDC binds: 100 * 400 / 900 = 44.444444 ACME.
    let acme_taken = 400 * one - test.tokens(DEPOSITOR_TOKEN_A);
    let usdc_taken = 100 * one - test.tokens(DEPOSITOR_TOKEN_B);
    assert_eq!(acme_taken, 44_444_444);
    assert_eq!(usdc_taken, 100 * one);
    let pool_a_amount = test.tokens(pool_a(test));
    let pool_b_amount = test.tokens(pool_b(test));
    assert_eq!(pool_a_amount, 444_444_444);
    assert_eq!(pool_b_amount, 1_000 * one);
    // The price is still 2.25 USDC per ACME, to the cent.
    let price_in_cents = (pool_b_amount as u128) * 100 / (pool_a_amount as u128);
    assert_eq!(price_in_cents, 225);
    // Mallory is minted her share and nothing more: the LP supply is
    // sqrt(400 * 900) = 600 (floor included), and 100 USDC is a ninth of
    // the 900 in the pool, so min(44.444444 * 600 / 400, 100 * 600 / 900)
    // = 66.666666 LP tokens.
    assert_eq!(test.tokens(DEPOSITOR_LP), 66_666_666);

    // The pool can never be in this state through the program, so put it
    // there by hand: zero the token-A reserve's balance and show the handler
    // refuses to price a pool with an empty reserve. The balance check runs
    // first, so give Mallory the USDC she offers.
    set_token_balance(test, pool_a(test), 0);
    set_token_balance(test, DEPOSITOR_TOKEN_B, 100 * one);
    deposit(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        DEPOSITOR_LP,
        300 * one,
        100 * one,
        0,
    )
    .fails_with(AmmError::EmptyPoolReserve);
}

#[quasar_test]
fn deposit_liquidity_subsequent_proportional(test: &mut Test) {
    // The pool opens at a 1:4 ratio.
    setup_pool(test, 1_000_000, 4_000_000);
    let lp1_bal = test.tokens(CREATOR_LP);

    // Second depositor with the same 1:4 ratio gets proportional LP tokens.
    fund(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        500_000,
        2_000_000,
    );
    deposit(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        DEPOSITOR_LP,
        500_000,
        2_000_000,
        0,
    )
    .succeeds();
    let lp2_bal = test.tokens(DEPOSITOR_LP);

    // Half the creator's deposit mints half of that deposit's sqrt(a * b):
    // the LP it received plus the unminted floor, since deposits divide by
    // the same `supply + MINIMUM_LIQUIDITY` that withdrawals do.
    assert_eq!(lp1_bal, 2_000_000 - crate::MINIMUM_LIQUIDITY);
    assert_eq!(lp2_bal, (lp1_bal + crate::MINIMUM_LIQUIDITY) / 2);
}

/// Once every LP token is burned the floor's share of the reserves is still
/// in the pool, so the next deposit is clamped to the floor's ratio and mints
/// against the floor alone. Dividing by the bare supply of zero would mint
/// nothing and leave the pool unable to take deposits again.
#[quasar_test]
fn deposit_after_every_lp_token_is_burned(test: &mut Test) {
    setup_pool(test, 4_000_000, 4_000_000);
    let lp_balance = test.tokens(CREATOR_LP);

    // 3_999_900 * 4_000_000 / 4_000_000 of each side leaves; the floor's 100
    // of each side stays.
    withdraw(test, CREATOR, CREATOR_LP, RECV_A, RECV_B, lp_balance, 0, 0)
        .succeeds()
        .has_tokens(pool_a(test), crate::MINIMUM_LIQUIDITY)
        .has_tokens(pool_b(test), crate::MINIMUM_LIQUIDITY);

    // 1_000_000 * (0 + 100) / 100 = 1_000_000 LP tokens.
    fund(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        1_000_000,
        1_000_000,
    );
    deposit(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        DEPOSITOR_LP,
        1_000_000,
        1_000_000,
        0,
    )
    .succeeds()
    .has_tokens(DEPOSITOR_LP, 1_000_000);
}

#[quasar_test]
fn deposit_insufficient_funds_rejected(test: &mut Test) {
    setup_pool(test, 1_000_000, 1_000_000);

    // Fund with only 100 of each but request 1_000_000.
    fund(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        100,
        100,
    );
    deposit(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        DEPOSITOR_LP,
        1_000_000,
        1_000_000,
        0,
    )
    .fails_with(AmmError::InsufficientBalance);
}

/// Regression test for the ratio-clamp direction bug: with reserves at
/// pool_a > pool_b, logic that branches on RESERVE sizes (instead of which
/// USER amount is binding) scales `amount_a` UP to
/// `amount_b * pool_a / pool_b`, past both the user's stated amount and the
/// balance check. The correct try-A-then-B clamp scales token B DOWN instead.
#[quasar_test]
fn deposit_clamps_down_never_up(test: &mut Test) {
    // Open at a 4:1 ratio so pool_a > pool_b.
    let (pool_seed_a, pool_seed_b) = (4_000_000u64, 1_000_000u64);
    setup_pool(test, pool_seed_a, pool_seed_b);
    // Later deposits divide by the LP supply plus the unminted floor.
    let total_supply = test.tokens(CREATOR_LP) + crate::MINIMUM_LIQUIDITY;

    // Depositor offers 1_000_000 of each and holds exactly that much. The
    // old logic would try to pull 4_000_000 token A (scaling A UP); the
    // correct clamp uses all 1_000_000 A and scales B down to 250_000.
    let (stated_a, stated_b) = (1_000_000u64, 1_000_000u64);
    fund(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        stated_a,
        stated_b,
    );

    let expected_b_pulled = mul_div(stated_a, pool_seed_b, pool_seed_a);
    let expected_lp = mul_div(stated_a, total_supply, pool_seed_a);

    deposit(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        DEPOSITOR_LP,
        stated_a,
        stated_b,
        expected_lp,
    )
    .succeeds()
    // Exact amounts pulled: all of A, ratio-clamped B, nothing more.
    .has_tokens(DEPOSITOR_TOKEN_A, 0)
    .has_tokens(DEPOSITOR_TOKEN_B, stated_b - expected_b_pulled)
    .has_tokens(pool_a(test), pool_seed_a + stated_a)
    .has_tokens(pool_b(test), pool_seed_b + expected_b_pulled)
    // LP mint must be proportional.
    .has_tokens(DEPOSITOR_LP, expected_lp);
}

/// Mirror of `deposit_clamps_down_never_up` with the reserves reversed
/// (pool_b > pool_a), so the binding side is token A's counterpart: the full
/// `amount_b` is used and `amount_a` is the side that covers the ratio.
#[quasar_test]
fn deposit_clamps_down_other_side(test: &mut Test) {
    // Open at a 1:4 ratio so pool_b > pool_a.
    let (pool_seed_a, pool_seed_b) = (1_000_000u64, 4_000_000u64);
    setup_pool(test, pool_seed_a, pool_seed_b);
    // Later deposits divide by the LP supply plus the unminted floor.
    let total_supply = test.tokens(CREATOR_LP) + crate::MINIMUM_LIQUIDITY;

    let (stated_a, stated_b) = (1_000_000u64, 1_000_000u64);
    fund(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        stated_a,
        stated_b,
    );

    // amount_b_required for the full stated_a would be 4_000_000 > stated_b,
    // so amount_b binds: all of B is used and A is clamped down.
    let expected_a_pulled = mul_div(stated_b, pool_seed_a, pool_seed_b);
    let expected_lp = mul_div(stated_b, total_supply, pool_seed_b);

    deposit(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        DEPOSITOR_LP,
        stated_a,
        stated_b,
        expected_lp,
    )
    .succeeds()
    .has_tokens(DEPOSITOR_TOKEN_A, stated_a - expected_a_pulled)
    .has_tokens(DEPOSITOR_TOKEN_B, 0)
    .has_tokens(pool_a(test), pool_seed_a + expected_a_pulled)
    .has_tokens(pool_b(test), pool_seed_b + stated_b)
    // LP mint must be proportional.
    .has_tokens(DEPOSITOR_LP, expected_lp);
}

#[quasar_test]
fn deposit_slippage_rejected(test: &mut Test) {
    let (pool_seed_a, pool_seed_b) = (1_000_000u64, 1_000_000u64);
    setup_pool(test, pool_seed_a, pool_seed_b);
    // Later deposits divide by the LP supply plus the unminted floor.
    let total_supply = test.tokens(CREATOR_LP) + crate::MINIMUM_LIQUIDITY;

    let (stated_a, stated_b) = (500_000u64, 500_000u64);
    fund(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        stated_a,
        stated_b,
    );

    // The pool will mint exactly this much; ask for one more.
    let exact_lp = mul_div(stated_a, total_supply, pool_seed_a);
    deposit(
        test,
        DEPOSITOR,
        DEPOSITOR_TOKEN_A,
        DEPOSITOR_TOKEN_B,
        DEPOSITOR_LP,
        stated_a,
        stated_b,
        exact_lp + 1,
    )
    .fails_with(AmmError::DepositBelowMinimum);

    // Nothing moved: depositor balances and pool reserves are unchanged.
    assert_eq!(
        test.tokens(DEPOSITOR_TOKEN_A),
        stated_a,
        "token A must be untouched after revert"
    );
    assert_eq!(
        test.tokens(DEPOSITOR_TOKEN_B),
        stated_b,
        "token B must be untouched after revert"
    );
    assert_eq!(
        test.tokens(pool_a(test)),
        pool_seed_a,
        "pool_a must be untouched after revert"
    );
    assert_eq!(
        test.tokens(pool_b(test)),
        pool_seed_b,
        "pool_b must be untouched after revert"
    );
}

// ─── withdraw_liquidity ──────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn withdraw(
    test: &mut Test,
    depositor: Pubkey,
    lp_token: Pubkey,
    recv_a: Pubkey,
    recv_b: Pubkey,
    amount: u64,
    minimum_token_a_out: u64,
    minimum_token_b_out: u64,
) -> Outcome {
    test.send(WithdrawLiquidityInstruction {
        depositor,
        mint_a: MINT_A,
        mint_b: MINT_B,
        pool_a: pool_a(test),
        pool_b: pool_b(test),
        liquidity_provider_token: lp_token,
        token_a: recv_a,
        token_b: recv_b,
        payer: PAYER,
        amount,
        minimum_token_a_out,
        minimum_token_b_out,
    })
}

#[quasar_test]
fn withdraw_liquidity_pays_the_proportional_share(test: &mut Test) {
    let amount_a = 2_000_000u64;
    let amount_b = 2_000_000u64;
    setup_pool(test, amount_a, amount_b);
    let lp_balance = test.tokens(CREATOR_LP);
    assert!(lp_balance > 0);

    // Withdraw half the LP tokens.
    let withdraw_amount = lp_balance / 2;

    // Expected proportional share, mirroring the program's formula:
    //   amount_out = lp_amount * reserve / (lp_supply + MINIMUM_LIQUIDITY)
    // The creator holds the entire LP supply, so supply == lp_balance.
    let divisor = lp_balance
        .checked_add(crate::MINIMUM_LIQUIDITY)
        .expect("divisor overflow");
    let expected_a = mul_div(withdraw_amount, amount_a, divisor);
    let expected_b = mul_div(withdraw_amount, amount_b, divisor);

    // Output token accounts are created by init(idempotent). Pass the exact
    // expected amounts as the slippage floors: the pool hasn't moved since
    // the quote, so the floors must be met.
    withdraw(
        test,
        CREATOR,
        CREATOR_LP,
        RECV_A,
        RECV_B,
        withdraw_amount,
        expected_a,
        expected_b,
    )
    .succeeds()
    // The depositor received exactly the proportional share.
    .has_tokens(RECV_A, expected_a)
    .has_tokens(RECV_B, expected_b)
    // LP tokens were burned.
    .has_tokens(CREATOR_LP, lp_balance - withdraw_amount);
}

#[quasar_test]
fn withdraw_slippage_rejected(test: &mut Test) {
    setup_pool(test, 2_000_000, 2_000_000);
    let lp_balance = test.tokens(CREATOR_LP);

    let withdraw_amount = lp_balance / 2;
    let divisor = lp_balance
        .checked_add(crate::MINIMUM_LIQUIDITY)
        .expect("divisor overflow");
    let expected_a = mul_div(withdraw_amount, 2_000_000, divisor);

    // Floor on token A set just above what the pool will pay out.
    withdraw(
        test,
        CREATOR,
        CREATOR_LP,
        RECV_A,
        RECV_B,
        withdraw_amount,
        expected_a + 1,
        0,
    )
    .fails_with(AmmError::WithdrawalBelowMinimum);

    // Nothing moved: pool reserves and the LP balance are unchanged.
    assert_eq!(
        test.tokens(pool_a(test)),
        2_000_000,
        "pool_a must be untouched after revert"
    );
    assert_eq!(
        test.tokens(pool_b(test)),
        2_000_000,
        "pool_b must be untouched after revert"
    );
    assert_eq!(
        test.tokens(CREATOR_LP),
        lp_balance,
        "LP balance must be untouched after revert"
    );
}

// ─── swap_tokens ─────────────────────────────────────────────────────────────

#[quasar_test]
fn swap_a_to_b_conserves_balances(test: &mut Test) {
    let (pool_seed_a, pool_seed_b) = (10_000_000u64, 10_000_000u64);
    setup_pool(test, pool_seed_a, pool_seed_b);

    // Trader swaps 100_000 token A for token B (the output account is created
    // by init(idempotent)).
    let trader_funding = 1_000_000u64;
    test.add(Wallet::new().at(TRADER));
    test.add(
        TokenAccount::new(MINT_A, TRADER)
            .at(TRADER_TOKEN_A)
            .amount(trader_funding),
    );

    let input = 100_000u64;
    let expected_output = expected_swap_output(input, POOL_FEE_BPS, pool_seed_a, pool_seed_b);
    // floor = exact quote; the pool hasn't moved.
    swap(
        test,
        TRADER,
        TRADER_TOKEN_A,
        TRADER_TOKEN_B,
        true,
        input,
        expected_output,
    )
    .succeeds()
    // Conservation: the trader pays exactly `input` and receives exactly
    // what the pool sent; nothing is minted or lost in transit.
    .has_tokens(TRADER_TOKEN_A, trader_funding - input)
    .has_tokens(TRADER_TOKEN_B, expected_output)
    .has_tokens(pool_a(test), pool_seed_a + input)
    .has_tokens(pool_b(test), pool_seed_b - expected_output);
}

#[quasar_test]
fn swap_b_to_a_conserves_balances(test: &mut Test) {
    let (pool_seed_a, pool_seed_b) = (10_000_000u64, 10_000_000u64);
    setup_pool(test, pool_seed_a, pool_seed_b);

    let trader_funding = 1_000_000u64;
    test.add(Wallet::new().at(TRADER));
    test.add(
        TokenAccount::new(MINT_B, TRADER)
            .at(TRADER_TOKEN_B)
            .amount(trader_funding),
    );

    let input = 100_000u64;
    let expected_output = expected_swap_output(input, POOL_FEE_BPS, pool_seed_b, pool_seed_a);
    // input_is_token_a = false.
    swap(
        test,
        TRADER,
        TRADER_TOKEN_A,
        TRADER_TOKEN_B,
        false,
        input,
        expected_output,
    )
    .succeeds()
    .has_tokens(TRADER_TOKEN_B, trader_funding - input)
    .has_tokens(TRADER_TOKEN_A, expected_output)
    .has_tokens(pool_b(test), pool_seed_b + input)
    .has_tokens(pool_a(test), pool_seed_a - expected_output);
}

#[quasar_test]
fn swap_slippage_rejected(test: &mut Test) {
    setup_pool(test, 10_000_000, 10_000_000);

    test.add(Wallet::new().at(TRADER));
    test.add(
        TokenAccount::new(MINT_A, TRADER)
            .at(TRADER_TOKEN_A)
            .amount(1_000_000),
    );

    // min_output set one above the exact quote, so the floor cannot be met.
    let input = 100_000u64;
    let quote = expected_swap_output(input, POOL_FEE_BPS, 10_000_000, 10_000_000);
    swap(
        test,
        TRADER,
        TRADER_TOKEN_A,
        TRADER_TOKEN_B,
        true,
        input,
        quote + 1,
    )
    .fails_with(AmmError::SlippageExceeded);

    // Nothing moved: the trader keeps their input and the pool is untouched.
    assert_eq!(
        test.tokens(TRADER_TOKEN_A),
        1_000_000,
        "trader balance must be untouched after revert"
    );
    assert_eq!(
        test.tokens(pool_a(test)),
        10_000_000,
        "pool_a must be untouched after revert"
    );
    assert_eq!(
        test.tokens(pool_b(test)),
        10_000_000,
        "pool_b must be untouched after revert"
    );
}

/// A swap that names a token account of its own as `pool_a` must be refused.
/// Without the `has_one(pool_a)` check the handler would price the trade from
/// that account's one-unit balance, send the trader's input into it (back to
/// the trader), and pay out nearly all of `pool_b` from the real reserve.
#[quasar_test]
fn swap_rejects_substituted_pool_vault(test: &mut Test) {
    setup_pool(test, 10_000_000, 10_000_000);

    test.add(Wallet::new().at(BAD_ACTOR));
    test.add(
        TokenAccount::new(MINT_A, BAD_ACTOR)
            .at(BAD_TOKEN_A)
            .amount(1_000_001),
    );
    // A second mint-A account the attacker owns, holding one unit, passed as
    // the pool's token A reserve.
    let fake_pool_a = Pubkey::new_from_array([25; 32]);
    test.add(
        TokenAccount::new(MINT_A, BAD_ACTOR)
            .at(fake_pool_a)
            .amount(1),
    );

    test.send(SwapTokensInstruction {
        trader: BAD_ACTOR,
        mint_a: MINT_A,
        mint_b: MINT_B,
        pool_a: fake_pool_a,
        pool_b: pool_b(test),
        token_a: BAD_TOKEN_A,
        token_b: BAD_TOKEN_B,
        payer: PAYER,
        input_is_token_a: true,
        input_amount: 1_000_000,
        min_output_amount: 1,
    })
    .fails_with(AmmError::InvalidPoolVault);

    assert_eq!(
        test.tokens(pool_b(test)),
        10_000_000,
        "pool_b must be untouched after the refused swap"
    );
}

/// A deposit that names the depositor's own token accounts as the reserves
/// must be refused. Without the `has_one` checks the LP tokens minted would be
/// priced from those accounts' balances, and the deposit would land in them.
#[quasar_test]
fn deposit_rejects_substituted_pool_vaults(test: &mut Test) {
    setup_pool(test, 10_000_000, 10_000_000);

    fund(
        test,
        BAD_ACTOR,
        BAD_TOKEN_A,
        BAD_TOKEN_B,
        1_000_000,
        1_000_000,
    );
    let fake_pool_a = Pubkey::new_from_array([25; 32]);
    let fake_pool_b = Pubkey::new_from_array([26; 32]);
    test.add(
        TokenAccount::new(MINT_A, BAD_ACTOR)
            .at(fake_pool_a)
            .amount(1),
    );
    test.add(
        TokenAccount::new(MINT_B, BAD_ACTOR)
            .at(fake_pool_b)
            .amount(1),
    );

    test.send(DepositLiquidityInstruction {
        depositor: BAD_ACTOR,
        mint_a: MINT_A,
        mint_b: MINT_B,
        pool_a: fake_pool_a,
        pool_b: fake_pool_b,
        liquidity_provider_token: Pubkey::new_from_array([27; 32]),
        token_a: BAD_TOKEN_A,
        token_b: BAD_TOKEN_B,
        payer: PAYER,
        amount_a: 1_000_000,
        amount_b: 1_000_000,
        minimum_lp_tokens_out: 0,
    })
    .fails_with(AmmError::InvalidPoolVault);
}

// ─── claim_admin_fees ────────────────────────────────────────────────────────

#[quasar_test]
fn claim_admin_fees_pays_the_admin(test: &mut Test) {
    // Open the pool and do a swap so fees accumulate.
    setup_pool(test, 10_000_000, 10_000_000);
    test.add(Wallet::new().at(TRADER));
    test.add(
        TokenAccount::new(MINT_A, TRADER)
            .at(TRADER_TOKEN_A)
            .amount(1_000_000),
    );
    swap(
        test,
        TRADER,
        TRADER_TOKEN_A,
        TRADER_TOKEN_B,
        true,
        500_000,
        1,
    )
    .succeeds();

    // Admin claims accumulated fees.
    test.add(Wallet::new().at(ADMIN));
    test.add(TokenAccount::new(MINT_A, ADMIN).at(ADMIN_TOKEN_A));
    test.add(TokenAccount::new(MINT_B, ADMIN).at(ADMIN_TOKEN_B));

    claim_fees(test, ADMIN, ADMIN_TOKEN_A, ADMIN_TOKEN_B).succeeds();

    // After the claim, admin_token_a should have received some fees (A was
    // the input side).
    assert!(
        test.tokens(ADMIN_TOKEN_A) > 0,
        "admin should have received token-A fees"
    );
}

#[quasar_test]
fn claim_admin_fees_rejects_non_admin(test: &mut Test) {
    setup_pool(test, 10_000_000, 10_000_000);

    // Swap to accumulate some fees.
    test.add(Wallet::new().at(TRADER));
    test.add(
        TokenAccount::new(MINT_A, TRADER)
            .at(TRADER_TOKEN_A)
            .amount(1_000_000),
    );
    swap(
        test,
        TRADER,
        TRADER_TOKEN_A,
        TRADER_TOKEN_B,
        true,
        100_000,
        1,
    )
    .succeeds();

    // Impersonator tries to claim with a wrong signer.
    test.add(Wallet::new().at(BAD_ACTOR));
    test.add(TokenAccount::new(MINT_A, BAD_ACTOR).at(BAD_TOKEN_A));
    test.add(TokenAccount::new(MINT_B, BAD_ACTOR).at(BAD_TOKEN_B));

    let outcome = claim_fees(test, BAD_ACTOR, BAD_TOKEN_A, BAD_TOKEN_B);
    assert!(
        outcome.is_err(),
        "unauthorized claim_admin_fees should fail"
    );
}
