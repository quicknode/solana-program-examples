//! The program is exercised with both transaction formats: legacy for a small
//! document, v1 for one that would never fit in 1,232 bytes. The v1 tests also
//! cover the part of the format that is easy to get wrong: the transaction
//! config that replaces ComputeBudget instructions, whose unset fields mean
//! zero rather than "use the default".

use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    borsh::BorshDeserialize,
    litesvm::LiteSVM,
    solana_keypair::{Keypair, Signer},
    solana_message::{v1, VersionedMessage},
    solana_native_token::LAMPORTS_PER_SOL,
    solana_packet::PACKET_DATA_SIZE,
    solana_transaction::{versioned::VersionedTransaction, Transaction},
    solana_transaction_error::TransactionError,
    transaction_v1_anchor_program::state::Document,
};

/// What LiteSVM charges per signature, and what mainnet has charged since
/// genesis.
const BASE_FEE_LAMPORTS: u64 = 5_000;

/// Enough for the account creation and the copy. Unlike a legacy transaction,
/// a v1 transaction has no 200,000 CU default to fall back on: leaving the
/// limit unset means zero.
const COMPUTE_UNIT_LIMIT: u32 = 50_000;

/// The payer, the new account, the system program and the program itself
/// together load well under 64 KiB. Like the compute unit limit, this defaults
/// to zero when unset.
const LOADED_ACCOUNTS_DATA_SIZE_LIMIT: u32 = 64 * 1024;

/// A v1 transaction's config carries the resource limits that legacy and v0
/// transactions express with ComputeBudget program instructions. Every test
/// that expects to succeed starts from this one.
fn transaction_config() -> v1::TransactionConfig {
    v1::TransactionConfig::empty()
        .with_compute_unit_limit(COMPUTE_UNIT_LIMIT)
        .with_loaded_accounts_data_size_limit(LOADED_ACCOUNTS_DATA_SIZE_LIMIT)
}

/// The account as Anchor lays it out: discriminator, then the borsh `Vec<u8>`.
#[derive(BorshDeserialize)]
struct DocumentAccount {
    _discriminator: [u8; 8],
    data: Vec<u8>,
}

fn setup() -> (LiteSVM, Pubkey, Keypair) {
    let program_id = transaction_v1_anchor_program::id();
    let mut svm = LiteSVM::new();
    let program_bytes = include_bytes!("../../../target/deploy/transaction_v1_anchor_program.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
    (svm, program_id, payer)
}

/// The instruction and the PDA it will create.
fn store_document(program_id: &Pubkey, payer: &Pubkey, document: &[u8]) -> (Instruction, Pubkey) {
    let (document_address, _bump) =
        Pubkey::find_program_address(&[Document::SEED_PREFIX, payer.as_ref()], program_id);
    let instruction = Instruction::new_with_bytes(
        *program_id,
        &transaction_v1_anchor_program::instruction::StoreDocument {
            document: document.to_vec(),
        }
        .data(),
        transaction_v1_anchor_program::accounts::StoreDocumentAccountConstraints {
            payer: *payer,
            document_account: document_address,
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    (instruction, document_address)
}

/// Compile a v1 message and sign it. `try_compile_with_config` is the v1
/// counterpart of `Message::new_with_blockhash`; the config rides in the
/// message header, so there is no ComputeBudget instruction to append.
fn v1_transaction(
    svm: &LiteSVM,
    payer: &Keypair,
    instructions: &[Instruction],
    config: v1::TransactionConfig,
) -> VersionedTransaction {
    let message = v1::Message::try_compile_with_config(
        &payer.pubkey(),
        instructions,
        svm.latest_blockhash(),
        config,
    )
    .unwrap();
    VersionedTransaction::try_new(VersionedMessage::V1(message), &[payer]).unwrap()
}

/// The bytes that would be sent to an RPC node.
fn wire_size(transaction: &VersionedTransaction) -> usize {
    wincode::serialize(transaction).unwrap().len()
}

/// A document with a recognizable pattern, so a partial or shifted copy
/// would not pass the equality check.
fn sample_document(length: usize) -> Vec<u8> {
    (0..length).map(|i| (i % 251) as u8).collect()
}

fn stored_document(svm: &LiteSVM, document_address: &Pubkey) -> Vec<u8> {
    let account = svm.get_account(document_address).unwrap();
    DocumentAccount::try_from_slice(&account.data).unwrap().data
}

#[test]
fn stores_a_document_too_large_for_a_legacy_transaction() {
    let (mut svm, program_id, payer) = setup();
    let document = sample_document(3_000);
    let (instruction, document_address) = store_document(&program_id, &payer.pubkey(), &document);

    let transaction = v1_transaction(&svm, &payer, &[instruction], transaction_config());

    // Over the legacy limit, under the v1 one. LiteSVM does not enforce either
    // (a validator's packet layer does), so the test checks the size itself.
    let size = wire_size(&transaction);
    assert!(
        size > PACKET_DATA_SIZE,
        "{size} bytes fits a legacy transaction"
    );
    assert!(
        size <= v1::MAX_TRANSACTION_SIZE,
        "{size} bytes is over the v1 limit"
    );

    svm.send_transaction(transaction).unwrap();

    let account = svm.get_account(&document_address).unwrap();
    assert_eq!(account.owner, program_id);
    assert_eq!(account.data.len(), Document::required_space(document.len()));
    assert_eq!(stored_document(&svm, &document_address), document);
}

#[test]
fn a_legacy_transaction_still_works_for_a_small_document() {
    let (mut svm, program_id, payer) = setup();
    let document = sample_document(500);
    let (instruction, document_address) = store_document(&program_id, &payer.pubkey(), &document);

    // Nothing about the program changed: the same instruction goes through the
    // legacy format when it fits.
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    assert!(wire_size(&VersionedTransaction::from(transaction.clone())) <= PACKET_DATA_SIZE);

    svm.send_transaction(transaction).unwrap();

    assert_eq!(stored_document(&svm, &document_address), document);
}

#[test]
fn the_transaction_config_replaces_compute_budget_instructions() {
    let (mut svm, program_id, payer) = setup();
    let document = sample_document(3_000);
    let (instruction, _) = store_document(&program_id, &payer.pubkey(), &document);
    let rent = svm.minimum_balance_for_rent_exemption(Document::required_space(document.len()));

    // A v1 priority fee is a total in lamports, not a price per compute unit.
    let priority_fee_lamports = 10_000;
    let config = transaction_config().with_priority_fee(priority_fee_lamports);
    let transaction = v1_transaction(&svm, &payer, &[instruction], config);

    // v1 moved the signatures to the end, so the first byte on the wire is the
    // message version rather than a signature count. 0x81 means v1.
    assert_eq!(wincode::serialize(&transaction).unwrap()[0], v1::V1_PREFIX);

    let balance_before = svm.get_balance(&payer.pubkey()).unwrap();
    let metadata = svm.send_transaction(transaction).unwrap();
    let balance_after = svm.get_balance(&payer.pubkey()).unwrap();

    assert!(metadata.compute_units_consumed <= u64::from(COMPUTE_UNIT_LIMIT));
    assert_eq!(
        balance_before - balance_after,
        rent + BASE_FEE_LAMPORTS + priority_fee_lamports
    );
}

#[test]
fn unset_config_fields_mean_zero_not_the_default() {
    let (mut svm, program_id, payer) = setup();
    let document = sample_document(500);
    let (instruction, _) = store_document(&program_id, &payer.pubkey(), &document);

    // No loaded-accounts data size limit: the transaction fails before any
    // program runs, because zero bytes of account data may be loaded.
    let transaction = v1_transaction(
        &svm,
        &payer,
        std::slice::from_ref(&instruction),
        v1::TransactionConfig::empty(),
    );
    let failure = svm.send_transaction(transaction).unwrap_err();
    assert_eq!(
        failure.err,
        TransactionError::MaxLoadedAccountsDataSizeExceeded
    );

    // Data size limit set, compute unit limit not: the program starts with a
    // budget of zero and fails on its first instruction.
    let config = v1::TransactionConfig::empty()
        .with_loaded_accounts_data_size_limit(LOADED_ACCOUNTS_DATA_SIZE_LIMIT);
    let transaction = v1_transaction(&svm, &payer, &[instruction], config);
    let failure = svm.send_transaction(transaction).unwrap_err();
    assert!(matches!(
        failure.err,
        TransactionError::InstructionError(0, _)
    ));
    assert!(
        failure
            .meta
            .logs
            .iter()
            .any(|log| log.contains("exceeded CUs")),
        "{:?}",
        failure.meta.logs
    );
}
