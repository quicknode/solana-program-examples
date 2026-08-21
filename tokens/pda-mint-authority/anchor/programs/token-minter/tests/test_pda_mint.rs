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
/// program's `mint_token` handler takes amounts in.
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
    let program_id = token_minter::id();
    let mut svm = anchor_v2_testing::svm();

    let program_bytes = include_bytes!("../../../target/deploy/token_minter.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let metadata_bytes = include_bytes!("../../../tests/fixtures/mpl_token_metadata.so");
    svm.add_program(metadata_program_id(), metadata_bytes)
        .unwrap();

    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    (svm, program_id, payer)
}

#[test]
fn test_create_token_and_mint() {
    let (mut svm, program_id, payer) = setup();

    // Derive the PDA mint account (seeds = [b"mint"])
    let (mint_pda, _bump) = Address::find_program_address(&[b"mint"], &program_id);
    let metadata_account = derive_metadata_pda(&mint_pda);

    // 1. Create token
    let create_ix = Instruction::new_with_bytes(
        program_id,
        &token_minter::instruction::CreateToken {
            token_name: "Solana Gold".to_string(),
            token_symbol: "GOLDSOL".to_string(),
            token_uri: "https://example.com/token.json".to_string(),
        }
        .data(),
        token_minter::accounts::CreateTokenAccountConstraints {
            payer: payer.pubkey(),
            mint_account: mint_pda,
            metadata_account,
            token_program: token_program_id(),
            token_metadata_program: metadata_program_id(),
            system_program: system_program::ID,
            rent: rent_sysvar_id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![create_ix], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify mint created
    let mint_account = svm.get_account(&mint_pda).expect("Mint PDA should exist");
    assert!(!mint_account.data.is_empty());

    // Verify metadata created
    let meta = svm
        .get_account(&metadata_account)
        .expect("Metadata should exist");
    assert!(!meta.data.is_empty());

    // 2. Mint 100 tokens to the payer's ATA. The handler takes minor units.
    svm.expire_blockhash();
    let ata = derive_ata(&payer.pubkey(), &mint_pda);

    let mint_ix = Instruction::new_with_bytes(
        program_id,
        &token_minter::instruction::MintToken {
            amount: to_minor_units(100),
        }
        .data(),
        token_minter::accounts::MintTokenAccountConstraints {
            payer: payer.pubkey(),
            mint_account: mint_pda,
            associated_token_account: ata,
            token_program: token_program_id(),
            associated_token_program: associated_token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![mint_ix], &[&payer], &payer.pubkey())
        .unwrap();

    // Verify 100 tokens minted (in minor units)
    let balance = get_token_account_balance(&svm, &ata).unwrap();
    assert_eq!(balance, to_minor_units(100), "Should have 100 tokens");
}
