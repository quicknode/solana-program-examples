use {
    litesvm::LiteSVM,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::{Keypair, Signer},
    solana_native_token::LAMPORTS_PER_SOL,
    solana_program::program_pack::Pack,
    solana_pubkey::Pubkey,
    solana_system_interface::instruction::create_account,
    solana_transaction::Transaction,
    spl_associated_token_account_interface::{
        address::get_associated_token_address, instruction::create_associated_token_account,
    },
    spl_token_interface::{
        instruction::{initialize_mint2, mint_to},
        state::{Account as TokenAccount, Mint},
    },
};

// borsh-encoded `EscrowInstruction` discriminants (see program/src/lib.rs).
const MAKE_OFFER: u8 = 0;
const TAKE_OFFER: u8 = 1;

const DECIMALS: u8 = 6;
const MINTED_AMOUNT: u64 = 100 * 1_000_000; // 100 tokens at 6 decimals
const AMOUNT_A: u64 = 4 * 1_000_000; // offered
const AMOUNT_B: u64 = 1_000_000; // wanted
const OFFER_ID: u64 = 0;

/// Sign with `payer` (fee payer) plus any extra signers and send the tx,
/// asserting success.
fn send(svm: &mut LiteSVM, payer: &Keypair, ixs: &[Instruction], extra_signers: &[&Keypair]) {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra_signers);
    let tx = Transaction::new_signed_with_payer(
        ixs,
        Some(&payer.pubkey()),
        &signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();
}

/// Create `mint`, an ATA for `holder`, and mint `MINTED_AMOUNT` into it. The
/// payer is the mint + freeze authority.
fn mint_tokens(svm: &mut LiteSVM, payer: &Keypair, mint: &Keypair, holder: &Pubkey) {
    let token_program = spl_token_interface::id();
    let rent = svm.minimum_balance_for_rent_exemption(Mint::LEN);

    let create_mint_account = create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent,
        Mint::LEN as u64,
        &token_program,
    );
    let init_mint = initialize_mint2(
        &token_program,
        &mint.pubkey(),
        &payer.pubkey(),
        Some(&payer.pubkey()),
        DECIMALS,
    )
    .unwrap();
    send(svm, payer, &[create_mint_account, init_mint], &[mint]);

    let ata = get_associated_token_address(holder, &mint.pubkey());
    let create_ata =
        create_associated_token_account(&payer.pubkey(), holder, &mint.pubkey(), &token_program);
    let mint_to_ix = mint_to(
        &token_program,
        &mint.pubkey(),
        &ata,
        &payer.pubkey(),
        &[],
        MINTED_AMOUNT,
    )
    .unwrap();
    send(svm, payer, &[create_ata, mint_to_ix], &[]);
}

fn token_amount(svm: &LiteSVM, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    TokenAccount::unpack(&account.data).unwrap().amount
}

#[test]
fn test_escrow_make_and_take() {
    let mut svm = LiteSVM::new();
    let program_id = Pubkey::new_unique();
    let program_bytes = include_bytes!("../../tests/fixtures/escrow_native_program.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 100)
        .unwrap();

    let maker = Keypair::new();
    let taker = Keypair::new();
    let mint_a = Keypair::new();
    let mint_b = Keypair::new();
    svm.airdrop(&maker.pubkey(), LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&taker.pubkey(), LAMPORTS_PER_SOL).unwrap();

    // Maker holds Mint A, taker holds Mint B.
    mint_tokens(&mut svm, &payer, &mint_a, &maker.pubkey());
    mint_tokens(&mut svm, &payer, &mint_b, &taker.pubkey());

    let token_program = spl_token_interface::id();
    let ata_program = spl_associated_token_account_interface::program::id();
    let system_program = solana_system_interface::program::ID;

    let (offer, _bump) = Pubkey::find_program_address(
        &[b"offer", maker.pubkey().as_ref(), &OFFER_ID.to_le_bytes()],
        &program_id,
    );
    let vault = get_associated_token_address(&offer, &mint_a.pubkey());
    let maker_account_a = get_associated_token_address(&maker.pubkey(), &mint_a.pubkey());
    let maker_account_b = get_associated_token_address(&maker.pubkey(), &mint_b.pubkey());
    let taker_account_a = get_associated_token_address(&taker.pubkey(), &mint_a.pubkey());
    let taker_account_b = get_associated_token_address(&taker.pubkey(), &mint_b.pubkey());

    // ---- Make Offer ----
    let mut make_data = vec![MAKE_OFFER];
    make_data.extend_from_slice(&OFFER_ID.to_le_bytes());
    make_data.extend_from_slice(&AMOUNT_A.to_le_bytes());
    make_data.extend_from_slice(&AMOUNT_B.to_le_bytes());

    let make_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(offer, false),
            AccountMeta::new_readonly(mint_a.pubkey(), false),
            AccountMeta::new_readonly(mint_b.pubkey(), false),
            AccountMeta::new(maker_account_a, false),
            AccountMeta::new(vault, false),
            AccountMeta::new(maker.pubkey(), true),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(ata_program, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: make_data,
    };
    send(&mut svm, &payer, &[make_ix], &[&maker]);

    // Vault should hold the offered Mint A amount.
    assert_eq!(token_amount(&svm, &vault), AMOUNT_A);

    // ---- Take Offer ----
    let take_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(offer, false),
            AccountMeta::new_readonly(mint_a.pubkey(), false),
            AccountMeta::new_readonly(mint_b.pubkey(), false),
            AccountMeta::new(maker_account_b, false),
            AccountMeta::new(taker_account_a, false),
            AccountMeta::new(taker_account_b, false),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(maker.pubkey(), false),
            AccountMeta::new(taker.pubkey(), true),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(ata_program, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: vec![TAKE_OFFER],
    };
    send(&mut svm, &payer, &[take_ix], &[&taker]);

    // Offer + vault should be closed (zero-lamport accounts are purged).
    assert!(svm.get_account(&offer).map(|a| a.lamports).unwrap_or(0) == 0);
    assert!(svm.get_account(&vault).map(|a| a.lamports).unwrap_or(0) == 0);

    // Taker received Mint A; maker received Mint B.
    assert_eq!(token_amount(&svm, &taker_account_a), AMOUNT_A);
    assert_eq!(token_amount(&svm, &maker_account_b), AMOUNT_B);
}
