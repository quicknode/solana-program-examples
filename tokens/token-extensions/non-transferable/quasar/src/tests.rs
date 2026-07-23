use {crate::cpi::InitializeInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);

/// Initialize creates a Token-2022 mint with the NonTransferable extension
/// (soulbound token).
#[quasar_test]
fn initialize_creates_a_non_transferable_mint(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    // The mint enters the transaction as an empty system account; the program
    // creates and initializes it via CPI.

    test.send(InitializeInstruction {
        payer: PAYER,
        mint_account: MINT,
    })
    .succeeds();

    let mint = test.account(MINT).expect("mint account exists");
    assert_eq!(mint.owner, SPL_TOKEN_2022_PROGRAM_ID);
    // 165 base + 1 account-type byte + 4 TLV header (NonTransferable is
    // zero-size) = 170 bytes.
    assert_eq!(mint.data.len(), 170);
}
