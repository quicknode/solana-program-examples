extern crate std;
use {
    alloc::vec,
    quasar_svm::{Account, Instruction, Pubkey, QuasarSvm},
    std::println,
};

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_token_swap.so").unwrap();
    QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program()
}

fn signer(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, 10_000_000_000)
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

fn build_create_config_data(fee: u16, admin_share_bps: u16) -> Vec<u8> {
    let mut data = vec![0u8]; // discriminator
    data.extend_from_slice(&fee.to_le_bytes());
    data.extend_from_slice(&admin_share_bps.to_le_bytes());
    data
}

#[test]
fn test_create_config() {
    let mut svm = setup();

    let payer = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let system_program = quasar_svm::system_program::ID;

    // Derive the singleton Config PDA (seeds = [b"config"]).
    let (config_pda, _) = Pubkey::find_program_address(&[b"config"], &crate::ID.into());

    // Uniswap V2's classic 1/6 split for the admin slice.
    let data = build_create_config_data(30, 1667);

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(config_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(admin.into(), false),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(system_program.into(), false),
        ],
        data,
    };

    let result = svm.process_instruction(
        &instruction,
        &[empty(config_pda), signer(admin), signer(payer)],
    );

    assert!(
        result.is_ok(),
        "create_config failed: {:?}",
        result.raw_result
    );
    println!("  CREATE CONFIG CU: {}", result.compute_units_consumed);
}

#[test]
fn test_create_config_invalid_fee() {
    let mut svm = setup();

    let payer = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let system_program = quasar_svm::system_program::ID;

    let (config_pda, _) = Pubkey::find_program_address(&[b"config"], &crate::ID.into());

    // Fee >= 10000 should fail.
    let data = build_create_config_data(10000, 1667);

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(config_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(admin.into(), false),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(system_program.into(), false),
        ],
        data,
    };

    let result = svm.process_instruction(
        &instruction,
        &[empty(config_pda), signer(admin), signer(payer)],
    );

    assert!(
        !result.is_ok(),
        "create_config should have failed with invalid fee"
    );
    println!("  CREATE CONFIG (invalid fee) correctly rejected");
}

#[test]
fn test_create_config_invalid_admin_share() {
    let mut svm = setup();

    let payer = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let system_program = quasar_svm::system_program::ID;

    let (config_pda, _) = Pubkey::find_program_address(&[b"config"], &crate::ID.into());

    // admin_share_bps >= 10000 should fail.
    let data = build_create_config_data(30, 10000);

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(config_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(admin.into(), false),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(system_program.into(), false),
        ],
        data,
    };

    let result = svm.process_instruction(
        &instruction,
        &[empty(config_pda), signer(admin), signer(payer)],
    );

    assert!(
        !result.is_ok(),
        "create_config should have failed with admin_share_bps >= 10000"
    );
    println!("  CREATE CONFIG (invalid admin share) correctly rejected");
}
