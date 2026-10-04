//! quasar-test integration tests. `fund_setup` drives the manager-side
//! setup (registry, approve asset, fund, add asset) and asserts state.
//! `deposit` is a two-program test: it loads the mock swap router too, wires
//! up rates and a Pyth-shaped price feed, and deposits, checking that the
//! deposit is priced 1:1 on the first deposit and deployed into the basket
//! through the router CPI. `deposit_rejects_price_from_before_a_restart`
//! reuses that setup to show a pre-restart price is refused.
//! `donation_does_not_inflate_share_price` shows a donation straight into the
//! USDC vault leaves the share price alone, and
//! `deposit_rejects_leg_that_buys_nothing` shows a deposit too small to buy
//! any of the asset is refused. The `test_rebalance*` tests run on a two-asset
//! fund (TSLAx 40%, NVDAx 60%) and carry the Anchor suite's names: a stranger
//! rebalances a drifted fund, and a fund at target, drift under the threshold,
//! a repeat call, an over-weight buy side, missing asset accounts and donated
//! tokens are all refused, while a retired asset is sold out.
//! `test_initialize_rejects_threshold_out_of_range` bounds the threshold, and
//! `test_valuation_scales_by_decimals_and_exponent` values an eight-decimal
//! asset priced by an exponent -5 feed, and
//! `test_wide_confidence_price_rejected` shows a feed whose confidence interval
//! is past 1% of its price stops deposit and rebalance but not withdraw.

use {
    crate::{
        cpi::{
            AddAssetInstruction, ApproveAssetInstruction, DepositInstruction,
            InitializeFundInstruction, InitializeRegistryInstruction, RebalanceInstruction,
            SetWeightInstruction, WithdrawInstruction,
        },
        errors::FundError,
        instructions::initialize_fund::{MAX_REBALANCE_THRESHOLD_BPS, MIN_REBALANCE_THRESHOLD_BPS},
        state::{
            read_asset_holdings, AssetConfig, AssetVaultPda, Fund, Registry, ShareMintPda,
            UsdcVaultPda, MAX_ASSETS,
        },
    },
    quasar_test::prelude::*,
};

const DECIMALS: u8 = 6;
/// One whole six-decimal token, in minor units.
const ONE_TOKEN: u64 = 1_000_000;
const FEE_BPS: u16 = 100;
const MAX_SLIPPAGE_BPS: u16 = 100;
const REBALANCE_THRESHOLD_BPS: u16 = 200; // two percentage points

// Router program (loaded for the deposit test).
const ROUTER_ID_STR: &str = "SWPR8Rk3aq3DrDGLdaANq7xCMnXoUFUJWJJmCWxc8Jm";
const RATE: u64 = 250_000_000; // router USDC minor units per whole token
const NOW: i64 = 1_000; // fixed clock for the deposit test
const FUND_INDEX: u64 = 0;

// Deterministic addresses.
const AUTHORITY: Pubkey = Pubkey::new_from_array([1; 32]);
const MANAGER: Pubkey = Pubkey::new_from_array([2; 32]);
const DEPOSITOR: Pubkey = Pubkey::new_from_array([3; 32]);
const USDC_MINT: Pubkey = Pubkey::new_from_array([4; 32]);
const ASSET_MINT: Pubkey = Pubkey::new_from_array([5; 32]);
const PRICE_FEED: Pubkey = Pubkey::new_from_array([6; 32]);
const DEPOSITOR_USDC: Pubkey = Pubkey::new_from_array([7; 32]);
const DEPOSITOR_SHARE: Pubkey = Pubkey::new_from_array([8; 32]);
const FEED_OWNER: Pubkey = Pubkey::new_from_array([9; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([11; 32]);
const ATTACKER_USDC: Pubkey = Pubkey::new_from_array([12; 32]);
const ATTACKER_SHARE: Pubkey = Pubkey::new_from_array([13; 32]);
const ATTACKER_ASSET: Pubkey = Pubkey::new_from_array([14; 32]);
const VICTIM: Pubkey = Pubkey::new_from_array([15; 32]);
const VICTIM_USDC: Pubkey = Pubkey::new_from_array([16; 32]);
const VICTIM_SHARE: Pubkey = Pubkey::new_from_array([17; 32]);
const VICTIM_ASSET: Pubkey = Pubkey::new_from_array([18; 32]);

fn router_id() -> Pubkey {
    ROUTER_ID_STR.parse().unwrap()
}

// Router PDAs (owned by the router program, so derived manually).
fn router_config_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"router_config"], &router_id()).0
}
fn router_treasury_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"treasury"], &router_id()).0
}
fn router_rate_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"rate", mint.as_ref()], &router_id()).0
}

// A Pyth PriceUpdateV2-shaped account: `price` (i64) at offset 73, `conf`
// (u64) at offset 81, `exponent` (i32) at offset 89, `publish_time` (i64) at
// offset 93, `posted_slot` (u64) at offset 125. The program reads only those
// five fields. Posted at slot 1, with the -8 exponent of Pyth's crypto USD
// feeds and a zero confidence interval.
fn add_pyth_feed(test: &mut Test, price: i64, publish_time: i64) {
    add_pyth_feed_posted_at(test, price, publish_time, 1);
}

// The same feed, as if Pyth posted it in `posted_slot`.
fn add_pyth_feed_posted_at(test: &mut Test, price: i64, publish_time: i64, posted_slot: u64) {
    write_price_feed(test, PRICE_FEED, price, -8, publish_time, posted_slot);
}

/// Write a Pyth feed with its own exponent: Pyth's US equity feeds use -5.
fn write_price_feed(
    test: &mut Test,
    feed: Pubkey,
    price: i64,
    exponent: i32,
    publish_time: i64,
    posted_slot: u64,
) {
    write_price_feed_with_confidence(test, feed, price, 0, exponent, publish_time, posted_slot);
}

/// Write a Pyth feed with its own confidence interval, in the price's units.
fn write_price_feed_with_confidence(
    test: &mut Test,
    feed: Pubkey,
    price: i64,
    confidence: u64,
    exponent: i32,
    publish_time: i64,
    posted_slot: u64,
) {
    let mut data = vec![0u8; 200];
    data[73..81].copy_from_slice(&price.to_le_bytes());
    data[81..89].copy_from_slice(&confidence.to_le_bytes());
    data[89..93].copy_from_slice(&exponent.to_le_bytes());
    data[93..101].copy_from_slice(&publish_time.to_le_bytes());
    data[125..133].copy_from_slice(&posted_slot.to_le_bytes());
    test.set_account(Account::new(feed, FEED_OWNER, 1_000_000, data));
}

/// Pin the LastRestartSlot sysvar account, simulating a cluster restart at
/// `slot`: prices posted at or before it must be rejected until Pyth posts
/// again. The sysvar's whole data is one little-endian u64.
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

/// The fund-side PDAs the assertions read.
struct Pdas {
    fund: Pubkey,
    asset_config: Pubkey,
    vault_asset: Pubkey,
    vault_usdc: Pubkey,
    share_mint: Pubkey,
}

fn pdas(test: &Test) -> Pdas {
    let fund = test.derive_pda(Fund::seeds(FUND_INDEX));
    Pdas {
        fund,
        asset_config: test.derive_pda(AssetConfig::seeds(&fund, 0)),
        vault_asset: test.derive_pda(AssetVaultPda::seeds(&fund, 0)),
        vault_usdc: test.derive_pda(UsdcVaultPda::seeds(&fund)),
        share_mint: test.derive_pda(ShareMintPda::seeds(&fund)),
    }
}

/// Registry + approved asset + fund + one basket asset at 100% weight.
fn setup_fund(test: &mut Test, asset_mint_authority: Pubkey) {
    test.add(Wallet::new().at(AUTHORITY));
    test.add(Wallet::new().at(MANAGER));
    test.add(Mint::new(AUTHORITY).at(USDC_MINT).decimals(DECIMALS));
    test.add(
        Mint::new(asset_mint_authority)
            .at(ASSET_MINT)
            .decimals(DECIMALS),
    );

    let registry = test.derive_pda(Registry::seeds(&AUTHORITY));

    test.send(InitializeRegistryInstruction {
        authority: AUTHORITY,
    })
    .succeeds();
    test.send(ApproveAssetInstruction {
        authority: AUTHORITY,
        asset_mint: ASSET_MINT,
        price_feed: PRICE_FEED,
    })
    .succeeds();
    test.send(InitializeFundInstruction {
        manager: MANAGER,
        usdc_mint: USDC_MINT,
        registry,
        index: FUND_INDEX,
        fee_bps: FEE_BPS,
        max_slippage_bps: MAX_SLIPPAGE_BPS,
        rebalance_threshold_bps: REBALANCE_THRESHOLD_BPS,
        swap_router: router_id(),
    })
    .succeeds();
    test.send(AddAssetInstruction {
        manager: MANAGER,
        fund_index_seed: FUND_INDEX,
        registry,
        asset_mint: ASSET_MINT,
        fund_asset_count_seed: 0,
        weight_bps: 10_000,
    })
    .succeeds();
}

#[quasar_test]
fn fund_setup_records_the_basket(test: &mut Test) {
    setup_fund(test, AUTHORITY);
    let w = pdas(test);

    let fund = test.read::<Fund>(w.fund);
    assert_eq!(fund.asset_count, 1, "asset_count");
    assert_eq!(u16::from(fund.total_weight_bps), 10_000, "total_weight_bps");

    let asset_config = test.read::<AssetConfig>(w.asset_config);
    assert_eq!(u16::from(asset_config.weight_bps), 10_000, "weight_bps");
    assert_eq!(asset_config.mint, ASSET_MINT, "asset mint");
    assert_eq!(
        asset_config.price_feed, PRICE_FEED,
        "price feed copied from registry"
    );
}

// Asset priced 250 USDC/token: Pyth price = 250 * 10^8 at exponent -8, so with
// six-decimal USDC and asset, asset_value = amount * price / 10^8 gives 250 USDC
// minor units per asset minor unit.
const PYTH_PRICE: i64 = 250 * 100_000_000;
const DEPOSIT: u64 = 1_000;

/// Load the router, set up a single-asset fund, fund the depositor, and
/// initialize the router with the asset's rate. Leaves the Pyth feed to the
/// caller.
fn setup_deposit(test: &mut Test) -> Pdas {
    load_router(test);

    // The router config account is the asset mint's mint authority, so the
    // router can mint it on swap.
    setup_fund(test, router_config_pda());
    let w = pdas(test);

    test.add(Wallet::new().at(DEPOSITOR));

    // Depositor token accounts (share account created up front).
    test.add(
        TokenAccount::new(USDC_MINT, DEPOSITOR)
            .at(DEPOSITOR_USDC)
            .amount(DEPOSIT),
    );
    test.add(TokenAccount::new(w.share_mint, DEPOSITOR).at(DEPOSITOR_SHARE));

    initialize_router(test);
    set_router_rate(test, ASSET_MINT, RATE);

    w
}

/// Load the router program and pin the clock (the router's own builders live
/// in the sibling crate, so its instructions here are hand-built).
fn load_router(test: &mut Test) {
    // Runtime read (NOT include_bytes!): quasar-test auto-loads only this
    // program's .so; the sibling router program is added explicitly.
    let router_elf =
        std::fs::read("../mock-swap-router/target/deploy/quasar_mock_swap_router.so").unwrap();
    test.add(Program::new(router_id(), &router_elf));
    test.warp_to_timestamp(NOW);
}

fn rent_id() -> Pubkey {
    "SysvarRent111111111111111111111111111111111"
        .parse()
        .unwrap()
}

/// Initialize the router with `USDC_MINT` as its USDC.
fn initialize_router(test: &mut Test) {
    test.send(Instruction {
        program_id: router_id(),
        accounts: vec![
            AccountMeta::new(AUTHORITY, true),
            AccountMeta::new_readonly(USDC_MINT, false),
            AccountMeta::new(router_config_pda(), false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: vec![0u8],
    })
    .succeeds();
}

/// Set (or reset) the router's rate for `mint`, in USDC minor units per whole
/// token. The first call also creates the router's USDC treasury.
fn set_router_rate(test: &mut Test, mint: Pubkey, rate: u64) {
    let mut set_rate_data = vec![1u8];
    set_rate_data.extend_from_slice(&rate.to_le_bytes());
    test.send(Instruction {
        program_id: router_id(),
        accounts: vec![
            AccountMeta::new(AUTHORITY, true),
            AccountMeta::new_readonly(router_config_pda(), false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(USDC_MINT, false),
            AccountMeta::new(router_rate_pda(&mint), false),
            AccountMeta::new(router_treasury_pda(), false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: set_rate_data,
    })
    .succeeds();
}

/// Deposit `DEPOSIT` USDC: declared accounts, then remaining accounts per
/// basket asset (asset_config, vault_asset, asset_mint, asset_rate, price_feed).
fn send_deposit(test: &mut Test, w: &Pdas) -> Outcome {
    test.send(DepositInstruction {
        depositor: DEPOSITOR,
        fund_index_seed: FUND_INDEX,
        usdc_mint: USDC_MINT,
        depositor_usdc_account: DEPOSITOR_USDC,
        depositor_share_account: DEPOSITOR_SHARE,
        router_config: router_config_pda(),
        router_usdc_treasury: router_treasury_pda(),
        swap_router_program: router_id(),
        usdc_amount: DEPOSIT,
        minimum_shares: DEPOSIT,
        remaining_accounts: vec![
            AccountMeta::new_readonly(w.asset_config, false),
            AccountMeta::new(w.vault_asset, false),
            AccountMeta::new(ASSET_MINT, false),
            AccountMeta::new_readonly(router_rate_pda(&ASSET_MINT), false),
            AccountMeta::new_readonly(PRICE_FEED, false),
        ],
    })
}

/// Two-program deposit: set up the router + a single-asset fund, then
/// deposit USDC. The first deposit mints shares 1:1 and deploys the whole
/// amount into the asset through the router CPI.
#[quasar_test]
fn deposit_mints_shares_and_deploys_into_the_basket(test: &mut Test) {
    let w = setup_deposit(test);
    add_pyth_feed(test, PYTH_PRICE, NOW);

    const ASSET_OUT: u64 = DEPOSIT * ONE_TOKEN / RATE; // 4

    send_deposit(test, &w)
        .succeeds()
        // First deposit mints shares 1:1 with USDC.
        .has_tokens(DEPOSITOR_SHARE, DEPOSIT)
        // The deposit was deployed into the asset via the router.
        .has_tokens(w.vault_asset, ASSET_OUT)
        .has_tokens(DEPOSITOR_USDC, 0)
        .has_tokens(router_treasury_pda(), DEPOSIT)
        // All USDC was swapped out of the vault into the asset.
        .has_tokens(w.vault_usdc, 0);
}

/// Under Alpenglow the Clock's unix_timestamp may only advance by up to twice
/// the slot time elapsed since the parent block, so after a halt it trails real
/// time and a price published just before the halt still passes the 60-second
/// staleness check. The fund must reject any price posted at or before the
/// restart slot until Pyth posts again.
#[quasar_test]
fn deposit_rejects_price_from_before_a_restart(test: &mut Test) {
    let w = setup_deposit(test);

    // The feed is posted at slot 1 and stamped `NOW`, so it is fresh by the
    // 60-second bound, but the cluster restarted at slot 3: only the restart
    // check can catch it.
    add_pyth_feed(test, PYTH_PRICE, NOW);
    set_last_restart_slot(test, 3);
    send_deposit(test, &w).fails_with(FundError::PricePredatesRestart);

    // Pyth posting again after the restart (slot 4) reopens the fund.
    add_pyth_feed_posted_at(test, PYTH_PRICE, NOW, 4);
    send_deposit(test, &w)
        .succeeds()
        .has_tokens(DEPOSITOR_SHARE, DEPOSIT);
}

/// A depositor's wallet plus their USDC, share, and asset token accounts (the
/// share account must exist before a deposit; the asset account before an
/// in-kind withdrawal).
fn add_depositor(test: &mut Test, w: &Pdas, owner: Pubkey, accounts: [Pubkey; 3], usdc: u64) {
    let [usdc_account, share_account, asset_account] = accounts;
    test.add(Wallet::new().at(owner));
    test.add(
        TokenAccount::new(USDC_MINT, owner)
            .at(usdc_account)
            .amount(usdc),
    );
    test.add(TokenAccount::new(w.share_mint, owner).at(share_account));
    test.add(TokenAccount::new(ASSET_MINT, owner).at(asset_account));
}

/// Deposit: declared accounts, then remaining accounts per basket asset
/// (asset_config, vault_asset, asset_mint, asset_rate, price_feed).
fn deposit_as(
    w: &Pdas,
    depositor: Pubkey,
    usdc_account: Pubkey,
    share_account: Pubkey,
    usdc_amount: u64,
    minimum_shares: u64,
) -> DepositInstruction {
    DepositInstruction {
        depositor,
        fund_index_seed: FUND_INDEX,
        usdc_mint: USDC_MINT,
        depositor_usdc_account: usdc_account,
        depositor_share_account: share_account,
        router_config: router_config_pda(),
        router_usdc_treasury: router_treasury_pda(),
        swap_router_program: router_id(),
        usdc_amount,
        minimum_shares,
        remaining_accounts: vec![
            AccountMeta::new_readonly(w.asset_config, false),
            AccountMeta::new(w.vault_asset, false),
            AccountMeta::new(ASSET_MINT, false),
            AccountMeta::new_readonly(router_rate_pda(&ASSET_MINT), false),
            AccountMeta::new_readonly(PRICE_FEED, false),
        ],
    }
}

/// Withdraw in kind: declared accounts, then remaining accounts per basket
/// asset (asset_config, vault_asset, asset_mint, user_asset_account).
fn withdraw(
    w: &Pdas,
    user: Pubkey,
    accounts: [Pubkey; 3],
    shares_to_burn: u64,
) -> WithdrawInstruction {
    let [usdc_account, share_account, asset_account] = accounts;
    WithdrawInstruction {
        user,
        fund_index_seed: FUND_INDEX,
        usdc_mint: USDC_MINT,
        user_share_account: share_account,
        user_usdc_account: usdc_account,
        shares_to_burn,
        min_usdc_out: 0,
        remaining_accounts: vec![
            AccountMeta::new_readonly(w.asset_config, false),
            AccountMeta::new(w.vault_asset, false),
            AccountMeta::new_readonly(ASSET_MINT, false),
            AccountMeta::new(asset_account, false),
        ],
    }
}

/// A holder's position valued in USDC minor units at the test's price: USDC
/// plus the asset at 250 USDC per token.
fn value_in_usdc(test: &Test, usdc_account: Pubkey, asset_account: Pubkey) -> u64 {
    test.tokens(usdc_account) + test.tokens(asset_account) * RATE / ONE_TOKEN
}

/// The first-depositor inflation attack: a small first deposit, then a donation
/// straight into the fund's USDC vault, then a 1,000 USDC deposit with no
/// `minimum_shares` floor. The program prices shares from the holdings it has
/// recorded, so the donation changes the vault's balance and nothing the
/// handler reads: the victim gets exactly the shares they would have got
/// without it, and the donated USDC stays in the vault outside the fund.
/// Modeled on the lending example's
/// `raw_token_donation_does_not_inflate_exchange_rate`.
#[quasar_test]
fn donation_does_not_inflate_share_price(test: &mut Test) {
    let w = setup_deposit(test);
    add_pyth_feed(test, PYTH_PRICE, NOW);

    // The smallest deposit that buys any of the asset at 250 USDC minor units
    // per asset minor unit.
    const ATTACKER_DEPOSIT: u64 = RATE / ONE_TOKEN;
    const DONATION: u64 = 1_000_000_000; // 1,000 USDC
    const VICTIM_DEPOSIT: u64 = 1_000_000_000; // 1,000 USDC

    let attacker_accounts = [ATTACKER_USDC, ATTACKER_SHARE, ATTACKER_ASSET];
    let victim_accounts = [VICTIM_USDC, VICTIM_SHARE, VICTIM_ASSET];
    add_depositor(
        test,
        &w,
        ATTACKER,
        attacker_accounts,
        ATTACKER_DEPOSIT + DONATION,
    );
    add_depositor(test, &w, VICTIM, victim_accounts, VICTIM_DEPOSIT);

    // The empty-fund deposit mints one share per minor unit and buys one minor
    // unit of the asset.
    test.send(deposit_as(
        &w,
        ATTACKER,
        ATTACKER_USDC,
        ATTACKER_SHARE,
        ATTACKER_DEPOSIT,
        0,
    ))
    .succeeds()
    .has_tokens(ATTACKER_SHARE, ATTACKER_DEPOSIT)
    .has_tokens(w.vault_asset, 1);

    // The attacker sends 1,000 USDC straight to the USDC vault with an ordinary
    // token transfer (SPL Token `Transfer`, instruction 3). The deposit handler
    // never ran, so the program records none of it.
    let mut transfer_data = vec![3u8];
    transfer_data.extend_from_slice(&DONATION.to_le_bytes());
    test.send(Instruction {
        program_id: SPL_TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(ATTACKER_USDC, false),
            AccountMeta::new(w.vault_usdc, false),
            AccountMeta::new_readonly(ATTACKER, true),
        ],
        data: transfer_data,
    })
    .succeeds()
    .has_tokens(w.vault_usdc, DONATION);
    let fund = test.read::<Fund>(w.fund);
    assert_eq!(
        u64::from(fund.usdc_holdings),
        0,
        "a donation is not recorded"
    );

    // The victim deposits 1,000 USDC with no floor. Priced off the recorded NAV of
    // 250 minor units against 250 shares: 1,000,000,000 shares, the same as with
    // no donation at all. Read off the vault balance it would have been
    // 1,000,000,000 * 250 / 1,000,000,250 = 249.
    test.send(deposit_as(
        &w,
        VICTIM,
        VICTIM_USDC,
        VICTIM_SHARE,
        VICTIM_DEPOSIT,
        0,
    ))
    .succeeds()
    .has_tokens(VICTIM_SHARE, VICTIM_DEPOSIT);

    // The victim redeems everything in kind, all 1,000 USDC of it in the asset.
    test.send(withdraw(&w, VICTIM, victim_accounts, VICTIM_DEPOSIT))
        .succeeds()
        .has_tokens(VICTIM_SHARE, 0);
    assert_eq!(
        value_in_usdc(test, VICTIM_USDC, VICTIM_ASSET),
        VICTIM_DEPOSIT
    );

    // The attacker redeems their shares for what they deposited through the
    // handler. The donation is never paid out and stays in the vault.
    test.send(withdraw(&w, ATTACKER, attacker_accounts, ATTACKER_DEPOSIT))
        .succeeds()
        .has_tokens(ATTACKER_SHARE, 0)
        .has_tokens(w.vault_usdc, DONATION);
    assert_eq!(
        value_in_usdc(test, ATTACKER_USDC, ATTACKER_ASSET),
        ATTACKER_DEPOSIT
    );

    let fund = test.read::<Fund>(w.fund);
    assert_eq!(u64::from(fund.total_shares), 0, "total_shares");
    assert_eq!(u64::from(fund.usdc_holdings), 0, "usdc_holdings");
}

/// A deposit leg that spends USDC and buys none of its asset would mint shares
/// against no recorded value, and every later deposit would then divide by a
/// zero NAV. At 250 USDC minor units per asset minor unit, a one-minor-unit
/// deposit buys nothing, so it must be refused.
#[quasar_test]
fn deposit_rejects_leg_that_buys_nothing(test: &mut Test) {
    let w = setup_deposit(test);
    add_pyth_feed(test, PYTH_PRICE, NOW);
    add_depositor(
        test,
        &w,
        ATTACKER,
        [ATTACKER_USDC, ATTACKER_SHARE, ATTACKER_ASSET],
        1,
    );

    test.send(deposit_as(
        &w,
        ATTACKER,
        ATTACKER_USDC,
        ATTACKER_SHARE,
        1,
        0,
    ))
    .fails_with(FundError::DepositTooSmall);

    let fund = test.read::<Fund>(w.fund);
    assert_eq!(u64::from(fund.total_shares), 0, "total_shares");
}

// The two-asset standard fund the rebalance tests share, the same basket as the
// Anchor suite: TSLAx at index 0 (40%) and NVDAx at index 1 (60%), both bought
// and sold through the router.
const TSLA_MINT: Pubkey = Pubkey::new_from_array([20; 32]);
const NVDA_MINT: Pubkey = Pubkey::new_from_array([21; 32]);
const TSLA_FEED: Pubkey = Pubkey::new_from_array([22; 32]);
const NVDA_FEED: Pubkey = Pubkey::new_from_array([23; 32]);
const THIRD_MINT: Pubkey = Pubkey::new_from_array([24; 32]);
const THIRD_FEED: Pubkey = Pubkey::new_from_array([25; 32]);

const TSLA_PRICE: i64 = 25_000_000_000; // $250
const NVDA_PRICE: i64 = 18_000_000_000; // $180
const TSLA_RATE: u64 = 250_000_000; // router USDC minor units per whole token
const NVDA_RATE: u64 = 180_000_000;

fn registry_pda(test: &Test) -> Pubkey {
    test.derive_pda(Registry::seeds(&AUTHORITY))
}

fn fund_pda(test: &Test) -> Pubkey {
    test.derive_pda(Fund::seeds(FUND_INDEX))
}

fn asset_config_pda(test: &Test, index: u8) -> Pubkey {
    test.derive_pda(AssetConfig::seeds(&fund_pda(test), index))
}

fn asset_vault_pda(test: &Test, index: u8) -> Pubkey {
    test.derive_pda(AssetVaultPda::seeds(&fund_pda(test), index))
}

fn usdc_vault_pda(test: &Test) -> Pubkey {
    test.derive_pda(UsdcVaultPda::seeds(&fund_pda(test)))
}

fn approve_asset(test: &mut Test, mint: Pubkey, feed: Pubkey) {
    test.send(ApproveAssetInstruction {
        authority: AUTHORITY,
        asset_mint: mint,
        price_feed: feed,
    })
    .succeeds();
}

/// Mints, router (config + rates + treasury), Pyth feeds, and a registry with
/// TSLAx and NVDAx approved. Does not create the fund.
fn setup_full(test: &mut Test) {
    setup_with_tsla_decimals(test, DECIMALS);
}

/// `setup_full`, with TSLAx minted at `tsla_decimals` instead of six.
fn setup_with_tsla_decimals(test: &mut Test, tsla_decimals: u8) {
    load_router(test);
    test.add(Wallet::new().at(AUTHORITY));
    test.add(Wallet::new().at(MANAGER));
    test.add(Mint::new(AUTHORITY).at(USDC_MINT).decimals(DECIMALS));
    // The router config account is every asset's mint authority, so the router
    // can mint it on swap.
    test.add(
        Mint::new(router_config_pda())
            .at(TSLA_MINT)
            .decimals(tsla_decimals),
    );
    test.add(
        Mint::new(router_config_pda())
            .at(NVDA_MINT)
            .decimals(DECIMALS),
    );
    write_price_feed(test, TSLA_FEED, TSLA_PRICE, -8, NOW, 1);
    write_price_feed(test, NVDA_FEED, NVDA_PRICE, -8, NOW, 1);

    initialize_router(test);
    set_router_rate(test, TSLA_MINT, TSLA_RATE);
    set_router_rate(test, NVDA_MINT, NVDA_RATE);

    test.send(InitializeRegistryInstruction {
        authority: AUTHORITY,
    })
    .succeeds();
    approve_asset(test, TSLA_MINT, TSLA_FEED);
    approve_asset(test, NVDA_MINT, NVDA_FEED);
}

fn initialize_fund_instruction(
    test: &Test,
    rebalance_threshold_bps: u16,
) -> InitializeFundInstruction {
    InitializeFundInstruction {
        manager: MANAGER,
        usdc_mint: USDC_MINT,
        registry: registry_pda(test),
        index: FUND_INDEX,
        fee_bps: FEE_BPS,
        max_slippage_bps: MAX_SLIPPAGE_BPS,
        rebalance_threshold_bps,
        swap_router: router_id(),
    }
}

fn init_fund(test: &mut Test) {
    let ix = initialize_fund_instruction(test, REBALANCE_THRESHOLD_BPS);
    test.send(ix).succeeds();
}

fn add_asset(test: &mut Test, index: u8, mint: Pubkey, weight_bps: u16) {
    let registry = registry_pda(test);
    test.send(AddAssetInstruction {
        manager: MANAGER,
        fund_index_seed: FUND_INDEX,
        registry,
        asset_mint: mint,
        fund_asset_count_seed: index,
        weight_bps,
    })
    .succeeds();
}

/// init fund + add TSLAx (index 0, 40%) + NVDAx (index 1, 60%).
fn standard_fund(test: &mut Test) {
    init_fund(test);
    add_asset(test, 0, TSLA_MINT, 4000);
    add_asset(test, 1, NVDA_MINT, 6000);
}

/// One asset's deposit remaining_accounts, in the order the handler reads:
/// [asset_config, vault, mint, rate, price_feed]. Deposit deploys into the asset,
/// so vault and mint must be writable.
fn asset_deposit_metas(test: &Test, index: u8, mint: Pubkey, feed: Pubkey) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new_readonly(asset_config_pda(test, index), false),
        AccountMeta::new(asset_vault_pda(test, index), false),
        AccountMeta::new(mint, false),
        AccountMeta::new_readonly(router_rate_pda(&mint), false),
        AccountMeta::new_readonly(feed, false),
    ]
}

/// remaining_accounts for a deposit into the two-asset standard fund.
fn deposit_remaining(test: &Test) -> Vec<AccountMeta> {
    let mut metas = asset_deposit_metas(test, 0, TSLA_MINT, TSLA_FEED);
    metas.extend(asset_deposit_metas(test, 1, NVDA_MINT, NVDA_FEED));
    metas
}

/// A depositor's wallet and token accounts in the standard fund's mints.
struct User {
    owner: Pubkey,
    usdc: Pubkey,
    share: Pubkey,
    tsla: Pubkey,
    nvda: Pubkey,
}

fn fund_user(test: &mut Test, usdc_amount: u64) -> User {
    let owner = test.add(Wallet::new());
    let share_mint = test.derive_pda(ShareMintPda::seeds(&fund_pda(test)));
    User {
        owner,
        usdc: test.add(TokenAccount::new(USDC_MINT, owner).amount(usdc_amount)),
        share: test.add(TokenAccount::new(share_mint, owner)),
        tsla: test.add(TokenAccount::new(TSLA_MINT, owner)),
        nvda: test.add(TokenAccount::new(NVDA_MINT, owner)),
    }
}

fn deposit_instruction(
    user: &User,
    usdc_amount: u64,
    remaining_accounts: Vec<AccountMeta>,
) -> DepositInstruction {
    DepositInstruction {
        depositor: user.owner,
        fund_index_seed: FUND_INDEX,
        usdc_mint: USDC_MINT,
        depositor_usdc_account: user.usdc,
        depositor_share_account: user.share,
        router_config: router_config_pda(),
        router_usdc_treasury: router_treasury_pda(),
        swap_router_program: router_id(),
        usdc_amount,
        minimum_shares: 1,
        remaining_accounts,
    }
}

fn do_deposit(test: &mut Test, user: &User, usdc_amount: u64) {
    let ix = deposit_instruction(user, usdc_amount, deposit_remaining(test));
    test.send(ix).succeeds();
}

/// Burn `shares` of the standard fund and take the payout in kind.
fn do_withdraw(test: &mut Test, user: &User, shares: u64) {
    let mut remaining_accounts = Vec::new();
    for (index, mint, user_asset) in [(0, TSLA_MINT, user.tsla), (1, NVDA_MINT, user.nvda)] {
        remaining_accounts.extend([
            AccountMeta::new_readonly(asset_config_pda(test, index), false),
            AccountMeta::new(asset_vault_pda(test, index), false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(user_asset, false),
        ]);
    }
    test.send(WithdrawInstruction {
        user: user.owner,
        fund_index_seed: FUND_INDEX,
        usdc_mint: USDC_MINT,
        user_share_account: user.share,
        user_usdc_account: user.usdc,
        shares_to_burn: shares,
        min_usdc_out: 0,
        remaining_accounts,
    })
    .succeeds();
}

/// Move NVDAx's price: rewrite its Pyth feed and update the router rate to match.
fn set_nvda_price(test: &mut Test, price: i64, rate: u64) {
    write_price_feed(test, NVDA_FEED, price, -8, NOW, 1);
    set_router_rate(test, NVDA_MINT, rate);
}

fn set_weight(test: &mut Test, index: u8, weight_bps: u16) {
    test.send(SetWeightInstruction {
        manager: MANAGER,
        fund_index_seed: FUND_INDEX,
        asset_config_index_seed: index,
        weight_bps,
    })
    .succeeds();
}

/// The fund's recorded USDC and per-asset holdings.
fn read_holdings(test: &Test) -> (u64, [u64; MAX_ASSETS as usize]) {
    let fund = test.read::<Fund>(fund_pda(test));
    (
        u64::from(fund.usdc_holdings),
        read_asset_holdings(&fund.asset_holdings),
    )
}

/// Recorded holdings must equal the vaults' token balances whenever nothing has
/// been donated: a mismatch means a handler moved tokens without recording it.
fn assert_holdings_match_vaults(test: &Test) {
    let (usdc_holdings, asset_holdings) = read_holdings(test);
    assert_eq!(
        usdc_holdings,
        test.tokens(usdc_vault_pda(test)),
        "recorded USDC matches the USDC vault"
    );
    assert_eq!(
        asset_holdings[0],
        test.tokens(asset_vault_pda(test, 0)),
        "recorded TSLAx matches the TSLAx vault"
    );
    assert_eq!(
        asset_holdings[1],
        test.tokens(asset_vault_pda(test, 1)),
        "recorded NVDAx matches the NVDAx vault"
    );
}

/// A rebalance of the fund, signed by `caller`: the program computes the trade,
/// so the caller names only the pair. The remaining accounts are the same five
/// per asset that a deposit takes.
fn rebalance_instruction_with(
    caller: Pubkey,
    sell_index: u8,
    buy_index: u8,
    remaining_accounts: Vec<AccountMeta>,
) -> RebalanceInstruction {
    RebalanceInstruction {
        caller,
        fund_index_seed: FUND_INDEX,
        usdc_mint: USDC_MINT,
        router_config: router_config_pda(),
        router_usdc_treasury: router_treasury_pda(),
        swap_router_program: router_id(),
        sell_index,
        buy_index,
        remaining_accounts,
    }
}

/// Rebalance the standard fund, signed by a fresh wallet that is neither the
/// manager nor a depositor.
fn try_rebalance(test: &mut Test, sell_index: u8, buy_index: u8) -> Outcome {
    let stranger = test.add(Wallet::new());
    let ix = rebalance_instruction_with(stranger, sell_index, buy_index, deposit_remaining(test));
    test.send(ix)
}

fn do_rebalance(test: &mut Test, sell_index: u8, buy_index: u8) {
    try_rebalance(test, sell_index, buy_index).succeeds();
}

/// Transfer `amount` from a token account straight into a fund vault with an
/// ordinary token transfer (SPL Token `Transfer`, instruction 3), never calling
/// a handler.
fn donate_token(test: &mut Test, owner: Pubkey, from: Pubkey, vault: Pubkey, amount: u64) {
    let mut transfer_data = vec![3u8];
    transfer_data.extend_from_slice(&amount.to_le_bytes());
    test.send(Instruction {
        program_id: SPL_TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(from, false),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(owner, true),
        ],
        data: transfer_data,
    })
    .succeeds();
}

#[quasar_test]
fn test_rebalance(test: &mut Test) {
    setup_full(test);
    standard_fund(test);

    // Alice deposits 900 USDC, auto-deployed to 1.44 TSLAx + 3.0 NVDAx (exactly 40/60).
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);

    // NVDAx rises 180 -> 200, pushing the basket to 37.5 / 62.5 by value.
    set_nvda_price(test, 20_000_000_000, 200_000_000);

    // A stranger, neither the manager nor a depositor, rebalances. The program
    // computes the trade back to 40/60: NVDAx is $24 over its $576 target and
    // TSLAx $24 under its $384 target, so it sells 0.12 NVDAx for 24 USDC and
    // buys 0.096 TSLAx with it.
    do_rebalance(test, 1, 0);

    // 1.44 + 0.096 = 1.536 TSLAx; 3.0 - 0.12 = 2.88 NVDAx. Now 384 / 576 = 40 / 60.
    // The USDC vault nets to zero across the two legs.
    assert_eq!(test.tokens(asset_vault_pda(test, 0)), 1_536_000);
    assert_eq!(test.tokens(asset_vault_pda(test, 1)), 2_880_000);
    assert_eq!(test.tokens(usdc_vault_pda(test)), 0);
    assert_holdings_match_vaults(test);
}

/// A fund sitting at its target weights has nothing to rebalance, in either
/// direction, so nobody can trade it.
#[quasar_test]
fn test_rebalance_refuses_fund_at_target(test: &mut Test) {
    setup_full(test);
    standard_fund(test);
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);

    // A fund at target must not sell NVDAx, nor TSLAx.
    try_rebalance(test, 1, 0).fails_with(FundError::DriftBelowThreshold);
    try_rebalance(test, 0, 1).fails_with(FundError::DriftBelowThreshold);
}

/// Drift smaller than the fund's threshold is not worth the slippage of a
/// trade, so the rebalance is refused.
#[quasar_test]
fn test_rebalance_refuses_drift_below_threshold(test: &mut Test) {
    setup_full(test);
    standard_fund(test);
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);

    // NVDAx 180 -> 185: 555 of a 915 fund is 60.66%, 0.66 points over its 60%
    // target and under the two-point threshold.
    set_nvda_price(test, 18_500_000_000, 185_000_000);
    try_rebalance(test, 1, 0).fails_with(FundError::DriftBelowThreshold);
    assert_eq!(read_holdings(test).1[1], 3_000_000);
}

/// Churn: once a rebalance has restored the weights, calling it again, in
/// either direction, finds nothing to do. Nobody can trade the fund back and
/// forth to bleed it through slippage.
#[quasar_test]
fn test_rebalance_cannot_churn(test: &mut Test) {
    setup_full(test);
    standard_fund(test);
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);

    set_nvda_price(test, 20_000_000_000, 200_000_000);
    do_rebalance(test, 1, 0);
    let after_first = read_holdings(test);

    // A second rebalance must find nothing to sell, and rebalancing back the
    // other way must be refused.
    try_rebalance(test, 1, 0).fails_with(FundError::DriftBelowThreshold);
    try_rebalance(test, 0, 1).fails_with(FundError::DriftBelowThreshold);
    assert_eq!(read_holdings(test), after_first);
}

/// A third basket asset the router can trade: its own mint (the router holds
/// the mint authority), router rate, Pyth feed, and registry approval.
fn create_routable_asset(test: &mut Test, price: i64, rate: u64) {
    test.add(
        Mint::new(router_config_pda())
            .at(THIRD_MINT)
            .decimals(DECIMALS),
    );
    set_router_rate(test, THIRD_MINT, rate);
    write_price_feed(test, THIRD_FEED, price, -8, NOW, 1);
    approve_asset(test, THIRD_MINT, THIRD_FEED);
}

/// The buy side must be under its target: a rebalance cannot pour the sale into
/// an asset already at or over its weight. Two assets cannot show this, since
/// when one is over its target the other is under by the same amount, so the
/// fund here holds a third, at 40/40/20.
#[quasar_test]
fn test_rebalance_refuses_buying_overweight_asset(test: &mut Test) {
    setup_full(test);
    init_fund(test);
    create_routable_asset(test, 10_000_000_000, 100_000_000); // $100
    add_asset(test, 0, TSLA_MINT, 4000);
    add_asset(test, 1, NVDA_MINT, 4000);
    add_asset(test, 2, THIRD_MINT, 2000);

    let mut remaining = deposit_remaining(test);
    remaining.extend(asset_deposit_metas(test, 2, THIRD_MINT, THIRD_FEED));
    let alice = fund_user(test, 1_000_000_000);
    let ix = deposit_instruction(&alice, 1_000_000_000, remaining.clone());
    test.send(ix).succeeds();

    // NVDAx and the third asset both rise 30%. NVDAx is about $520 of $1,180
    // against a $472 target; the third asset $260 against $236; TSLAx $400
    // against $472. NVDAx is over by more than the threshold, the third asset
    // is over too, and only TSLAx is under.
    set_nvda_price(test, 23_400_000_000, 234_000_000);
    write_price_feed(test, THIRD_FEED, 13_000_000_000, -8, NOW, 1);
    set_router_rate(test, THIRD_MINT, 130_000_000);

    // An over-weight asset must not be bought.
    let stranger = test.add(Wallet::new());
    test.send(rebalance_instruction_with(
        stranger,
        1,
        2,
        remaining.clone(),
    ))
    .fails_with(FundError::NotUnderweight);

    // Naming the same asset on both sides is refused outright.
    test.send(rebalance_instruction_with(
        stranger,
        1,
        1,
        remaining.clone(),
    ))
    .fails_with(FundError::SameMint);

    // Selling NVDAx into TSLAx, the asset that is under, goes through.
    test.send(rebalance_instruction_with(stranger, 1, 0, remaining))
        .succeeds();
    assert!(read_holdings(test).1[0] > 1_600_000);
}

/// A rebalance values every asset, so it needs every asset's accounts, as a
/// deposit does.
#[quasar_test]
fn test_rebalance_rejects_incomplete_assets(test: &mut Test) {
    setup_full(test);
    standard_fund(test);
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);
    set_nvda_price(test, 20_000_000_000, 200_000_000);

    // A rebalance missing an asset must revert.
    let stranger = test.add(Wallet::new());
    let mut remaining = deposit_remaining(test);
    remaining.truncate(remaining.len() - 5);
    test.send(rebalance_instruction_with(stranger, 1, 0, remaining))
        .fails_with(FundError::IncompleteAssetAccounts);
}

/// Setting a weight to zero retires an asset, and a rebalance then sells all of
/// it, however little is left: a retired asset needs no threshold, because
/// selling it to zero is a trade that can happen only once.
#[quasar_test]
fn test_rebalance_sells_retired_asset(test: &mut Test) {
    setup_full(test);
    standard_fund(test);
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);

    // Maria retires NVDAx and moves its weight to TSLAx.
    set_weight(test, 1, 0);
    set_weight(test, 0, 10_000);

    // NVDAx falls to $1, so the 3 NVDAx left are $3 of a $363 fund: under one
    // percentage point, below the two-point threshold.
    set_nvda_price(test, 100_000_000, 1_000_000);
    do_rebalance(test, 1, 0);

    // All 3 NVDAx sold for 3 USDC, which bought 0.012 TSLAx.
    let (usdc_holdings, asset_holdings) = read_holdings(test);
    assert_eq!(asset_holdings[1], 0);
    assert_eq!(asset_holdings[0], 1_452_000);
    assert_eq!(usdc_holdings, 0);
    assert_holdings_match_vaults(test);

    // A retired asset already sold has nothing left to sell.
    try_rebalance(test, 1, 0).fails_with(FundError::DriftBelowThreshold);
}

/// Donations can neither force a rebalance nor pay for one. Donated NVDAx is not
/// recorded, so it cannot push NVDAx over its target; donated USDC is never
/// spent, because the buy leg invests only what the sale brought in.
#[quasar_test]
fn test_rebalance_ignores_donations(test: &mut Test) {
    setup_full(test);
    standard_fund(test);
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);

    // The donor gets NVDAx the way anyone can: deposit, then withdraw in kind.
    let donor = fund_user(test, 200_000_000);
    do_deposit(test, &donor, 100_000_000);
    let donor_shares = test.tokens(donor.share);
    do_withdraw(test, &donor, donor_shares);
    let donated_nvda = test.tokens(donor.nvda);
    assert!(donated_nvda > 0);
    let (vault_nvda, vault_usdc) = (asset_vault_pda(test, 1), usdc_vault_pda(test));
    donate_token(test, donor.owner, donor.nvda, vault_nvda, donated_nvda);
    donate_token(test, donor.owner, donor.usdc, vault_usdc, 100_000_000);

    // Counted, the donated NVDAx would put NVDAx far over its target. Recorded
    // holdings are still 40/60, so there is nothing to rebalance.
    try_rebalance(test, 1, 0).fails_with(FundError::DriftBelowThreshold);

    // A real price move does drift the fund, and the rebalance spends only
    // what its sale brought in: the donated USDC is still in the vault, outside
    // the recorded holdings.
    set_nvda_price(test, 20_000_000_000, 200_000_000);
    do_rebalance(test, 1, 0);
    let (usdc_holdings, asset_holdings) = read_holdings(test);
    assert_eq!(usdc_holdings, 0);
    assert_eq!(test.tokens(vault_usdc), 100_000_000);
    assert_eq!(asset_holdings[1] + donated_nvda, test.tokens(vault_nvda));
}

/// The threshold is fixed at creation, within bounds: a manager cannot set it
/// near zero, where the fund would trade on every small move, or so high that
/// the target weights stop describing the fund.
#[quasar_test]
fn test_initialize_rejects_threshold_out_of_range(test: &mut Test) {
    setup_full(test);
    for threshold in [
        0,
        MIN_REBALANCE_THRESHOLD_BPS - 1,
        MAX_REBALANCE_THRESHOLD_BPS + 1,
    ] {
        // A threshold outside the bounds must be rejected.
        let ix = initialize_fund_instruction(test, threshold);
        test.send(ix)
            .fails_with(FundError::RebalanceThresholdOutOfRange);
    }
    init_fund(test);
    let fund = test.read::<Fund>(fund_pda(test));
    assert_eq!(
        u16::from(fund.rebalance_threshold_bps),
        REBALANCE_THRESHOLD_BPS
    );
}

/// Valuation scales by each asset's decimals and each feed's exponent. TSLAx
/// here has eight decimals and a Pyth equity feed with exponent -5, while USDC
/// and NVDAx keep six decimals and NVDAx's feed keeps -8. Assuming six decimals
/// and -8 would value Alice's 1.44 TSLAx at $360 * 100 * 1,000, and Bob's
/// deposit would buy almost no shares. Scaled correctly, every figure matches
/// the six-decimal story.
#[quasar_test]
fn test_valuation_scales_by_decimals_and_exponent(test: &mut Test) {
    setup_with_tsla_decimals(test, 8);
    write_price_feed(test, TSLA_FEED, 25_000_000, -5, NOW, 1); // $250
    standard_fund(test);
    assert_eq!(
        test.read::<AssetConfig>(asset_config_pda(test, 0)).decimals,
        8
    );
    assert_eq!(
        test.read::<AssetConfig>(asset_config_pda(test, 1)).decimals,
        6
    );

    // Alice's 900 USDC deploys to 1.44 TSLAx (eight decimals) and 3 NVDAx.
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);
    assert_eq!(test.tokens(alice.share), 900_000_000);
    assert_eq!(read_holdings(test).1[0], 144_000_000);
    assert_eq!(read_holdings(test).1[1], 3_000_000);

    // NVDAx to $200. The rebalance computes its trade in the same units: 0.12
    // NVDAx sold for 24 USDC, which buys 0.096 TSLAx, back to 40/60.
    set_nvda_price(test, 20_000_000_000, 200_000_000);
    do_rebalance(test, 1, 0);
    assert_eq!(read_holdings(test).1[0], 153_600_000);
    assert_eq!(read_holdings(test).1[1], 2_880_000);

    // The fund is worth $960, so Bob's 480 USDC buys 450 shares.
    let bob = fund_user(test, 480_000_000);
    do_deposit(test, &bob, 480_000_000);
    assert_eq!(test.tokens(bob.share), 450_000_000);
    assert_eq!(read_holdings(test).1[0], 230_400_000);
    assert_eq!(read_holdings(test).1[1], 4_320_000);
    assert_holdings_match_vaults(test);
}

/// A price the oracle is unsure of is not traded on. With NVDAx's confidence
/// interval at 2% of its price, past the 1% limit, deposit and rebalance both
/// refuse, but withdraw still pays out in kind: it reads no price, so investors
/// can always leave. A band of exactly 1% is accepted.
#[quasar_test]
fn test_wide_confidence_price_rejected(test: &mut Test) {
    setup_full(test);
    standard_fund(test);

    // Alice deposits 900 USDC: 1.44 TSLAx + 3.0 NVDAx at 40/60.
    let alice = fund_user(test, 900_000_000);
    do_deposit(test, &alice, 900_000_000);

    // NVDAx rises to $200, so the fund has drifted and needs a rebalance. Then
    // its feed reports a $4 confidence interval: 2% of the price.
    set_nvda_price(test, 20_000_000_000, 200_000_000);
    write_price_feed_with_confidence(test, NVDA_FEED, 20_000_000_000, 400_000_000, -8, NOW, 1);

    let bob = fund_user(test, 480_000_000);
    let ix = deposit_instruction(&bob, 480_000_000, deposit_remaining(test));
    test.send(ix).fails_with(FundError::OracleConfidenceTooWide);
    try_rebalance(test, 1, 0).fails_with(FundError::OracleConfidenceTooWide);

    // Withdraw reads no price: Alice takes half her shares out in kind.
    do_withdraw(test, &alice, 450_000_000);
    assert_eq!(test.tokens(alice.tsla), 720_000);
    assert_eq!(test.tokens(alice.nvda), 1_500_000);

    // A $2 interval is exactly 1% of the price, which is accepted: the
    // rebalance sells 0.06 NVDAx for 12 USDC and buys 0.048 TSLAx.
    write_price_feed_with_confidence(test, NVDA_FEED, 20_000_000_000, 200_000_000, -8, NOW, 1);
    do_rebalance(test, 1, 0);
    let (_, holdings) = read_holdings(test);
    assert_eq!(holdings[0], 768_000);
    assert_eq!(holdings[1], 1_440_000);
    assert_holdings_match_vaults(test);
}
