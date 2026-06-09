use {
    litesvm::LiteSVM,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::{Keypair, Signer},
    solana_native_token::LAMPORTS_PER_SOL,
    solana_program::program_pack::Pack,
    solana_pubkey::{pubkey, Pubkey},
    solana_transaction::Transaction,
    spl_token_interface::state::{Account as TokenAccount, Mint},
    transfer_tokens_program::instructions::{
        create::CreateTokenArgs, mint_spl::MintSplArgs, transfer::TransferTokensArgs,
    },
};

const TOKEN_METADATA_PROGRAM_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
const RENT_SYSVAR_ID: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");

// Instruction enum discriminants: Create=0, MintNft=1, MintSpl=2, TransferTokens=3.
fn ix_data<T: borsh::BorshSerialize>(discriminant: u8, args: &T) -> Vec<u8> {
    let mut data = vec![discriminant];
    data.extend(borsh::to_vec(args).unwrap());
    data
}

#[test]
fn test_create_mint_and_transfer_spl() {
    let mut svm = LiteSVM::new();

    let program_id = Pubkey::new_unique();
    svm.add_program(
        program_id,
        include_bytes!("../../tests/fixtures/transfer_tokens_program.so"),
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

    let recipient = Keypair::new();
    svm.airdrop(&recipient.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let mint = Keypair::new();
    let (metadata, _bump) = Pubkey::find_program_address(
        &[
            b"metadata",
            TOKEN_METADATA_PROGRAM_ID.as_ref(),
            mint.pubkey().as_ref(),
        ],
        &TOKEN_METADATA_PROGRAM_ID,
    );

    // --- Create the SPL token (9 decimals) ---
    let create_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), true),
            AccountMeta::new(payer.pubkey(), false),
            AccountMeta::new(metadata, false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(RENT_SYSVAR_ID, false),
            AccountMeta::new_readonly(system_program_id, false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(TOKEN_METADATA_PROGRAM_ID, false),
        ],
        data: ix_data(
            0,
            &CreateTokenArgs {
                token_title: "Solana Gold".to_string(),
                token_symbol: "GOLDSOL".to_string(),
                token_uri: "https://example.com/spl-token.json".to_string(),
                decimals: 9,
            },
        ),
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
    assert_eq!(Mint::unpack(&mint_account.data).unwrap().decimals, 9);
    assert_eq!(
        svm.get_account(&metadata).unwrap().owner,
        TOKEN_METADATA_PROGRAM_ID
    );

    // --- Mint 150 tokens to payer's ATA ---
    let payer_ata = spl_associated_token_account_interface::address::get_associated_token_address(
        &payer.pubkey(),
        &mint.pubkey(),
    );
    let mint_spl_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), false),
            AccountMeta::new(payer.pubkey(), false),
            AccountMeta::new(payer_ata, false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(system_program_id, false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(ata_program_id, false),
        ],
        data: ix_data(2, &MintSplArgs { quantity: 150 }),
    };
    let tx = Transaction::new_signed_with_payer(
        &[mint_spl_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    assert_eq!(
        TokenAccount::unpack(&svm.get_account(&payer_ata).unwrap().data)
            .unwrap()
            .amount,
        150
    );

    // --- Transfer 15 tokens to recipient (creates their ATA) ---
    let recipient_ata =
        spl_associated_token_account_interface::address::get_associated_token_address(
            &recipient.pubkey(),
            &mint.pubkey(),
        );
    let transfer_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), false),
            AccountMeta::new(payer_ata, false),
            AccountMeta::new(recipient_ata, false),
            AccountMeta::new(payer.pubkey(), true), // owner
            AccountMeta::new(recipient.pubkey(), true), // recipient
            AccountMeta::new(payer.pubkey(), true), // payer
            AccountMeta::new_readonly(system_program_id, false),
            AccountMeta::new_readonly(token_program_id, false),
            AccountMeta::new_readonly(ata_program_id, false),
        ],
        data: ix_data(3, &TransferTokensArgs { quantity: 15 }),
    };
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&payer.pubkey()),
        &[&payer, &recipient],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    // Recipient ATA holds 15, payer ATA holds 135.
    let recipient_state =
        TokenAccount::unpack(&svm.get_account(&recipient_ata).unwrap().data).unwrap();
    assert_eq!(recipient_state.amount, 15);
    assert_eq!(recipient_state.owner, recipient.pubkey());
    assert_eq!(
        TokenAccount::unpack(&svm.get_account(&payer_ata).unwrap().data)
            .unwrap()
            .amount,
        135
    );
}
