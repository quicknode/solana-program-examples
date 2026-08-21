use {
    anchor_lang::{
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
        solana_program::instruction::Instruction, system_program, Address, InstructionData,
        ToAccountMetas,
    },
    solana_kite::{
        assert_token_account_balance, create_wallet, send_transaction_from_instructions,
        token_extensions::{get_token_extensions_account_address, TOKEN_EXTENSIONS_PROGRAM_ID},
    },
};

fn associated_token_program_id() -> Address {
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap()
}

fn setup() -> (LiteSVM, Address, Keypair) {
    let program_id = anchor::id();
    let mut svm = anchor_v2_testing::svm();

    let program_bytes = include_bytes!("../../../target/deploy/anchor.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    (svm, program_id, payer)
}

#[test]
fn test_create_token_and_mint_and_transfer() {
    let (mut svm, program_id, payer) = setup();

    let token_name = "TestToken".to_string();

    // Derive the mint PDA
    let (mint, _bump) = Address::find_program_address(
        &[
            b"token-2022-token",
            payer.pubkey().as_ref(),
            token_name.as_bytes(),
        ],
        &program_id,
    );

    // Step 1: Create Token via the program's own instruction
    let create_token_ix = Instruction::new_with_bytes(
        program_id,
        &anchor::instruction::CreateToken {
            token_name: token_name.clone(),
        }
        .data(),
        anchor::accounts::CreateTokenAccountConstraints {
            signer: payer.pubkey(),
            mint,
            system_program: system_program::ID,
            token_program: TOKEN_EXTENSIONS_PROGRAM_ID,
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(&mut svm, vec![create_token_ix], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify mint account exists
    let mint_account = svm.get_account(&mint).expect("Mint account should exist");
    assert!(!mint_account.data.is_empty(), "Mint should have data");

    svm.expire_blockhash();

    // Step 2: Create Associated Token Account for payer via the program's own instruction
    let payer_ata = get_token_extensions_account_address(&payer.pubkey(), &mint);

    let create_ata_ix = Instruction::new_with_bytes(
        program_id,
        &anchor::instruction::CreateAssociatedTokenAccount {}.data(),
        anchor::accounts::CreateAssociatedTokenAccountAccountConstraints {
            signer: payer.pubkey(),
            mint,
            token_account: payer_ata,
            system_program: system_program::ID,
            token_program: TOKEN_EXTENSIONS_PROGRAM_ID,
            associated_token_program: associated_token_program_id(),
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(&mut svm, vec![create_ata_ix], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify ATA exists
    let ata_account = svm.get_account(&payer_ata).expect("Payer ATA should exist");
    assert!(!ata_account.data.is_empty(), "ATA should have data");

    svm.expire_blockhash();

    // Step 3: Mint tokens to payer ATA via the program's own instruction
    let mint_amount: u64 = 200_000_000;

    let mint_token_ix = Instruction::new_with_bytes(
        program_id,
        &anchor::instruction::MintToken {
            amount: mint_amount,
        }
        .data(),
        anchor::accounts::MintTokenAccountConstraints {
            signer: payer.pubkey(),
            mint,
            receiver: payer_ata,
            token_program: TOKEN_EXTENSIONS_PROGRAM_ID,
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(&mut svm, vec![mint_token_ix], &[&payer], &payer.pubkey())
        .unwrap();

    assert_token_account_balance(
        &svm,
        &payer_ata,
        mint_amount,
        &format!("Should have minted {} tokens", mint_amount),
    );

    svm.expire_blockhash();

    // Step 4: Transfer tokens to receiver via the program's own instruction
    let receiver = create_wallet(&mut svm, 1_000_000_000).unwrap();
    let receiver_ata = get_token_extensions_account_address(&receiver.pubkey(), &mint);

    let transfer_amount: u64 = 100;

    let transfer_ix = Instruction::new_with_bytes(
        program_id,
        &anchor::instruction::TransferToken {
            amount: transfer_amount,
        }
        .data(),
        anchor::accounts::TransferTokenAccountConstraints {
            signer: payer.pubkey(),
            from: payer_ata,
            to: receiver.pubkey(),
            to_ata: receiver_ata,
            mint,
            token_program: TOKEN_EXTENSIONS_PROGRAM_ID,
            system_program: system_program::ID,
            associated_token_program: associated_token_program_id(),
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(&mut svm, vec![transfer_ix], &[&payer], &payer.pubkey())
        .unwrap();

    assert_token_account_balance(
        &svm,
        &receiver_ata,
        transfer_amount,
        &format!("Receiver should have {} tokens", transfer_amount),
    );
    assert_token_account_balance(
        &svm,
        &payer_ata,
        mint_amount - transfer_amount,
        "Payer should have remaining tokens",
    );
}
