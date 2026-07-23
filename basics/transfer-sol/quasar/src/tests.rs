use {
    crate::{
        cpi::{TransferSolWithCpiInstruction, TransferSolWithProgramInstruction},
        instructions::TransferSolError,
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const RECIPIENT: Pubkey = Pubkey::new_from_array([2; 32]);

/// Install a zero-data account owned by this program: direct lamport
/// manipulation is only allowed on program-owned accounts.
fn add_program_owned_account(test: &mut Test, address: Pubkey, lamports: u64) {
    test.set_account(Account::new(
        address,
        Pubkey::from(crate::ID),
        lamports,
        vec![],
    ));
}

#[quasar_test]
fn transfer_with_cpi_moves_lamports(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let amount = 1_000_000_000; // 1 SOL

    // The recipient starts empty: missing writable accounts enter the
    // transaction as empty system accounts automatically.
    test.send(TransferSolWithCpiInstruction {
        payer: PAYER,
        recipient: RECIPIENT,
        amount,
    })
    .succeeds()
    .has_lamports(PAYER, DEFAULT_WALLET_LAMPORTS - amount)
    .has_lamports(RECIPIENT, amount);
}

#[quasar_test]
fn transfer_with_program_moves_lamports_directly(test: &mut Test) {
    let amount = 500_000_000; // 0.5 SOL

    // The payer must be owned by our program for direct lamport manipulation.
    add_program_owned_account(test, PAYER, 2_000_000_000);
    add_program_owned_account(test, RECIPIENT, 1_000_000_000);

    test.send(TransferSolWithProgramInstruction {
        payer: PAYER,
        recipient: RECIPIENT,
        amount,
    })
    .succeeds()
    .has_lamports(PAYER, 2_000_000_000 - amount)
    .has_lamports(RECIPIENT, 1_000_000_000 + amount);
}

#[quasar_test]
fn transfer_with_program_rejects_a_foreign_owned_payer(test: &mut Test) {
    let amount = 500_000_000; // 0.5 SOL

    // The payer is owned by the system program, not this program, so the
    // owner constraint must reject the transfer before any lamports move.
    test.add(Wallet::new().at(PAYER).lamports(2_000_000_000));
    add_program_owned_account(test, RECIPIENT, 1_000_000_000);

    test.send(TransferSolWithProgramInstruction {
        payer: PAYER,
        recipient: RECIPIENT,
        amount,
    })
    .fails_with(TransferSolError::PayerNotOwnedByProgram);
}

#[quasar_test]
fn transfer_with_program_rejects_insufficient_funds(test: &mut Test) {
    let payer_lamports = 100_000_000; // 0.1 SOL
    let amount = 500_000_000; // 0.5 SOL, more than the payer holds

    add_program_owned_account(test, PAYER, payer_lamports);
    add_program_owned_account(test, RECIPIENT, 1_000_000_000);

    test.send(TransferSolWithProgramInstruction {
        payer: PAYER,
        recipient: RECIPIENT,
        amount,
    })
    .fails_with(TransferSolError::InsufficientFunds);

    // No lamports moved.
    assert_eq!(test.lamports(PAYER), payer_lamports);
}
