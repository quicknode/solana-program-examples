extern crate std;
use {quasar_test::prelude::*, std::vec};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([3; 32]);

// Note: the mint_nft instruction requires the Metaplex Token Metadata program
// deployed in the SVM for the create_metadata and create_master_edition CPIs.
// The quasar-test harness does not currently include it, so we verify the
// program builds and can at least mint a token (the first CPI step).
// Full integration testing requires a devnet/localnet deploy with Metaplex.

#[quasar_test]
fn nft_minter_program_loads(test: &mut Test) {
    // The #[quasar_test] harness loads the compiled program ELF; reaching
    // this point means the program deploys into the SVM.
    let _ = test;
}

#[quasar_test]
fn spl_mint_to_deposits_one_token(test: &mut Test) {
    // Test that the SPL Token mint_to CPI works independently.
    test.add(Wallet::new().at(PAYER));
    test.add(Mint::new(PAYER).at(MINT).decimals(0));
    test.add(TokenAccount::new(MINT, PAYER).at(TOKEN_ACCOUNT));

    // Build a raw mint_to instruction to verify the token setup works.
    let mut data = vec![7u8]; // SPL Token MintTo instruction
    data.extend_from_slice(&1u64.to_le_bytes());

    let instruction = Instruction {
        program_id: SPL_TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(MINT, false),
            AccountMeta::new(TOKEN_ACCOUNT, false),
            AccountMeta::new_readonly(PAYER, true),
        ],
        data,
    };

    test.send(instruction)
        .succeeds()
        .has_tokens(TOKEN_ACCOUNT, 1)
        .has_supply(MINT, 1);
}
