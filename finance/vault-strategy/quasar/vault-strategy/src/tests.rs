//! quasar-test integration tests. `strategy_setup` drives the manager-side
//! setup (registry, approve asset, strategy, add asset) and asserts state.
//! `deposit` is a two-program test: it loads the mock swap router too, wires
//! up rates and a Pyth-shaped price feed, and deposits, checking that the
//! deposit is priced 1:1 on the first deposit and deployed into the basket
//! through the router CPI.

use {
    crate::{
        cpi::{
            AddAssetInstruction, ApproveAssetInstruction, DepositInstruction,
            InitializeRegistryInstruction, InitializeStrategyInstruction,
        },
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
fn router_authority_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"router_authority"], &router_id()).0
}
fn router_treasury_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"treasury"], &router_id()).0
}
fn router_rate_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"rate", mint.as_ref()], &router_id()).0
}

// A Pyth PriceUpdateV2-shaped account: `price` (i64) at offset 73,
// `publish_time` (i64) at offset 93. The program reads only those two fields.
fn add_pyth_feed(test: &mut Test, price: i64, publish_time: i64) {
    let mut data = vec![0u8; 200];
    data[73..81].copy_from_slice(&price.to_le_bytes());
    data[93..101].copy_from_slice(&publish_time.to_le_bytes());
    test.set_account(Account::new(PRICE_FEED, FEED_OWNER, 1_000_000, data));
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

/// Two-program deposit: set up the router + a single-asset strategy, then
/// deposit USDC. The first deposit mints shares 1:1 and deploys the whole
/// amount into the asset through the router CPI.
#[quasar_test]
fn deposit_mints_shares_and_deploys_into_the_basket(test: &mut Test) {
    // Runtime read (NOT include_bytes!): quasar-test auto-loads only this
    // program's .so; the sibling router program is added explicitly.
    let router_elf =
        std::fs::read("../mock-swap-router/target/deploy/quasar_mock_swap_router.so").unwrap();
    test.add(Program::new(router_id(), &router_elf));
    test.warp_to_timestamp(NOW);

    let r_authority = router_authority_pda();
    // The asset mint is minted by the router authority.
    setup_strategy(test, r_authority);
    let w = pdas(test);

    test.add(Wallet::new().at(DEPOSITOR));

    // Asset priced 250 USDC/token: Pyth price = 250 * 10^8 so
    // asset_value = amount * price / 10^8 gives 250 USDC per token base unit.
    let pyth_price: i64 = 250 * 100_000_000;
    add_pyth_feed(test, pyth_price, NOW);

    const DEPOSIT: u64 = 1_000;
    const ASSET_OUT: u64 = DEPOSIT / RATE; // 4

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
            AccountMeta::new_readonly(r_authority, false),
            AccountMeta::new(router_treasury_pda(), false),
            AccountMeta::new_readonly(rent_id, false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: set_rate_data,
    })
    .succeeds();

    // Deposit: declared accounts, then remaining accounts per basket asset
    // (asset_config, vault_asset, asset_mint, asset_rate, price_feed).
    test.send(DepositInstruction {
        depositor: DEPOSITOR,
        strategy_index_seed: STRATEGY_INDEX,
        usdc_mint: USDC_MINT,
        depositor_usdc_account: DEPOSITOR_USDC,
        depositor_share_account: DEPOSITOR_SHARE,
        router_config: router_config_pda(),
        router_usdc_treasury: router_treasury_pda(),
        router_authority: r_authority,
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
