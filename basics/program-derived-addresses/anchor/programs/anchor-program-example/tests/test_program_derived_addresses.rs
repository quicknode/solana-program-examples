use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program, Address, InstructionData,
        ToAccountMetas,
    },
    borsh::BorshDeserialize,
    litesvm::LiteSVM,
    solana_kite::{create_wallet, send_transaction_from_instructions},
    solana_signer::Signer,
};

fn setup() -> (LiteSVM, solana_keypair::Keypair) {
    let program_id = program_derived_addresses_program::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/program_derived_addresses_program.so");
    svm.add_program(program_id, bytes).unwrap();
    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    (svm, payer)
}

/// Mirrors the on-chain layout. v2's `#[account]` is zero-copy, so the struct
/// carries explicit padding out to its alignment — `try_from_slice` rejects
/// trailing bytes, so the test type has to spell the padding out too.
#[derive(BorshDeserialize)]
struct PageVisits {
    page_visits: u32,
    bump: u8,
    _padding: [u8; 3],
}

#[test]
fn test_create_and_increment_page_visits() {
    let (mut svm, payer) = setup();
    let program_id = program_derived_addresses_program::id();

    // Derive PDA
    let (page_visits_pda, _bump) =
        Address::find_program_address(&[b"page_visits", payer.pubkey().as_ref()], &program_id);

    // Create page visits account
    let create_ix = Instruction::new_with_bytes(
        program_id,
        &program_derived_addresses_program::instruction::CreatePageVisits {}.data(),
        program_derived_addresses_program::accounts::CreatePageVisitsAccountConstraints {
            payer: payer.pubkey(),
            page_visits: page_visits_pda,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![create_ix], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify initial state (page_visits = 0)
    let account = svm.get_account(&page_visits_pda).expect("PDA should exist");
    let data = PageVisits::try_from_slice(&account.data[8..]).unwrap();
    assert_eq!(data.page_visits, 0, "Initial page visits should be 0");

    svm.expire_blockhash();

    // Increment page visits
    let increment_ix = Instruction::new_with_bytes(
        program_id,
        &program_derived_addresses_program::instruction::IncrementPageVisits {}.data(),
        program_derived_addresses_program::accounts::IncrementPageVisitsAccountConstraints {
            user: payer.pubkey(),
            page_visits: page_visits_pda,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![increment_ix], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify page_visits = 1
    let account = svm.get_account(&page_visits_pda).expect("PDA should exist");
    let data = PageVisits::try_from_slice(&account.data[8..]).unwrap();
    assert_eq!(
        data.page_visits, 1,
        "Page visits should be 1 after increment"
    );

    svm.expire_blockhash();

    // Increment again
    let increment_ix2 = Instruction::new_with_bytes(
        program_id,
        &program_derived_addresses_program::instruction::IncrementPageVisits {}.data(),
        program_derived_addresses_program::accounts::IncrementPageVisitsAccountConstraints {
            user: payer.pubkey(),
            page_visits: page_visits_pda,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![increment_ix2], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify page_visits = 2
    let account = svm.get_account(&page_visits_pda).expect("PDA should exist");
    let data = PageVisits::try_from_slice(&account.data[8..]).unwrap();
    assert_eq!(
        data.page_visits, 2,
        "Page visits should be 2 after second increment"
    );
}
