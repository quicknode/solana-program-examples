//! QuasarSVM integration tests for the mock swap router: initialize it, set a
//! rate (which also creates the USDC treasury), then swap USDC for an asset and
//! back, asserting the minted/burned amounts and treasury flows.

extern crate std;

use {
    alloc::vec,
    quasar_svm::{Account, AccountMeta, Instruction, Pubkey, QuasarSvm},
    solana_program_pack::Pack,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::println,
};

use crate::state::{ASSET_RATE_SEED, ROUTER_AUTHORITY_SEED, ROUTER_CONFIG_SEED};

const DECIMALS: u8 = 6;
const RATE: u64 = 250; // 250 USDC base units per asset base unit
const STARTING_LAMPORTS: u64 = 1_000_000_000;

fn program_id() -> Pubkey {
    Pubkey::new_from_array(crate::ID.to_bytes())
}
fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_mock_swap_router.so").unwrap();
    QuasarSvm::new()
        .with_program(&program_id(), &elf)
        .with_token_program()
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

fn config_pda() -> Pubkey {
    Pubkey::find_program_address(&[ROUTER_CONFIG_SEED], &program_id()).0
}
fn authority_pda() -> Pubkey {
    Pubkey::find_program_address(&[ROUTER_AUTHORITY_SEED], &program_id()).0
}
fn treasury_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"treasury"], &program_id()).0
}
fn rate_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[ASSET_RATE_SEED, mint.as_ref()], &program_id()).0
}

fn init_router_ix(authority: Pubkey, usdc_mint: Pubkey, config: Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new(config, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: vec![0u8],
    }
}

#[allow(clippy::too_many_arguments)]
fn set_rate_ix(
    authority: Pubkey,
    config: Pubkey,
    asset_mint: Pubkey,
    usdc_mint: Pubkey,
    rate: Pubkey,
    router_authority: Pubkey,
    treasury: Pubkey,
    usdc_per_token: u64,
) -> Instruction {
    let mut data = vec![1u8];
    data.extend_from_slice(&usdc_per_token.to_le_bytes());
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(config, false),
            AccountMeta::new_readonly(asset_mint, false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new(rate, false),
            AccountMeta::new_readonly(router_authority, false),
            AccountMeta::new(treasury, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data,
    }
}

#[allow(clippy::too_many_arguments)]
fn swap_usdc_for_asset_ix(
    caller: Pubkey,
    config: Pubkey,
    rate: Pubkey,
    usdc_mint: Pubkey,
    asset_mint: Pubkey,
    caller_usdc: Pubkey,
    caller_asset: Pubkey,
    treasury: Pubkey,
    router_authority: Pubkey,
    usdc_amount_in: u64,
    minimum_asset_out: u64,
) -> Instruction {
    let mut data = vec![2u8];
    data.extend_from_slice(&usdc_amount_in.to_le_bytes());
    data.extend_from_slice(&minimum_asset_out.to_le_bytes());
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(caller, true),
            AccountMeta::new_readonly(config, false),
            AccountMeta::new_readonly(rate, false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new(asset_mint, false),
            AccountMeta::new(caller_usdc, false),
            AccountMeta::new(caller_asset, false),
            AccountMeta::new(treasury, false),
            AccountMeta::new_readonly(router_authority, false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data,
    }
}

#[test]
fn test_initialize_and_swap_usdc_for_asset() {
    let mut svm = setup();

    let authority = Pubkey::new_unique();
    let usdc_mint = Pubkey::new_unique();
    let asset_mint = Pubkey::new_unique();
    let config = config_pda();
    let router_authority = authority_pda();
    let treasury = treasury_pda();
    let rate = rate_pda(&asset_mint);
    let caller_usdc = Pubkey::new_unique();
    let caller_asset = Pubkey::new_unique();

    const USDC_IN: u64 = 1_000;
    const ASSET_OUT: u64 = USDC_IN / RATE; // 4

    let accounts = vec![
        signer_account(authority),
        mint_account(usdc_mint, authority),
        // The asset mint's authority is the router-authority PDA, so the router
        // can mint it.
        mint_account(asset_mint, router_authority),
        empty_account(config),
        empty_account(rate),
        empty_account(router_authority),
        empty_account(treasury),
        token_account(caller_usdc, usdc_mint, authority, USDC_IN),
        token_account(caller_asset, asset_mint, authority, 0),
    ];

    let instructions = vec![
        init_router_ix(authority, usdc_mint, config),
        set_rate_ix(
            authority,
            config,
            asset_mint,
            usdc_mint,
            rate,
            router_authority,
            treasury,
            RATE,
        ),
        swap_usdc_for_asset_ix(
            authority,
            config,
            rate,
            usdc_mint,
            asset_mint,
            caller_usdc,
            caller_asset,
            treasury,
            router_authority,
            USDC_IN,
            ASSET_OUT,
        ),
    ];

    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(
        result.is_ok(),
        "router chain failed: {:?}",
        result.raw_result
    );

    // Caller paid all USDC and received the minted asset; treasury holds the USDC.
    assert_eq!(token_amount(result.account(&caller_usdc).unwrap()), 0);
    assert_eq!(
        token_amount(result.account(&caller_asset).unwrap()),
        ASSET_OUT
    );
    assert_eq!(token_amount(result.account(&treasury).unwrap()), USDC_IN);

    println!("  ROUTER SWAP CU: {}", result.compute_units_consumed);
}
