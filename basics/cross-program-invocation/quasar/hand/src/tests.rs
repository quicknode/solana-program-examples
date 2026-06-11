use quasar_svm::{Account, Instruction, Pubkey, QuasarSvm};
use solana_address::Address;

/// Lever program's program ID - must match the lever's declare_id!().
fn lever_program_id() -> Pubkey {
    Pubkey::from(crate::LEVER_PROGRAM_ID)
}

/// PowerStatus discriminator from the lever program.
const POWER_STATUS_DISCRIMINATOR: u8 = 1;

fn setup() -> QuasarSvm {
    let hand_elf = include_bytes!("../target/deploy/quasar_hand.so");
    let lever_elf = include_bytes!("../../lever/target/deploy/quasar_lever.so");
    QuasarSvm::new()
        .with_program(&Pubkey::from(crate::ID), hand_elf)
        .with_program(&lever_program_id(), lever_elf)
}

fn power_account(address: Pubkey, is_on: bool) -> Account {
    // Account data: [discriminator: u8] [is_on: u8]
    let data = vec![POWER_STATUS_DISCRIMINATOR, if is_on { 1 } else { 0 }];
    Account {
        address,
        lamports: 1_000_000_000,
        data,
        owner: lever_program_id(),
        executable: false,
    }
}

/// Build pull_lever instruction data (discriminator = 0).
///
/// Wire format: [discriminator = 0] [name: u8 length prefix + bytes].
///
/// The hand's pull_lever instruction takes `String<50>`, which Quasar
/// serialises with a single-byte length prefix. The CPI builder in
/// `pull_lever.rs` re-serialises the same name into the lever's
/// instruction data using the same u8 prefix.
fn build_pull_lever(name: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(2 + name.len());
    data.push(0u8); // discriminator = 0
    data.push(name.len() as u8);
    data.extend_from_slice(name.as_bytes());
    data
}

#[test]
fn test_pull_lever_turns_on() {
    let mut svm = setup();

    let (power_addr, _bump) = Pubkey::find_program_address(&[b"power"], &lever_program_id());

    let ix = Instruction {
        program_id: Pubkey::from(crate::ID),
        accounts: vec![
            solana_instruction::AccountMeta::new(
                Address::from(power_addr.to_bytes()),
                false,
            ),
            solana_instruction::AccountMeta::new_readonly(
                Address::from(lever_program_id().to_bytes()),
                false,
            ),
        ],
        data: build_pull_lever("Alice"),
    };

    // The lever program account is provided by the SVM (loaded via with_program).
    // Only the power data account needs to be passed explicitly.
    let result = svm.process_instruction(
        &ix,
        &[power_account(power_addr, false)],
    );

    result.assert_success();

    let logs = result.logs.join("\n");
    assert!(logs.contains("Hand is pulling"), "hand should log");
    assert!(logs.contains("pulling the power switch"), "lever should log");
    assert!(logs.contains("now on"), "power should turn on");
    // Verifies the CPI wire format: the lever logs the name it
    // deserialised. A stale u32 length prefix on either the inbound
    // `pull_lever` payload or the CPI to `switch_power` would corrupt
    // this (e.g. "\0\0\0Al" instead of "Alice").
    assert!(
        logs.contains("Alice"),
        "name should round-trip through hand → lever CPI; logs: {logs}"
    );

    let account = result.account(&power_addr).unwrap();
    assert_eq!(account.data[1], 1, "power should be on");
}

#[test]
fn test_pull_lever_turns_off() {
    let mut svm = setup();

    let (power_addr, _bump) = Pubkey::find_program_address(&[b"power"], &lever_program_id());

    let ix = Instruction {
        program_id: Pubkey::from(crate::ID),
        accounts: vec![
            solana_instruction::AccountMeta::new(
                Address::from(power_addr.to_bytes()),
                false,
            ),
            solana_instruction::AccountMeta::new_readonly(
                Address::from(lever_program_id().to_bytes()),
                false,
            ),
        ],
        data: build_pull_lever("Bob"),
    };

    let result = svm.process_instruction(
        &ix,
        &[power_account(power_addr, true)],
    );

    result.assert_success();

    let logs = result.logs.join("\n");
    assert!(logs.contains("now off"), "power should turn off");
    assert!(
        logs.contains("Bob"),
        "name should round-trip through hand → lever CPI; logs: {logs}"
    );

    let account = result.account(&power_addr).unwrap();
    assert_eq!(account.data[1], 0, "power should be off");
}
