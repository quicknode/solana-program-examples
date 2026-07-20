//! QuasarSVM integration tests. `test_strategy_setup` drives the manager-side
//! setup (registry, approve asset, strategy, add asset) and asserts state.
//! `test_deposit` is a two-program test: it loads the mock swap router too,
//! wires up rates and a Pyth-shaped price feed, and deposits, checking that the
//! deposit is priced 1:1 on the first deposit and deployed into the basket
//! through the router CPI.

extern crate std;

use {
    alloc::vec,
    alloc::vec::Vec,
    quasar_svm::{Account, AccountMeta, Instruction, Pubkey, QuasarSvm},
    solana_program_pack::Pack,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::println,
};

use crate::state::{APPROVED_ASSET_SEED, ASSET_CONFIG_SEED, REGISTRY_SEED, STRATEGY_SEED};

const DECIMALS: u8 = 6;
const FEE_BPS: u16 = 100;
const MAX_SLIPPAGE_BPS: u16 = 100;
const STARTING_LAMPORTS: u64 = 1_000_000_000;

// Router program (loaded for the deposit test).
const ROUTER_ID_STR: &str = "SWPR8Rk3aq3DrDGLdaANq7xCMnXoUFUJWJJmCWxc8Jm";
const RATE: u64 = 250; // USDC base units per asset base unit
const NOW: i64 = 1_000; // fixed clock for the deposit test

fn program_id() -> Pubkey {
    Pubkey::new_from_array(crate::ID.to_bytes())
}
fn router_id() -> Pubkey {
    ROUTER_ID_STR.parse().unwrap()
}
fn rent_id() -> Pubkey {
    quasar_svm::solana_sdk_ids::sysvar::rent::ID
}
fn token_program_id() -> Pubkey {
    quasar_svm::SPL_TOKEN_PROGRAM_ID
}
fn system_program_id() -> Pubkey {
    quasar_svm::system_program::ID
}

fn signer_account(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, STARTING_LAMPORTS)
}
fn empty_account(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: system_program_id(),
        executable: false,
    }
}
fn mint_account(address: Pubkey, authority: Pubkey) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &Mint {
            mint_authority: Some(authority).into(),
            supply: 0,
            decimals: DECIMALS,
            is_initialized: true,
            freeze_authority: None.into(),
        },
    )
}
fn token_account(address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
    quasar_svm::token::create_keyed_token_account(
        &address,
        &TokenAccount {
            mint,
            owner,
            amount,
            state: AccountState::Initialized,
            ..TokenAccount::default()
        },
    )
}
fn token_amount(account: &Account) -> u64 {
    TokenAccount::unpack(&account.data).unwrap().amount
}

// A Pyth PriceUpdateV2-shaped account: `price` (i64) at offset 73, `publish_time`
// (i64) at offset 93. The program reads only those two fields.
fn pyth_feed_account(address: Pubkey, price: i64, publish_time: i64) -> Account {
    let mut data = vec![0u8; 200];
    data[73..81].copy_from_slice(&price.to_le_bytes());
    data[93..101].copy_from_slice(&publish_time.to_le_bytes());
    Account {
        address,
        lamports: 1_000_000,
        data,
        owner: Pubkey::new_unique(),
        executable: false,
    }
}

fn registry_pda(authority: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[REGISTRY_SEED, authority.as_ref()], &program_id()).0
}
fn approved_asset_pda(registry: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[APPROVED_ASSET_SEED, registry.as_ref(), mint.as_ref()],
        &program_id(),
    )
    .0
}
fn strategy_pda(index: u64) -> Pubkey {
    Pubkey::find_program_address(&[STRATEGY_SEED, &index.to_le_bytes()], &program_id()).0
}
fn share_mint_pda(strategy: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"share_mint", strategy.as_ref()], &program_id()).0
}
fn usdc_vault_pda(strategy: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"usdc_vault", strategy.as_ref()], &program_id()).0
}
fn asset_config_pda(strategy: &Pubkey, index: u8) -> Pubkey {
    Pubkey::find_program_address(
        &[ASSET_CONFIG_SEED, strategy.as_ref(), &[index]],
        &program_id(),
    )
    .0
}
fn asset_vault_pda(strategy: &Pubkey, index: u8) -> Pubkey {
    Pubkey::find_program_address(
        &[b"asset_vault", strategy.as_ref(), &[index]],
        &program_id(),
    )
    .0
}
// Router PDAs.
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

// --- vault-strategy instruction data ---
fn init_registry_data() -> Vec<u8> {
    vec![0u8]
}
fn approve_asset_data(price_feed: &Pubkey) -> Vec<u8> {
    let mut data = vec![1u8];
    data.extend_from_slice(price_feed.as_ref());
    data
}
fn init_strategy_data(index: u64, swap_router: &Pubkey) -> Vec<u8> {
    let mut data = vec![2u8];
    data.extend_from_slice(&index.to_le_bytes());
    data.extend_from_slice(&FEE_BPS.to_le_bytes());
    data.extend_from_slice(&MAX_SLIPPAGE_BPS.to_le_bytes());
    data.extend_from_slice(swap_router.as_ref());
    data
}
fn add_asset_data(weight_bps: u16) -> Vec<u8> {
    let mut data = vec![3u8];
    data.extend_from_slice(&weight_bps.to_le_bytes());
    data
}
fn deposit_data(usdc_amount: u64, minimum_shares: u64) -> Vec<u8> {
    let mut data = vec![5u8];
    data.extend_from_slice(&usdc_amount.to_le_bytes());
    data.extend_from_slice(&minimum_shares.to_le_bytes());
    data
}

// --- router instruction data ---
fn router_init_data() -> Vec<u8> {
    vec![0u8]
}
fn router_set_rate_data(usdc_per_token: u64) -> Vec<u8> {
    let mut data = vec![1u8];
    data.extend_from_slice(&usdc_per_token.to_le_bytes());
    data
}

fn init_registry_ix(authority: Pubkey, registry: Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new(registry, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: init_registry_data(),
    }
}

fn approve_asset_ix(
    authority: Pubkey,
    registry: Pubkey,
    asset_mint: Pubkey,
    approved_asset: Pubkey,
    price_feed: Pubkey,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(registry, false),
            AccountMeta::new_readonly(asset_mint, false),
            AccountMeta::new(approved_asset, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: approve_asset_data(&price_feed),
    }
}

#[allow(clippy::too_many_arguments)]
fn init_strategy_ix(
    manager: Pubkey,
    usdc_mint: Pubkey,
    registry: Pubkey,
    strategy: Pubkey,
    share_mint: Pubkey,
    vault_usdc: Pubkey,
    index: u64,
    swap_router: Pubkey,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(manager, true),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new_readonly(registry, false),
            AccountMeta::new(strategy, false),
            AccountMeta::new(share_mint, false),
            AccountMeta::new(vault_usdc, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: init_strategy_data(index, &swap_router),
    }
}

#[allow(clippy::too_many_arguments)]
fn add_asset_ix(
    manager: Pubkey,
    strategy: Pubkey,
    registry: Pubkey,
    asset_mint: Pubkey,
    approved_asset: Pubkey,
    asset_config: Pubkey,
    vault_asset: Pubkey,
    weight_bps: u16,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(manager, true),
            AccountMeta::new(strategy, false),
            AccountMeta::new_readonly(registry, false),
            AccountMeta::new_readonly(asset_mint, false),
            AccountMeta::new_readonly(approved_asset, false),
            AccountMeta::new(asset_config, false),
            AccountMeta::new(vault_asset, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: add_asset_data(weight_bps),
    }
}

// Strategy account offsets (1-byte discriminator, tight packing).
const STRATEGY_ASSET_COUNT_OFFSET: usize = 1 + 8 + 32 + 32 + 32 + 32 + 32 + 2 + 2 + 8 + 8;
const STRATEGY_TOTAL_WEIGHT_OFFSET: usize = STRATEGY_ASSET_COUNT_OFFSET + 1;
// AssetConfig weight_bps offset.
const ASSET_CONFIG_WEIGHT_OFFSET: usize = 1 + 32 + 1 + 32 + 32 + 32;

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

#[test]
fn test_strategy_setup() {
    let mut svm = QuasarSvm::new()
        .with_program(
            &program_id(),
            &std::fs::read("target/deploy/quasar_vault_strategy.so").unwrap(),
        )
        .with_token_program();

    let authority = Pubkey::new_unique();
    let manager = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let asset_mint = Pubkey::new_unique();
    let price_feed = Pubkey::new_unique();
    let swap_router = router_id();

    let registry = registry_pda(&authority);
    let approved_asset = approved_asset_pda(&registry, &asset_mint);
    let index = 0u64;
    let strategy = strategy_pda(index);
    let share_mint = share_mint_pda(&strategy);
    let vault_usdc = usdc_vault_pda(&strategy);
    let asset_config = asset_config_pda(&strategy, 0);
    let vault_asset = asset_vault_pda(&strategy, 0);

    let accounts = vec![
        signer_account(authority),
        signer_account(manager),
        mint_account(usdc_mint, authority),
        mint_account(asset_mint, authority),
        empty_account(registry),
        empty_account(approved_asset),
        empty_account(strategy),
        empty_account(share_mint),
        empty_account(vault_usdc),
        empty_account(asset_config),
        empty_account(vault_asset),
    ];

    let instructions = vec![
        init_registry_ix(authority, registry),
        approve_asset_ix(authority, registry, asset_mint, approved_asset, price_feed),
        init_strategy_ix(
            manager,
            usdc_mint,
            registry,
            strategy,
            share_mint,
            vault_usdc,
            index,
            swap_router,
        ),
        add_asset_ix(
            manager,
            strategy,
            registry,
            asset_mint,
            approved_asset,
            asset_config,
            vault_asset,
            10_000,
        ),
    ];

    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(
        result.is_ok(),
        "setup chain failed: {:?}",
        result.raw_result
    );

    let strategy_data = &result.account(&strategy).unwrap().data;
    assert_eq!(strategy_data[0], 3, "strategy discriminator");
    assert_eq!(strategy_data[STRATEGY_ASSET_COUNT_OFFSET], 1, "asset_count");
    assert_eq!(
        read_u16(strategy_data, STRATEGY_TOTAL_WEIGHT_OFFSET),
        10_000,
        "total_weight_bps"
    );

    let asset_config_data = &result.account(&asset_config).unwrap().data;
    assert_eq!(asset_config_data[0], 4, "asset_config discriminator");
    assert_eq!(
        read_u16(asset_config_data, ASSET_CONFIG_WEIGHT_OFFSET),
        10_000,
        "weight_bps"
    );

    println!("  STRATEGY SETUP CU: {}", result.compute_units_consumed);
}

/// Two-program deposit: set up the router + a single-asset strategy, then
/// deposit USDC. The first deposit mints shares 1:1 and deploys the whole amount
/// into the asset through the router CPI.
#[test]
fn test_deposit() {
    let mut svm = QuasarSvm::new()
        .with_program(
            &program_id(),
            &std::fs::read("target/deploy/quasar_vault_strategy.so").unwrap(),
        )
        .with_program(
            &router_id(),
            &std::fs::read("../mock-swap-router/target/deploy/quasar_mock_swap_router.so").unwrap(),
        )
        .with_token_program();
    svm.warp_to_timestamp(NOW);

    let authority = Pubkey::new_unique();
    let manager = Pubkey::new_unique();
    let depositor = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let asset_mint = Pubkey::new_unique();
    let price_feed = Pubkey::new_unique();

    let registry = registry_pda(&authority);
    let approved_asset = approved_asset_pda(&registry, &asset_mint);
    let index = 0u64;
    let strategy = strategy_pda(index);
    let share_mint = share_mint_pda(&strategy);
    let vault_usdc = usdc_vault_pda(&strategy);
    let asset_config = asset_config_pda(&strategy, 0);
    let vault_asset = asset_vault_pda(&strategy, 0);

    let r_config = router_config_pda();
    let r_authority = router_authority_pda();
    let r_treasury = router_treasury_pda();
    let r_rate = router_rate_pda(&asset_mint);

    let depositor_usdc = Pubkey::new_unique();
    let depositor_share = Pubkey::new_unique();

    // Asset priced 250 USDC/token: Pyth price = 250 * 10^8 so
    // asset_value = amount * price / 10^8 gives 250 USDC per token base unit.
    let pyth_price: i64 = 250 * 100_000_000;

    const DEPOSIT: u64 = 1_000;
    const ASSET_OUT: u64 = DEPOSIT / RATE; // 4

    let accounts = vec![
        signer_account(authority),
        signer_account(manager),
        signer_account(depositor),
        // The asset mint is minted by the router authority.
        mint_account(usdc_mint, authority),
        mint_account(asset_mint, r_authority),
        // Router accounts.
        empty_account(r_config),
        empty_account(r_authority),
        empty_account(r_treasury),
        empty_account(r_rate),
        // Vault-strategy accounts.
        empty_account(registry),
        empty_account(approved_asset),
        empty_account(strategy),
        empty_account(share_mint),
        empty_account(vault_usdc),
        empty_account(asset_config),
        empty_account(vault_asset),
        // Depositor token accounts (share account created up front).
        token_account(depositor_usdc, usdc_mint, depositor, DEPOSIT),
        token_account(depositor_share, share_mint, depositor, 0),
        // Pyth feed.
        pyth_feed_account(price_feed, pyth_price, NOW),
    ];

    // Router account order for set_rate:
    //   authority, router_config, asset_mint, usdc_mint, asset_rate,
    //   router_authority, router_usdc_treasury, rent, token_program, system_program
    let router_set_rate = Instruction {
        program_id: router_id(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(r_config, false),
            AccountMeta::new_readonly(asset_mint, false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new(r_rate, false),
            AccountMeta::new_readonly(r_authority, false),
            AccountMeta::new(r_treasury, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: router_set_rate_data(RATE),
    };
    let router_init = Instruction {
        program_id: router_id(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new(r_config, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: router_init_data(),
    };

    // Deposit: declared accounts then remaining (asset_config, vault_asset,
    // asset_mint, asset_rate, price_feed).
    let mut deposit_accounts = vec![
        AccountMeta::new(depositor, true),
        AccountMeta::new(strategy, false),
        AccountMeta::new(share_mint, false),
        AccountMeta::new_readonly(usdc_mint, false),
        AccountMeta::new(depositor_usdc, false),
        AccountMeta::new(depositor_share, false),
        AccountMeta::new(vault_usdc, false),
        AccountMeta::new(r_config, false),
        AccountMeta::new(r_treasury, false),
        AccountMeta::new_readonly(r_authority, false),
        AccountMeta::new_readonly(router_id(), false),
        AccountMeta::new_readonly(token_program_id(), false),
        AccountMeta::new_readonly(system_program_id(), false),
    ];
    for meta in [
        AccountMeta::new_readonly(asset_config, false),
        AccountMeta::new(vault_asset, false),
        AccountMeta::new(asset_mint, false),
        AccountMeta::new_readonly(r_rate, false),
        AccountMeta::new_readonly(price_feed, false),
    ] {
        deposit_accounts.push(meta);
    }
    let deposit_ix = Instruction {
        program_id: program_id(),
        accounts: deposit_accounts,
        data: deposit_data(DEPOSIT, DEPOSIT),
    };

    let instructions = vec![
        router_init,
        router_set_rate,
        init_registry_ix(authority, registry),
        approve_asset_ix(authority, registry, asset_mint, approved_asset, price_feed),
        init_strategy_ix(
            manager,
            usdc_mint,
            registry,
            strategy,
            share_mint,
            vault_usdc,
            index,
            router_id(),
        ),
        add_asset_ix(
            manager,
            strategy,
            registry,
            asset_mint,
            approved_asset,
            asset_config,
            vault_asset,
            10_000,
        ),
        deposit_ix,
    ];

    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(
        result.is_ok(),
        "deposit chain failed: {:?}",
        result.raw_result
    );

    // First deposit mints shares 1:1 with USDC.
    assert_eq!(
        token_amount(result.account(&depositor_share).unwrap()),
        DEPOSIT
    );
    // The deposit was deployed into the asset via the router.
    assert_eq!(
        token_amount(result.account(&vault_asset).unwrap()),
        ASSET_OUT
    );
    assert_eq!(token_amount(result.account(&depositor_usdc).unwrap()), 0);
    assert_eq!(token_amount(result.account(&r_treasury).unwrap()), DEPOSIT);
    // All USDC was swapped out of the vault into the asset.
    assert_eq!(token_amount(result.account(&vault_usdc).unwrap()), 0);

    println!("  DEPOSIT CU: {}", result.compute_units_consumed);
}
