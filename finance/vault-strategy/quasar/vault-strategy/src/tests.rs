//! quasar-test integration tests. `strategy_setup` drives the manager-side
//! setup (registry, approve asset, strategy, add asset) and asserts state.
//! `deposit` is a two-program test: it loads the mock swap router too, wires
//! up rates and a Pyth-shaped price feed, and deposits, checking that the
//! deposit is priced 1:1 on the first deposit and deployed into the basket
//! through the router CPI. `deposit_rejects_price_from_before_a_restart`
//! reuses that setup to show a pre-restart price is refused.

use {
    crate::{
        cpi::{
            AddAssetInstruction, ApproveAssetInstruction, DepositInstruction,
            InitializeRegistryInstruction, InitializeStrategyInstruction,
        },
        errors::VaultError,
        state::{AssetConfig, AssetVaultPda, Registry, ShareMintPda, Strategy, UsdcVaultPda},
    },
    quasar_test::prelude::*,
};

const DECIMALS: u8 = 6;
const FEE_BPS: u16 = 100;
const MAX_SLIPPAGE_BPS: u16 = 100;

// Router program (loaded for the deposit test).
const ROUTER_ID_STR: &str = "SWPR8Rk3aq3DrDGLdaANq7xCMnXoUFUJWJJmCWxc8Jm";
const RATE: u64 = 250; // USDC base units per asset base unit
const NOW: i64 = 1_000; // fixed clock for the deposit test
const STRATEGY_INDEX: u64 = 0;

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

// A Pyth PriceUpdateV2-shaped account: `price` (i64) at offset 73,
// `publish_time` (i64) at offset 93, `posted_slot` (u64) at offset 125. The
// program reads only those three fields. Posted at slot 1.
fn add_pyth_feed(test: &mut Test, price: i64, publish_time: i64) {
    add_pyth_feed_posted_at(test, price, publish_time, 1);
}

// The same feed, as if Pyth posted it in `posted_slot`.
fn add_pyth_feed_posted_at(test: &mut Test, price: i64, publish_time: i64, posted_slot: u64) {
    let mut data = vec![0u8; 200];
    data[73..81].copy_from_slice(&price.to_le_bytes());
    data[93..101].copy_from_slice(&publish_time.to_le_bytes());
    data[125..133].copy_from_slice(&posted_slot.to_le_bytes());
    test.set_account(Account::new(PRICE_FEED, FEED_OWNER, 1_000_000, data));
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

/// The strategy-side PDAs the assertions read.
struct Pdas {
    strategy: Pubkey,
    asset_config: Pubkey,
    vault_asset: Pubkey,
    vault_usdc: Pubkey,
    share_mint: Pubkey,
}

fn pdas(test: &Test) -> Pdas {
    let strategy = test.derive_pda(Strategy::seeds(STRATEGY_INDEX));
    Pdas {
        strategy,
        asset_config: test.derive_pda(AssetConfig::seeds(&strategy, 0)),
        vault_asset: test.derive_pda(AssetVaultPda::seeds(&strategy, 0)),
        vault_usdc: test.derive_pda(UsdcVaultPda::seeds(&strategy)),
        share_mint: test.derive_pda(ShareMintPda::seeds(&strategy)),
    }
}

/// Registry + approved asset + strategy + one basket asset at 100% weight.
fn setup_strategy(test: &mut Test, asset_mint_authority: Pubkey) {
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
    test.send(InitializeStrategyInstruction {
        manager: MANAGER,
        usdc_mint: USDC_MINT,
        registry,
        index: STRATEGY_INDEX,
        fee_bps: FEE_BPS,
        max_slippage_bps: MAX_SLIPPAGE_BPS,
        swap_router: router_id(),
    })
    .succeeds();
    test.send(AddAssetInstruction {
        manager: MANAGER,
        strategy_index_seed: STRATEGY_INDEX,
        registry,
        asset_mint: ASSET_MINT,
        strategy_asset_count_seed: 0,
        weight_bps: 10_000,
    })
    .succeeds();
}

#[quasar_test]
fn strategy_setup_records_the_basket(test: &mut Test) {
    setup_strategy(test, AUTHORITY);
    let w = pdas(test);

    let strategy = test.read::<Strategy>(w.strategy);
    assert_eq!(strategy.asset_count, 1, "asset_count");
    assert_eq!(
        u16::from(strategy.total_weight_bps),
        10_000,
        "total_weight_bps"
    );

    let asset_config = test.read::<AssetConfig>(w.asset_config);
    assert_eq!(u16::from(asset_config.weight_bps), 10_000, "weight_bps");
    assert_eq!(asset_config.mint, ASSET_MINT, "asset mint");
    assert_eq!(
        asset_config.price_feed, PRICE_FEED,
        "price feed copied from registry"
    );
}

// Asset priced 250 USDC/token: Pyth price = 250 * 10^8 so
// asset_value = amount * price / 10^8 gives 250 USDC per token base unit.
const PYTH_PRICE: i64 = 250 * 100_000_000;
const DEPOSIT: u64 = 1_000;

/// Load the router, set up a single-asset strategy, fund the depositor, and
/// initialize the router with the asset's rate. Leaves the Pyth feed to the
/// caller.
fn setup_deposit(test: &mut Test) -> Pdas {
    // Runtime read (NOT include_bytes!): quasar-test auto-loads only this
    // program's .so; the sibling router program is added explicitly.
    let router_elf =
        std::fs::read("../mock-swap-router/target/deploy/quasar_mock_swap_router.so").unwrap();
    test.add(Program::new(router_id(), &router_elf));
    test.warp_to_timestamp(NOW);

    // The router config account is the asset mint's mint authority, so the
    // router can mint it on swap.
    setup_strategy(test, router_config_pda());
    let w = pdas(test);

    test.add(Wallet::new().at(DEPOSITOR));

    // Depositor token accounts (share account created up front).
    test.add(
        TokenAccount::new(USDC_MINT, DEPOSITOR)
            .at(DEPOSITOR_USDC)
            .amount(DEPOSIT),
    );
    test.add(TokenAccount::new(w.share_mint, DEPOSITOR).at(DEPOSITOR_SHARE));

    // Initialize the router and set the asset's rate (hand-built: the router's
    // builders live in the sibling crate).
    let rent_id: Pubkey = "SysvarRent111111111111111111111111111111111"
        .parse()
        .unwrap();
    test.send(Instruction {
        program_id: router_id(),
        accounts: vec![
            AccountMeta::new(AUTHORITY, true),
            AccountMeta::new_readonly(USDC_MINT, false),
            AccountMeta::new(router_config_pda(), false),
            AccountMeta::new_readonly(rent_id, false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: vec![0u8],
    })
    .succeeds();
    let mut set_rate_data = vec![1u8];
    set_rate_data.extend_from_slice(&RATE.to_le_bytes());
    test.send(Instruction {
        program_id: router_id(),
        accounts: vec![
            AccountMeta::new(AUTHORITY, true),
            AccountMeta::new_readonly(router_config_pda(), false),
            AccountMeta::new_readonly(ASSET_MINT, false),
            AccountMeta::new_readonly(USDC_MINT, false),
            AccountMeta::new(router_rate_pda(&ASSET_MINT), false),
            AccountMeta::new(router_treasury_pda(), false),
            AccountMeta::new_readonly(rent_id, false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: set_rate_data,
    })
    .succeeds();

    w
}

/// Deposit `DEPOSIT` USDC: declared accounts, then remaining accounts per
/// basket asset (asset_config, vault_asset, asset_mint, asset_rate, price_feed).
fn send_deposit(test: &mut Test, w: &Pdas) -> Outcome {
    test.send(DepositInstruction {
        depositor: DEPOSITOR,
        strategy_index_seed: STRATEGY_INDEX,
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

/// Two-program deposit: set up the router + a single-asset strategy, then
/// deposit USDC. The first deposit mints shares 1:1 and deploys the whole
/// amount into the asset through the router CPI.
#[quasar_test]
fn deposit_mints_shares_and_deploys_into_the_basket(test: &mut Test) {
    let w = setup_deposit(test);
    add_pyth_feed(test, PYTH_PRICE, NOW);

    const ASSET_OUT: u64 = DEPOSIT / RATE; // 4

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
/// staleness check. The vault must reject any price posted at or before the
/// restart slot until Pyth posts again.
#[quasar_test]
fn deposit_rejects_price_from_before_a_restart(test: &mut Test) {
    let w = setup_deposit(test);

    // The feed is posted at slot 1 and stamped `NOW`, so it is fresh by the
    // 60-second bound, but the cluster restarted at slot 3: only the restart
    // check can catch it.
    add_pyth_feed(test, PYTH_PRICE, NOW);
    set_last_restart_slot(test, 3);
    send_deposit(test, &w).fails_with(VaultError::PricePredatesRestart);

    // Pyth posting again after the restart (slot 4) reopens the vault.
    add_pyth_feed_posted_at(test, PYTH_PRICE, NOW, 4);
    send_deposit(test, &w)
        .succeeds()
        .has_tokens(DEPOSITOR_SHARE, DEPOSIT);
}
