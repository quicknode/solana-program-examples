use {crate::cpi::InitializeGroupInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);

/// Initialize creates a Token-2022 mint with the GroupPointer extension.
#[quasar_test]
fn initialize_group_creates_a_group_pointer_mint(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    // The mint enters the transaction as an empty system account; the program
    // creates and initializes it via CPI.

    test.send(InitializeGroupInstruction {
        payer: PAYER,
        mint_account: MINT,
    })
    .succeeds();

    let mint = test.account(MINT).expect("mint account exists");
    assert_eq!(mint.owner, SPL_TOKEN_2022_PROGRAM_ID);
    // Base mint padded to 165 + account-type byte + GroupPointer TLV
    // (2 type + 2 len + 64 data) = 234 bytes.
    assert_eq!(mint.data.len(), 234);
}
