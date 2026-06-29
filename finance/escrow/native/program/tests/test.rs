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
const CANCEL_OFFER: u8 = 2;

const DECIMALS: u8 = 6;
const MINTED_AMOUNT: u64 = 100 * 1_000_000; // 100 tokens at 6 decimals
const AMOUNT_A: u64 = 4 * 1_000_000; // offered
const AMOUNT_B: u64 = 1_000_000; // wanted
const OFFER_ID: u64 = 0;

/// Sign with `payer` (fee payer) plus any extra signers and send the tx,
/// asserting success.
fn send(svm: &mut LiteSVM, payer: &Keypair, ixs: &[Instruction], extra_signers: &[&Keypair]) {
    try_send(svm, payer, ixs, extra_signers).unwrap();
}

/// Sign with `payer` (fee payer) plus any extra signers and send the tx,
/// returning the result.
fn try_send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    ixs: &[Instruction],
    extra_signers: &[&Keypair],
) -> Result<(), Box<litesvm::types::FailedTransactionMetadata>> {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend_from_slice(extra_signers);
    let tx = Transaction::new_signed_with_payer(
        ixs,
        Some(&payer.pubkey()),
        &signers,
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).map(|_| ()).map_err(Box::new)
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

fn lamports(svm: &LiteSVM, address: &Pubkey) -> u64 {
    svm.get_account(address).map(|a| a.lamports).unwrap_or(0)
}

struct EscrowSetup {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    maker: Keypair,
    taker: Keypair,
    mint_a: Keypair,
    mint_b: Keypair,
    offer: Pubkey,
    vault: Pubkey,
    maker_account_a: Pubkey,
    maker_account_b: Pubkey,
    taker_account_a: Pubkey,
    taker_account_b: Pubkey,
}

fn setup() -> EscrowSetup {
    let mut svm = LiteSVM::new();
    let program_id = Pubkey::new_unique();
    // The .so is built into the local target/deploy by
    // `cargo build-sbf --manifest-path=./program/Cargo.toml` (run from the
    // project root). Rebuild after every program change: the binary is
    // embedded at test-compile time, so a stale .so silently tests old code.
    let program_bytes = include_bytes!("../../target/deploy/escrow_native_program.so");
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

    let (offer, _bump) = Pubkey::find_program_address(
        &[b"offer", maker.pubkey().as_ref(), &OFFER_ID.to_le_bytes()],
        &program_id,
    );
    let vault = get_associated_token_address(&offer, &mint_a.pubkey());
    let maker_account_a = get_associated_token_address(&maker.pubkey(), &mint_a.pubkey());
    let maker_account_b = get_associated_token_address(&maker.pubkey(), &mint_b.pubkey());
    let taker_account_a = get_associated_token_address(&taker.pubkey(), &mint_a.pubkey());
    let taker_account_b = get_associated_token_address(&taker.pubkey(), &mint_b.pubkey());

    EscrowSetup {
        svm,
        program_id,
        payer,
        maker,
        taker,
        mint_a,
        mint_b,
        offer,
        vault,
        maker_account_a,
        maker_account_b,
        taker_account_a,
        taker_account_b,
    }
}

fn make_offer_instruction(es: &EscrowSetup) -> Instruction {
    let mut make_data = vec![MAKE_OFFER];
    make_data.extend_from_slice(&OFFER_ID.to_le_bytes());
    make_data.extend_from_slice(&AMOUNT_A.to_le_bytes());
    make_data.extend_from_slice(&AMOUNT_B.to_le_bytes());

    Instruction {
        program_id: es.program_id,
        accounts: vec![
            AccountMeta::new(es.offer, false),
            AccountMeta::new_readonly(es.mint_a.pubkey(), false),
            AccountMeta::new_readonly(es.mint_b.pubkey(), false),
            AccountMeta::new(es.maker_account_a, false),
            AccountMeta::new(es.maker_account_b, false),
            AccountMeta::new(es.vault, false),
            AccountMeta::new(es.maker.pubkey(), true),
            AccountMeta::new_readonly(spl_token_interface::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account_interface::program::id(), false),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
        ],
        data: make_data,
    }
}

fn take_offer_instruction(es: &EscrowSetup) -> Instruction {
    Instruction {
        program_id: es.program_id,
        accounts: vec![
            AccountMeta::new(es.offer, false),
            AccountMeta::new_readonly(es.mint_a.pubkey(), false),
            AccountMeta::new_readonly(es.mint_b.pubkey(), false),
            AccountMeta::new(es.maker_account_b, false),
            AccountMeta::new(es.taker_account_a, false),
            AccountMeta::new(es.taker_account_b, false),
            AccountMeta::new(es.vault, false),
            AccountMeta::new(es.maker.pubkey(), false),
            AccountMeta::new(es.taker.pubkey(), true),
            AccountMeta::new_readonly(spl_token_interface::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account_interface::program::id(), false),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
        ],
        data: vec![TAKE_OFFER],
    }
}

fn cancel_offer_instruction(es: &EscrowSetup, canceller: &Pubkey) -> Instruction {
    Instruction {
        program_id: es.program_id,
        accounts: vec![
            AccountMeta::new(es.offer, false),
            AccountMeta::new_readonly(es.mint_a.pubkey(), false),
            AccountMeta::new(es.maker_account_a, false),
            AccountMeta::new(es.vault, false),
            AccountMeta::new(*canceller, true),
            AccountMeta::new_readonly(spl_token_interface::id(), false),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
        ],
        data: vec![CANCEL_OFFER],
    }
}

#[test]
fn test_escrow_make_and_take() {
    let mut es = setup();

    // Pre-create the maker's Mint B ATA (paid by the global payer) so the
    // maker's lamports can be compared exactly across make + take.
    let create_maker_ata_b = create_associated_token_account(
        &es.payer.pubkey(),
        &es.maker.pubkey(),
        &es.mint_b.pubkey(),
        &spl_token_interface::id(),
    );
    let payer = es.payer.insecure_clone();
    send(&mut es.svm, &payer, &[create_maker_ata_b], &[]);

    let maker_lamports_before_make = lamports(&es.svm, &es.maker.pubkey());
    let taker_lamports_before_take = lamports(&es.svm, &es.taker.pubkey());

    // ---- Make Offer ----
    let make_ix = make_offer_instruction(&es);
    let maker = es.maker.insecure_clone();
    send(&mut es.svm, &payer, &[make_ix], &[&maker]);

    // Vault holds the offered Mint A amount, and the maker paid the rent for
    // the offer account and the vault.
    assert_eq!(token_amount(&es.svm, &es.vault), AMOUNT_A);
    let offer_rent = lamports(&es.svm, &es.offer);
    let vault_rent = lamports(&es.svm, &es.vault);
    assert!(offer_rent > 0 && vault_rent > 0);
    assert_eq!(
        lamports(&es.svm, &es.maker.pubkey()),
        maker_lamports_before_make - offer_rent - vault_rent
    );

    // ---- Take Offer ----
    let take_ix = take_offer_instruction(&es);
    let taker = es.taker.insecure_clone();
    send(&mut es.svm, &payer, &[take_ix], &[&taker]);

    // Offer + vault are closed (zero-lamport accounts are purged).
    assert_eq!(lamports(&es.svm, &es.offer), 0);
    assert_eq!(lamports(&es.svm, &es.vault), 0);

    // Taker received Mint A; maker received Mint B.
    assert_eq!(token_amount(&es.svm, &es.taker_account_a), AMOUNT_A);
    assert_eq!(token_amount(&es.svm, &es.maker_account_b), AMOUNT_B);

    // Rent destinations: the maker's lamports fully recover (the offer and
    // vault rent both come back to the maker). The taker only paid the rent
    // for their own new Mint A ATA.
    assert_eq!(
        lamports(&es.svm, &es.maker.pubkey()),
        maker_lamports_before_make
    );
    let taker_ata_a_rent = lamports(&es.svm, &es.taker_account_a);
    assert_eq!(
        lamports(&es.svm, &es.taker.pubkey()),
        taker_lamports_before_take - taker_ata_a_rent
    );
}

#[test]
fn test_escrow_make_and_cancel() {
    let mut es = setup();
    let payer = es.payer.insecure_clone();
    let maker = es.maker.insecure_clone();

    let maker_lamports_before_make = lamports(&es.svm, &es.maker.pubkey());
    let maker_a_before_make = token_amount(&es.svm, &es.maker_account_a);

    // ---- Make Offer ----
    // The maker has no Mint B ATA yet; make_offer creates it, paid by the
    // maker.
    let make_ix = make_offer_instruction(&es);
    send(&mut es.svm, &payer, &[make_ix], &[&maker]);
    assert_eq!(token_amount(&es.svm, &es.vault), AMOUNT_A);
    let maker_ata_b_rent = lamports(&es.svm, &es.maker_account_b);
    assert!(maker_ata_b_rent > 0);

    // ---- Cancel Offer ----
    let cancel_ix = cancel_offer_instruction(&es, &es.maker.pubkey());
    send(&mut es.svm, &payer, &[cancel_ix], &[&maker]);

    // Offer + vault are closed.
    assert_eq!(lamports(&es.svm, &es.offer), 0);
    assert_eq!(lamports(&es.svm, &es.vault), 0);

    // The maker's Mint A tokens are back in full.
    assert_eq!(
        token_amount(&es.svm, &es.maker_account_a),
        maker_a_before_make
    );

    // Rent destinations: the offer and vault rent return to the maker. The
    // only lamports the maker is down is the rent of their still-open Mint B
    // ATA, created during make_offer.
    assert_eq!(
        lamports(&es.svm, &es.maker.pubkey()),
        maker_lamports_before_make - maker_ata_b_rent
    );
}

#[test]
fn test_cancel_offer_rejects_non_maker() {
    let mut es = setup();
    let payer = es.payer.insecure_clone();
    let maker = es.maker.insecure_clone();
    let taker = es.taker.insecure_clone();

    let make_ix = make_offer_instruction(&es);
    send(&mut es.svm, &payer, &[make_ix], &[&maker]);

    // The taker signs a cancel attempt. The offer's stored maker does not
    // match the signer, so the program must reject it.
    let cancel_ix = cancel_offer_instruction(&es, &es.taker.pubkey());
    let result = try_send(&mut es.svm, &payer, &[cancel_ix], &[&taker]);
    assert!(
        result.is_err(),
        "non-maker must not be able to cancel the offer"
    );

    // The vault still holds the offered tokens.
    assert_eq!(token_amount(&es.svm, &es.vault), AMOUNT_A);
}
