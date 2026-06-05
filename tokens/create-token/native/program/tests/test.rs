use {
    litesvm::LiteSVM,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::{Keypair, Signer},
    solana_native_token::LAMPORTS_PER_SOL,
    solana_pubkey::{pubkey, Pubkey},
    solana_transaction::Transaction,
    spl_token_interface::state::Mint,
    {create_token_program::CreateTokenArgs, solana_program::program_pack::Pack},
};

// SPL Token-Metadata program id (the program loaded from the fixture .so).
const TOKEN_METADATA_PROGRAM_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
const RENT_SYSVAR_ID: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");

#[test]
fn test_create_token_with_metadata() {
    let mut svm = LiteSVM::new();

    let program_id = Pubkey::new_unique();
    svm.add_program(
        program_id,
        include_bytes!("../../tests/fixtures/create_token_program.so"),
    )
    .unwrap();

    // litesvm bundles SPL Token but not Token-Metadata, so load it from the
    // fixture .so at its canonical address.
    svm.add_program(
        TOKEN_METADATA_PROGRAM_ID,
        include_bytes!("../../tests/fixtures/mpl_token_metadata.so"),
    )
    .unwrap();

    let token_program_id = spl_token_interface::id();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    let mint = Keypair::new();
    let (metadata, _bump) = Pubkey::find_program_address(
        &[
            b"metadata",
            TOKEN_METADATA_PROGRAM_ID.as_ref(),
            mint.pubkey().as_ref(),
        ],
        &TOKEN_METADATA_PROGRAM_ID,
    );

    let data = borsh::to_vec(&CreateTokenArgs {
        token_title: "Solana Gold".to_string(),
        token_symbol: "GOLDSOL".to_string(),
        token_uri: "https://example.com/spl-token.json".to_string(),
        token_decimals: 9,
    })
    .unwrap();

    // payer doubles as the mint authority (matches the original example).
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), true),   // mint account
            AccountMeta::new(payer.pubkey(), false), // mint authority
            AccountMeta::new(metadata, false),       // metadata account
            AccountMeta::new(payer.pubkey(), true),  // payer
            AccountMeta::new_readonly(RENT_SYSVAR_ID, false),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(TOKEN_METADATA_PROGRAM_ID, false),
        ],
        data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &mint],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    // The mint exists, is owned by the token program, and has the right decimals.
    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(mint_account.owner, token_program_id);
    let mint_state = Mint::unpack(&mint_account.data).unwrap();
    assert_eq!(mint_state.decimals, 9);

    // The metadata account exists and is owned by the Token-Metadata program.
    let metadata_account = svm.get_account(&metadata).unwrap();
    assert_eq!(metadata_account.owner, TOKEN_METADATA_PROGRAM_ID);
    assert!(!metadata_account.data.is_empty());
}
