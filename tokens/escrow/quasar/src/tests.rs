extern crate std;
use {
    alloc::vec,
    alloc::vec::Vec,
    quasar_svm::{Account, Instruction, Pubkey, QuasarSvm},
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::println,
};

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_escrow.so").unwrap();
    QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program()
}

fn signer(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, 1_000_000_000)
}

fn empty(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: quasar_svm::system_program::ID,
        executable: false,
    }
}

fn mint(address: Pubkey, authority: Pubkey) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &Mint {
            mint_authority: Some(authority).into(),
            supply: 1_000_000_000,
            decimals: 9,
            is_initialized: true,
            freeze_authority: None.into(),
        },
    )
}

fn token(address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
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

/// Build offer account data manually.
/// Layout (from #[account] codegen):
///   [disc: 1 byte = 1]
///   [maker: 32 bytes (Address)]
///   [token_mint_a: 32 bytes]
///   [token_mint_b: 32 bytes]
///   [maker_token_account_b: 32 bytes]
///   [receive: 8 bytes (PodU64 LE)]
///   [bump: 1 byte]
/// Total: 138 bytes
fn offer_data(
    maker: Pubkey,
    token_mint_a: Pubkey,
    token_mint_b: Pubkey,
    maker_token_account_b: Pubkey,
    receive: u64,
    bump: u8,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(138);
    data.push(1u8); // discriminator
    data.extend_from_slice(maker.as_ref());
    data.extend_from_slice(token_mint_a.as_ref());
    data.extend_from_slice(token_mint_b.as_ref());
    data.extend_from_slice(maker_token_account_b.as_ref());
    data.extend_from_slice(&receive.to_le_bytes());
    data.push(bump);
    data
}

fn offer_account(
    address: Pubkey,
    maker: Pubkey,
    token_mint_a: Pubkey,
    token_mint_b: Pubkey,
    maker_token_account_b: Pubkey,
    receive: u64,
    bump: u8,
) -> Account {
    Account {
        address,
        lamports: 2_000_000,
        data: offer_data(maker, token_mint_a, token_mint_b, maker_token_account_b, receive, bump),
        owner: crate::ID,
        executable: false,
    }
}

/// Mark specific account indices as signers on an instruction.
fn with_signers(mut ix: Instruction, indices: &[usize]) -> Instruction {
    for &i in indices {
        ix.accounts[i].is_signer = true;
    }
    ix
}

/// Build make_offer instruction data.
/// Wire format: [discriminator: u8 = 0] [deposit: u64 LE] [receive: u64 LE]
fn build_make_offer_data(deposit: u64, receive: u64) -> Vec<u8> {
    let mut data = vec![0u8];
    data.extend_from_slice(&deposit.to_le_bytes());
    data.extend_from_slice(&receive.to_le_bytes());
    data
}

/// Build take_offer instruction data.
/// Wire format: [discriminator: u8 = 1]
fn build_take_offer_data() -> Vec<u8> {
    vec![1u8]
}

/// Build cancel_offer instruction data.
/// Wire format: [discriminator: u8 = 2]
fn build_cancel_offer_data() -> Vec<u8> {
    vec![2u8]
}

#[test]
fn test_make_offer() {
    let mut svm = setup();

    let token_program = quasar_svm::SPL_TOKEN_PROGRAM_ID;
    let system_program = quasar_svm::system_program::ID;
    let maker = Pubkey::new_unique();
    let token_mint_a = Pubkey::new_unique();
    let token_mint_b = Pubkey::new_unique();
    let maker_token_account_a = Pubkey::new_unique();
    let maker_token_account_b = Pubkey::new_unique();
    let vault = Pubkey::new_unique();
    let (offer, offer_bump) =
        Pubkey::find_program_address(&[b"offer", maker.as_ref()], &crate::ID);
    let rent = quasar_svm::solana_sdk_ids::sysvar::rent::ID;

    let data = build_make_offer_data(1337, 1337);

    let instruction = with_signers(
        Instruction {
            program_id: crate::ID,
            accounts: vec![
                solana_instruction::AccountMeta::new(maker.into(), true),
                solana_instruction::AccountMeta::new(offer.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_mint_a.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_mint_b.into(), false),
                solana_instruction::AccountMeta::new(maker_token_account_a.into(), false),
                solana_instruction::AccountMeta::new(maker_token_account_b.into(), false),
                solana_instruction::AccountMeta::new(vault.into(), false),
                solana_instruction::AccountMeta::new_readonly(rent.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_program.into(), false),
                solana_instruction::AccountMeta::new_readonly(system_program.into(), false),
            ],
            data,
        },
        &[5, 6], // maker_token_account_b, vault as signers for create_account CPI
    );

    let result = svm.process_instruction(
        &instruction,
        &[
            signer(maker),
            empty(offer),
            mint(token_mint_a, maker),
            mint(token_mint_b, maker),
            token(maker_token_account_a, token_mint_a, maker, 1_000_000),
            empty(maker_token_account_b),
            empty(vault),
        ],
    );

    assert!(result.is_ok(), "make_offer failed: {:?}", result.raw_result);

    // Verify offer state
    let offer_data = &result.account(&offer).unwrap().data;
    assert_eq!(offer_data[0], 1, "discriminator");
    assert_eq!(&offer_data[1..33], maker.as_ref(), "maker");
    assert_eq!(&offer_data[129..137], &1337u64.to_le_bytes(), "receive");
    assert_eq!(offer_data[137], offer_bump, "bump");

    println!("  MAKE_OFFER CU: {}", result.compute_units_consumed);
}

#[test]
fn test_take_offer() {
    let mut svm = setup();

    let token_program = quasar_svm::SPL_TOKEN_PROGRAM_ID;
    let system_program = quasar_svm::system_program::ID;
    let maker = Pubkey::new_unique();
    let taker = Pubkey::new_unique();
    let token_mint_a = Pubkey::new_unique();
    let token_mint_b = Pubkey::new_unique();
    let taker_token_account_a = Pubkey::new_unique();
    let taker_token_account_b = Pubkey::new_unique();
    let maker_token_account_b = Pubkey::new_unique();
    let vault = Pubkey::new_unique();
    let (offer, offer_bump) =
        Pubkey::find_program_address(&[b"offer", maker.as_ref()], &crate::ID);
    let rent = quasar_svm::solana_sdk_ids::sysvar::rent::ID;

    let data = build_take_offer_data();

    let instruction = with_signers(
        Instruction {
            program_id: crate::ID,
            accounts: vec![
                solana_instruction::AccountMeta::new(taker.into(), true),
                solana_instruction::AccountMeta::new(offer.into(), false),
                solana_instruction::AccountMeta::new(maker.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_mint_a.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_mint_b.into(), false),
                solana_instruction::AccountMeta::new(taker_token_account_a.into(), false),
                solana_instruction::AccountMeta::new(taker_token_account_b.into(), false),
                solana_instruction::AccountMeta::new(maker_token_account_b.into(), false),
                solana_instruction::AccountMeta::new(vault.into(), false),
                solana_instruction::AccountMeta::new_readonly(rent.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_program.into(), false),
                solana_instruction::AccountMeta::new_readonly(system_program.into(), false),
            ],
            data,
        },
        &[5, 7], // taker_token_account_a, maker_token_account_b as signers for create_account CPI
    );

    let result = svm.process_instruction(
        &instruction,
        &[
            signer(taker),
            offer_account(offer, maker, token_mint_a, token_mint_b, maker_token_account_b, 1337, offer_bump),
            signer(maker),
            mint(token_mint_a, maker),
            mint(token_mint_b, maker),
            empty(taker_token_account_a),
            token(taker_token_account_b, token_mint_b, taker, 10_000),
            empty(maker_token_account_b),
            token(vault, token_mint_a, offer, 1337),
        ],
    );

    assert!(result.is_ok(), "take_offer failed: {:?}", result.raw_result);
    println!("  TAKE_OFFER CU: {}", result.compute_units_consumed);
}

#[test]
fn test_cancel_offer() {
    let mut svm = setup();

    let token_program = quasar_svm::SPL_TOKEN_PROGRAM_ID;
    let system_program = quasar_svm::system_program::ID;
    let maker = Pubkey::new_unique();
    let token_mint_a = Pubkey::new_unique();
    let token_mint_b = Pubkey::new_unique();
    let maker_token_account_a = Pubkey::new_unique();
    let maker_token_account_b = Pubkey::new_unique();
    let vault = Pubkey::new_unique();
    let (offer, offer_bump) =
        Pubkey::find_program_address(&[b"offer", maker.as_ref()], &crate::ID);
    let rent = quasar_svm::solana_sdk_ids::sysvar::rent::ID;

    let data = build_cancel_offer_data();

    let instruction = with_signers(
        Instruction {
            program_id: crate::ID,
            accounts: vec![
                solana_instruction::AccountMeta::new(maker.into(), true),
                solana_instruction::AccountMeta::new(offer.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_mint_a.into(), false),
                solana_instruction::AccountMeta::new(maker_token_account_a.into(), false),
                solana_instruction::AccountMeta::new(vault.into(), false),
                solana_instruction::AccountMeta::new_readonly(rent.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_program.into(), false),
                solana_instruction::AccountMeta::new_readonly(system_program.into(), false),
            ],
            data,
        },
        &[3], // maker_token_account_a as signer for create_account CPI
    );

    let result = svm.process_instruction(
        &instruction,
        &[
            signer(maker),
            offer_account(offer, maker, token_mint_a, token_mint_b, maker_token_account_b, 1337, offer_bump),
            mint(token_mint_a, maker),
            empty(maker_token_account_a),
            token(vault, token_mint_a, offer, 1337),
        ],
    );

    assert!(result.is_ok(), "cancel_offer failed: {:?}", result.raw_result);
    println!("  CANCEL_OFFER CU: {}", result.compute_units_consumed);
}
