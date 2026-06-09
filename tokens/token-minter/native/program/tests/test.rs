use {
    litesvm::LiteSVM,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::{Keypair, Signer},
    solana_native_token::LAMPORTS_PER_SOL,
    solana_program::program_pack::Pack,
    solana_pubkey::{pubkey, Pubkey},
    solana_transaction::Transaction,
    spl_token_interface::state::{Account as TokenAccount, Mint},
    token_minter_native_program::instructions::{create::CreateTokenArgs, mint::MintToArgs},
};

// SPL Token-Metadata program id (the program loaded from the fixture .so).
const TOKEN_METADATA_PROGRAM_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
const RENT_SYSVAR_ID: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");

// Borsh serializes the program's instruction enum as a single u8 discriminant
// followed by the variant payload. `Create` is variant 0, `Mint` is variant 1.
fn create_ix_data(args: &CreateTokenArgs) -> Vec<u8> {
    let mut data = vec![0u8];
    data.extend(borsh::to_vec(args).unwrap());
    data
}

fn mint_ix_data(args: &MintToArgs) -> Vec<u8> {
    let mut data = vec![1u8];
    data.extend(borsh::to_vec(args).unwrap());
    data
}

#[test]
fn test_create_and_mint() {
    let mut svm = LiteSVM::new();

    let program_id = Pubkey::new_unique();
    svm.add_program(
        program_id,
        include_bytes!("../../tests/fixtures/token_minter_native_program.so"),
    )
    .unwrap();
    svm.add_program(
        TOKEN_METADATA_PROGRAM_ID,
        include_bytes!("../../tests/fixtures/mpl_token_metadata.so"),
    )
    .unwrap();

    let token_program_id = spl_token_interface::id();
    let ata_program_id = spl_associated_token_account_interface::program::id();

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

    // --- Create the token ---
    let create_ix = Instruction {
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
        data: create_ix_data(&CreateTokenArgs {
            token_title: "Solana Gold".to_string(),
            token_symbol: "GOLDSOL".to_string(),
            token_uri: "https://example.com/spl-token.json".to_string(),
        }),
    };
    let tx = Transaction::new_signed_with_payer(
        &[create_ix],
        Some(&payer.pubkey()),
        &[&payer, &mint],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    // The mint exists, is owned by the token program, has 9 decimals.
    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(mint_account.owner, token_program_id);
    let mint_state = Mint::unpack(&mint_account.data).unwrap();
    assert_eq!(mint_state.decimals, 9);

    // Metadata account exists and is owned by Token-Metadata.
    let metadata_account = svm.get_account(&metadata).unwrap();
    assert_eq!(metadata_account.owner, TOKEN_METADATA_PROGRAM_ID);

    // --- Mint tokens to payer's ATA ---
    let ata = spl_associated_token_account_interface::address::get_associated_token_address(
        &payer.pubkey(),
        &mint.pubkey(),
    );

    let mint_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), false),  // mint account
            AccountMeta::new(payer.pubkey(), false), // mint authority
            AccountMeta::new(ata, false),            // ATA
            AccountMeta::new(payer.pubkey(), true),  // payer
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(ata_program_id, false),
        ],
        data: mint_ix_data(&MintToArgs { quantity: 150 }),
    };
    let tx = Transaction::new_signed_with_payer(
        &[mint_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    // The ATA exists, holds 150 tokens of the mint, owned by the token program.
    let ata_account = svm.get_account(&ata).unwrap();
    assert_eq!(ata_account.owner, token_program_id);
    let token_state = TokenAccount::unpack(&ata_account.data).unwrap();
    assert_eq!(token_state.mint, mint.pubkey());
    assert_eq!(token_state.owner, payer.pubkey());
    assert_eq!(token_state.amount, 150);
}
