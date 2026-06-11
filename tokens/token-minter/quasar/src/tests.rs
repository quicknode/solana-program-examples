extern crate std;
use {
    alloc::vec,
    quasar_svm::{Account, Instruction, Pubkey, QuasarSvm},
    solana_program_pack::Pack,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::println,
};

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_token_minter.so").unwrap();
    QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program()
}

fn signer(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, 5_000_000_000)
}

fn mint(address: Pubkey, authority: Pubkey) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &Mint {
            mint_authority: Some(authority).into(),
            supply: 0,
            decimals: 9,
            is_initialized: true,
            freeze_authority: Some(authority).into(),
        },
    )
}

fn token_account(address: Pubkey, mint_address: Pubkey, owner: Pubkey, amount: u64) -> Account {
    quasar_svm::token::create_keyed_token_account(
        &address,
        &TokenAccount {
            mint: mint_address,
            owner,
            amount,
            state: AccountState::Initialized,
            ..TokenAccount::default()
        },
    )
}

/// Decimals configured by the mint fixture above, matching the program's
/// `mint(decimals = 9)` constraint in `CreateTokenAccountConstraints`.
const MINT_DECIMALS: u32 = 9;

/// Converts a whole-token (major unit) count to minor units, the form the
/// program's `mint_token` handler takes amounts in.
fn to_minor_units(major_units: u64) -> u64 {
    major_units.checked_mul(10u64.pow(MINT_DECIMALS)).unwrap()
}

/// Build mint_token instruction data.
/// Wire format: [disc=1] [amount: u64 LE, in minor units]
fn build_mint_token_data(amount: u64) -> Vec<u8> {
    let mut data = vec![1u8];
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

// Note: create_token test requires the Metaplex Token Metadata program
// deployed in the SVM. The quasar-svm harness does not currently ship it,
// so we test mint_token (pure SPL Token CPI) only.

#[test]
fn test_mint_token() {
    let mut svm = setup();

    let authority = Pubkey::new_unique();
    let recipient = Pubkey::new_unique();
    let mint_address = Pubkey::new_unique();
    let token_addr = Pubkey::new_unique();
    let token_program = quasar_svm::SPL_TOKEN_PROGRAM_ID;
    let system_program = quasar_svm::system_program::ID;

    let amount = to_minor_units(100);
    let data = build_mint_token_data(amount);

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(authority.into(), true),
            solana_instruction::AccountMeta::new_readonly(recipient.into(), false),
            solana_instruction::AccountMeta::new(mint_address.into(), false),
            solana_instruction::AccountMeta::new(token_addr.into(), false),
            solana_instruction::AccountMeta::new_readonly(token_program.into(), false),
            solana_instruction::AccountMeta::new_readonly(system_program.into(), false),
        ],
        data,
    };

    let result = svm.process_instruction(
        &instruction,
        &[
            signer(authority),
            signer(recipient),
            mint(mint_address, authority),
            token_account(token_addr, mint_address, recipient, 0),
        ],
    );

    assert!(
        result.is_ok(),
        "mint_token failed: {:?}",
        result.raw_result
    );

    // The recipient's token account balance is the exact minor-unit amount
    // requested - the program performs no onchain scaling.
    let token_account_after = result.account(&token_addr).unwrap();
    let token_account_state = TokenAccount::unpack_from_slice(&token_account_after.data).unwrap();
    assert_eq!(token_account_state.amount, amount);

    println!("  MINT TOKEN CU: {}", result.compute_units_consumed);
}
