use {crate::cpi::MintTokenInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const AUTHORITY: Pubkey = Pubkey::new_from_array([1; 32]);
const RECIPIENT: Pubkey = Pubkey::new_from_array([2; 32]);
const MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([4; 32]);

/// Decimals configured by the mint fixture below, matching the program's
/// `mint(decimals = 9)` constraint in `CreateTokenAccountConstraints`.
const MINT_DECIMALS: u32 = 9;

/// Converts a whole-token (major unit) count to minor units, the form the
/// program's `mint_token` handler takes amounts in.
fn to_minor_units(major_units: u64) -> u64 {
    major_units.checked_mul(10u64.pow(MINT_DECIMALS)).unwrap()
}

// Note: the create_token test requires the Metaplex Token Metadata program
// deployed in the SVM. The quasar-test harness does not currently ship it,
// so we test mint_token (pure SPL Token CPI) only.

#[quasar_test]
fn mint_token_mints_the_exact_minor_unit_amount(test: &mut Test) {
    test.add(Wallet::new().at(AUTHORITY));
    test.add(Wallet::new().at(RECIPIENT));
    test.add(Mint::new(AUTHORITY).at(MINT).decimals(9));
    test.add(TokenAccount::new(MINT, RECIPIENT).at(TOKEN_ACCOUNT));

    let amount = to_minor_units(100);

    // The recipient's token account balance is the exact minor-unit amount
    // requested - the program performs no onchain scaling.
    test.send(MintTokenInstruction {
        mint_authority: AUTHORITY,
        recipient: RECIPIENT,
        mint_account: MINT,
        associated_token_account: TOKEN_ACCOUNT,
        amount,
    })
    .succeeds()
    .has_tokens(TOKEN_ACCOUNT, amount);
}
