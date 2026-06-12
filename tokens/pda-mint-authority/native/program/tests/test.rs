use {
    litesvm::LiteSVM,
    pda_mint_authority_native_program::instructions::create::CreateTokenArgs,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::{Keypair, Signer},
    solana_native_token::LAMPORTS_PER_SOL,
    solana_program::program_pack::Pack,
    solana_pubkey::{pubkey, Pubkey},
    solana_transaction::Transaction,
    spl_token_interface::state::{Account as TokenAccount, Mint},
};

const TOKEN_METADATA_PROGRAM_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
const RENT_SYSVAR_ID: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");

#[test]
fn test_init_create_and_mint_with_pda_authority() {
    let mut svm = LiteSVM::new();

    let program_id = Pubkey::new_unique();
    // The .so is built into the workspace target/deploy by
    // `cargo build-sbf --manifest-path=./program/Cargo.toml` (run from the
    // project root). Rebuild after every program change: the binary is
    // embedded at test-compile time, so a stale .so silently tests old code.
    svm.add_program(
        program_id,
        include_bytes!("../../../../../target/deploy/pda_mint_authority_native_program.so"),
    )
    .unwrap();
    svm.add_program(
        TOKEN_METADATA_PROGRAM_ID,
        include_bytes!("../../tests/fixtures/mpl_token_metadata.so"),
    )
    .unwrap();

    let token_program_id = spl_token_interface::id();
    let ata_program_id = spl_associated_token_account_interface::program::id();
    let system_program_id = solana_system_interface::program::ID;

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    let (mint_authority, _b) = Pubkey::find_program_address(&[b"mint_authority"], &program_id);

    // --- Init the mint authority PDA (Init = variant 0, unit) ---
    let init_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint_authority, false),
            AccountMeta::new(payer.pubkey(), false),
            AccountMeta::new_readonly(system_program_id, false),
        ],
        data: vec![0u8],
    };
    let tx = Transaction::new_signed_with_payer(
        &[init_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    // The PDA exists and is owned by the program.
    assert_eq!(svm.get_account(&mint_authority).unwrap().owner, program_id);

    let mint = Keypair::new();
    let (metadata, _b) = Pubkey::find_program_address(
        &[
            b"metadata",
            TOKEN_METADATA_PROGRAM_ID.as_ref(),
            mint.pubkey().as_ref(),
        ],
        &TOKEN_METADATA_PROGRAM_ID,
    );
    let (edition, _b) = Pubkey::find_program_address(
        &[
            b"metadata",
            TOKEN_METADATA_PROGRAM_ID.as_ref(),
            mint.pubkey().as_ref(),
            b"edition",
        ],
        &TOKEN_METADATA_PROGRAM_ID,
    );

    // --- Create NFT mint + metadata, signed by the PDA (Create = variant 1) ---
    let mut create_data = vec![1u8];
    create_data.extend(
        borsh::to_vec(&CreateTokenArgs {
            nft_title: "Homer NFT".to_string(),
            nft_symbol: "HOMR".to_string(),
            nft_uri: "https://example.com/nft.json".to_string(),
        })
        .unwrap(),
    );
    let create_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), true),
            AccountMeta::new(mint_authority, false),
            AccountMeta::new(metadata, false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(RENT_SYSVAR_ID, false),
            AccountMeta::new_readonly(system_program_id, false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(TOKEN_METADATA_PROGRAM_ID, false),
        ],
        data: create_data,
    };
    let tx = Transaction::new_signed_with_payer(
        &[create_ix],
        Some(&payer.pubkey()),
        &[&payer, &mint],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(mint_account.owner, token_program_id);
    let mint_state = Mint::unpack(&mint_account.data).unwrap();
    assert_eq!(mint_state.decimals, 0);
    assert_eq!(mint_state.mint_authority.unwrap(), mint_authority);
    assert_eq!(
        svm.get_account(&metadata).unwrap().owner,
        TOKEN_METADATA_PROGRAM_ID
    );

    // --- Mint the NFT + master edition, signed by the PDA (Mint = variant 2) ---
    let ata = spl_associated_token_account_interface::address::get_associated_token_address(
        &payer.pubkey(),
        &mint.pubkey(),
    );
    let mint_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), false),
            AccountMeta::new(metadata, false),
            AccountMeta::new(edition, false),
            AccountMeta::new(mint_authority, false),
            AccountMeta::new(ata, false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(RENT_SYSVAR_ID, false),
            AccountMeta::new_readonly(system_program_id, false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(ata_program_id, false),
            AccountMeta::new_readonly(TOKEN_METADATA_PROGRAM_ID, false),
        ],
        data: vec![2u8],
    };
    let tx = Transaction::new_signed_with_payer(
        &[mint_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    // The ATA holds 1 NFT, master edition exists and owned by Token-Metadata.
    assert_eq!(
        TokenAccount::unpack(&svm.get_account(&ata).unwrap().data)
            .unwrap()
            .amount,
        1
    );
    assert_eq!(
        svm.get_account(&edition).unwrap().owner,
        TOKEN_METADATA_PROGRAM_ID
    );
}
