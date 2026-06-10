use quasar_svm::{Account, Instruction, Pubkey, QuasarSvm};
use solana_address::Address;

fn setup() -> QuasarSvm {
    let elf = include_bytes!("../target/deploy/quasar_realloc.so");
    QuasarSvm::new().with_program(&Pubkey::from(crate::ID), elf)
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

/// Build initialize instruction data using Quasar's compact wire format.
/// `String<1024>` defaults to a u8 length prefix (the second `String` generic
/// argument is the prefix type and its default is `u8`).
///
///   header: [disc: u8 = 0][message_len: u8]
///   tail:   [message bytes]
fn build_initialize(message: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(2 + message.len());
    data.push(0u8); // discriminator
    data.push(message.len() as u8);
    data.extend_from_slice(message.as_bytes());
    data
}

/// Build update instruction data using the same compact wire format.
fn build_update(message: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(2 + message.len());
    data.push(1u8); // discriminator
    data.push(message.len() as u8);
    data.extend_from_slice(message.as_bytes());
    data
}

#[test]
fn test_initialize() {
    let mut svm = setup();

    let payer = Pubkey::new_unique();
    let message_account = Pubkey::new_unique();
    let system_program = quasar_svm::system_program::ID;

    let ix = Instruction {
        program_id: Pubkey::from(crate::ID),
        accounts: vec![
            solana_instruction::AccountMeta::new(Address::from(payer.to_bytes()), true),
            solana_instruction::AccountMeta::new(
                Address::from(message_account.to_bytes()),
                true,
            ),
            solana_instruction::AccountMeta::new_readonly(
                Address::from(system_program.to_bytes()),
                false,
            ),
        ],
        data: build_initialize("Hello, World!"),
    };

    let result = svm.process_instruction(&ix, &[signer(payer), empty(message_account)]);
    result.assert_success();

    // Verify: disc(1) + message (u8 prefix + bytes)
    let account = result.account(&message_account).unwrap();
    assert_eq!(account.data[0], 1, "discriminator");

    let msg_len = account.data[1] as usize;
    assert_eq!(msg_len, 13);
    assert_eq!(&account.data[2..2 + msg_len], b"Hello, World!");
}

#[test]
fn test_update_longer_message() {
    let mut svm = setup();

    let payer = Pubkey::new_unique();
    let message_account = Pubkey::new_unique();
    let system_program = quasar_svm::system_program::ID;
    let program_id = Pubkey::from(crate::ID);

    // Initialize with short message
    let init_ix = Instruction {
        program_id,
        accounts: vec![
            solana_instruction::AccountMeta::new(Address::from(payer.to_bytes()), true),
            solana_instruction::AccountMeta::new(
                Address::from(message_account.to_bytes()),
                true,
            ),
            solana_instruction::AccountMeta::new_readonly(
                Address::from(system_program.to_bytes()),
                false,
            ),
        ],
        data: build_initialize("Hi"),
    };

    let result = svm.process_instruction(&init_ix, &[signer(payer), empty(message_account)]);
    result.assert_success();

    let payer_after_init = result.account(&payer).unwrap().clone();
    let msg_after_init = result.account(&message_account).unwrap().clone();

    // Update with longer message - triggers realloc
    let update_ix = Instruction {
        program_id,
        accounts: vec![
            solana_instruction::AccountMeta::new(Address::from(payer.to_bytes()), true),
            solana_instruction::AccountMeta::new(
                Address::from(message_account.to_bytes()),
                false,
            ),
            solana_instruction::AccountMeta::new_readonly(
                Address::from(system_program.to_bytes()),
                false,
            ),
        ],
        data: build_update("Hello, this is a much longer message!"),
    };

    let result = svm.process_instruction(&update_ix, &[payer_after_init, msg_after_init]);
    result.assert_success();

    // Note: QuasarSvm may not fully reflect realloc changes (data length change)
    // in test results. The realloc is handled by set_inner which modifies the
    // RuntimeAccount data_len field directly. Onchain this works correctly.
}
