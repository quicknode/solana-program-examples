use {
    anchor_lang::{
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
        solana_program::instruction::Instruction, system_program, Address, InstructionData,
        ToAccountMetas,
    },
    solana_kite::{create_wallet, get_token_account_balance, send_transaction_from_instructions},
};

/// Decimals configured by the program's `mint::decimals` constraint in
/// `CreateTokenAccountConstraints`.
const MINT_DECIMALS: u32 = 9;

/// Converts a whole-token (major unit) count to minor units, the form the
/// program's instruction handlers take amounts in.
fn to_minor_units(major_units: u64) -> u64 {
    major_units.checked_mul(10u64.pow(MINT_DECIMALS)).unwrap()
}

fn metadata_program_id() -> Address {
    "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
        .parse()
        .unwrap()
}

fn token_program_id() -> Address {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

fn associated_token_program_id() -> Address {
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap()
}

fn rent_sysvar_id() -> Address {
    "SysvarRent111111111111111111111111111111111"
        .parse()
        .unwrap()
}

fn derive_metadata_pda(mint: &Address) -> Address {
    let metadata_pid = metadata_program_id();
    let (pda, _bump) = Address::find_program_address(
        &[b"metadata", metadata_pid.as_ref(), mint.as_ref()],
        &metadata_pid,
    );
    pda
}

fn derive_ata(wallet: &Address, mint: &Address) -> Address {
    let (ata, _bump) = Address::find_program_address(
        &[wallet.as_ref(), token_program_id().as_ref(), mint.as_ref()],
        &associated_token_program_id(),
    );
    ata
}

fn setup() -> (LiteSVM, Address, Keypair) {
    let program_id = transfer_tokens::id();
    let mut svm = anchor_v2_testing::svm();

    let program_bytes = include_bytes!("../../../target/deploy/transfer_tokens.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let metadata_bytes = include_bytes!("../../../tests/fixtures/mpl_token_metadata.so");
    svm.add_program(metadata_program_id(), metadata_bytes)
        .unwrap();

    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    (svm, program_id, payer)
}

#[test]
fn test_create_mint_and_transfer() {
    let (mut svm, program_id, payer) = setup();
    let mint_keypair = Keypair::new();
    let metadata_account = derive_metadata_pda(&mint_keypair.pubkey());

    // 1. Create token (with metadata)
    let create_ix = Instruction::new_with_bytes(
        program_id,
        &transfer_tokens::instruction::CreateToken {
            token_title: "Solana Gold".to_string(),
            token_symbol: "GOLDSOL".to_string(),
            token_uri: "https://example.com/token.json".to_string(),
        }
        .data(),
        transfer_tokens::accounts::CreateTokenAccountConstraints {
            payer: payer.pubkey(),
            mint_account: mint_keypair.pubkey(),
            metadata_account,
            token_program: token_program_id(),
            token_metadata_program: metadata_program_id(),
            system_program: system_program::ID,
            rent: rent_sysvar_id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut svm,
        vec![create_ix],
        &[&payer, &mint_keypair],
        &payer.pubkey(),
    )
    .unwrap();

    // Verify mint created
    let mint_account = svm
        .get_account(&mint_keypair.pubkey())
        .expect("Mint should exist");
    assert!(!mint_account.data.is_empty());

    // 2. Mint 100 tokens to payer's ATA. The handler takes minor units.
    svm.expire_blockhash();
    let sender_ata = derive_ata(&payer.pubkey(), &mint_keypair.pubkey());

    let mint_ix = Instruction::new_with_bytes(
        program_id,
        &transfer_tokens::instruction::MintToken {
            amount: to_minor_units(100),
        }
        .data(),
        transfer_tokens::accounts::MintTokenAccountConstraints {
            mint_authority: payer.pubkey(),
            recipient: payer.pubkey(),
            mint_account: mint_keypair.pubkey(),
            associated_token_account: sender_ata,
            token_program: token_program_id(),
            associated_token_program: associated_token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![mint_ix], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify 100 tokens minted (in minor units)
    assert_eq!(
        get_token_account_balance(&svm, &sender_ata).unwrap(),
        to_minor_units(100)
    );

    // 3. Transfer 50 tokens to recipient. The handler takes minor units.
    svm.expire_blockhash();
    let recipient = Keypair::new();
    let recipient_ata = derive_ata(&recipient.pubkey(), &mint_keypair.pubkey());

    let transfer_ix = Instruction::new_with_bytes(
        program_id,
        &transfer_tokens::instruction::TransferTokens {
            amount: to_minor_units(50),
        }
        .data(),
        transfer_tokens::accounts::TransferTokensAccountConstraints {
            sender: payer.pubkey(),
            recipient: recipient.pubkey(),
            mint_account: mint_keypair.pubkey(),
            sender_token_account: sender_ata,
            recipient_token_account: recipient_ata,
            token_program: token_program_id(),
            associated_token_program: associated_token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![transfer_ix], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify: sender 50 tokens, recipient 50 tokens (in minor units)
    assert_eq!(
        get_token_account_balance(&svm, &sender_ata).unwrap(),
        to_minor_units(50)
    );
    assert_eq!(
        get_token_account_balance(&svm, &recipient_ata).unwrap(),
        to_minor_units(50)
    );
}
