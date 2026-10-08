//! Send this example's test transactions in Solana's v1 transaction format
//! (SIMD-0385), live on mainnet since epoch 1035.
//!
//! `send_transaction_from_instructions` has the same name, arguments and error
//! type as `solana_kite::send_transaction_from_instructions`, so a test swaps
//! one import and nothing else. Kite's version builds a legacy transaction.
//!
//! A v1 transaction carries its resource limits in a `TransactionConfig` in the
//! message instead of in ComputeBudget instructions, and a limit it leaves
//! unset is zero rather than the legacy default. Every transaction built here
//! asks for what a legacy transaction gets without ComputeBudget instructions,
//! so the programs under test see the same budget they always did.

// Each test crate in this directory compiles its own copy of this module and
// uses only what it needs from it.
#![allow(dead_code)]

use {
    anchor_lang::solana_program::{instruction::Instruction, pubkey::Pubkey},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_kite::SolanaKiteError,
    solana_message::{v1, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

/// The default compute unit limit of a legacy transaction is 200,000 per
/// instruction.
const COMPUTE_UNITS_PER_INSTRUCTION: u32 = 200_000;

/// The most any transaction may ask for, and the cap on the legacy default.
const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

/// 64 MiB: the most account data any transaction may load, and the legacy
/// default.
const MAX_LOADED_ACCOUNTS_DATA_SIZE: u32 = 64 * 1024 * 1024;

/// The config a legacy transaction with these instructions would get by
/// default.
pub fn transaction_config(instruction_count: usize) -> v1::TransactionConfig {
    let compute_unit_limit = u32::try_from(instruction_count)
        .unwrap_or(u32::MAX)
        .saturating_mul(COMPUTE_UNITS_PER_INSTRUCTION)
        .min(MAX_COMPUTE_UNIT_LIMIT);
    v1::TransactionConfig::empty()
        .with_compute_unit_limit(compute_unit_limit)
        .with_loaded_accounts_data_size_limit(MAX_LOADED_ACCOUNTS_DATA_SIZE)
}

/// Compile and sign a v1 transaction. Panics if the instructions do not fit
/// a v1 message or the signers do not match it, as kite's legacy builder does.
pub fn v1_transaction(
    svm: &LiteSVM,
    instructions: &[Instruction],
    signers: &[&Keypair],
    fee_payer: &Pubkey,
) -> VersionedTransaction {
    let message = v1::Message::try_compile_with_config(
        fee_payer,
        instructions,
        svm.latest_blockhash(),
        transaction_config(instructions.len()),
    )
    .expect("instructions do not fit in a v1 message");

    // A legacy transaction accepts the same keypair twice (a fee payer that is
    // also a named signer, say); a v1 one wants each signer once.
    let mut unique_signers: Vec<&Keypair> = Vec::with_capacity(signers.len());
    for signer in signers {
        if !unique_signers
            .iter()
            .any(|seen| seen.pubkey() == signer.pubkey())
        {
            unique_signers.push(signer);
        }
    }

    VersionedTransaction::try_new(VersionedMessage::V1(message), &unique_signers)
        .expect("signers do not match the transaction")
}

/// `solana_kite::send_transaction_from_instructions`, sending a v1 transaction.
pub fn send_transaction_from_instructions(
    svm: &mut LiteSVM,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
    fee_payer: &Pubkey,
) -> Result<(), SolanaKiteError> {
    let transaction = v1_transaction(svm, &instructions, signers, fee_payer);
    svm.send_transaction(transaction)
        .map(|_| ())
        .map_err(|e| SolanaKiteError::TransactionFailed(format!("{e:?}")))
}
