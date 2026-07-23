use {crate::cpi::InitializeInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([2; 32]);
const MINT: Pubkey = Pubkey::new_from_array([3; 32]);

/// Initialize creates a Token-2022 token account with the ImmutableOwner
/// extension.
#[quasar_test]
fn initialize_creates_an_immutable_owner_token_account(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    test.add(
        Mint::new(PAYER)
            .at(MINT)
            .decimals(2)
            .token_program(TokenProgram::Token2022),
    );
    // The token account enters the transaction as an empty system account;
    // the program creates and initializes it via CPI.

    test.send(InitializeInstruction {
        payer: PAYER,
        token_account: TOKEN_ACCOUNT,
        mint_account: MINT,
    })
    .succeeds();

    let token_account = test.account(TOKEN_ACCOUNT).expect("token account exists");
    assert_eq!(token_account.owner, SPL_TOKEN_2022_PROGRAM_ID);
    // 165 base + 1 account-type byte + 4 TLV header (ImmutableOwner is
    // zero-size) = 170 bytes.
    assert_eq!(token_account.data.len(), 170);
    // Balance is zero. has_tokens can't be used here: it unpacks the strict
    // 165-byte base layout, which rejects extended Token-2022 accounts. The
    // amount field lives at bytes 64..72 in both layouts.
    assert_eq!(&token_account.data[64..72], &0u64.to_le_bytes());
}
