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

// The story the tests tell: the maker (Alice) offers 250 USDC (mint A) and
// wants 1 TSLAx (mint B) for it, and the taker (Bob) takes the offer. USDC
// has 6 decimals and TSLAx has 8, so every amount below is in those minor
// units.
const USDC_DECIMALS: u8 = 6;
const TSLAX_DECIMALS: u8 = 8;
const ONE_USDC: u64 = 10u64.pow(USDC_DECIMALS as u32);
const ONE_TSLAX: u64 = 10u64.pow(TSLAX_DECIMALS as u32);
const USDC_OFFERED: u64 = 250 * ONE_USDC;
const TSLAX_WANTED: u64 = ONE_TSLAX;
// What each side holds before the offer: both start with the standard
// wallet of 1 SOL and 1,000 USDC, and the taker also holds 1 TSLAx.
const MAKER_USDC: u64 = 1_000 * ONE_USDC;
const TAKER_USDC: u64 = 1_000 * ONE_USDC;
const TAKER_TSLAX: u64 = ONE_TSLAX;
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

/// Create `mint` with `decimals`, an ATA for `holder`, and mint `amount` into
/// it. The payer is the mint + freeze authority.
fn mint_tokens(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Keypair,
    decimals: u8,
    holder: &Pubkey,
    amount: u64,
) {
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
        decimals,
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
        amount,
    )
    .unwrap();
    send(svm, payer, &[create_ata, mint_to_ix], &[]);
}

/// Create `holder`'s ATA for an existing `mint` and mint `amount` into it. The
/// payer is the mint authority and pays the ATA's rent.
fn fund_token_account(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    holder: &Pubkey,
    amount: u64,
) {
    let token_program = spl_token_interface::id();
    let ata = get_associated_token_address(holder, mint);
    let create_ata = create_associated_token_account(&payer.pubkey(), holder, mint, &token_program);
    let mint_to_ix = mint_to(&token_program, mint, &ata, &payer.pubkey(), &[], amount).unwrap();
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

    // Mint A is USDC, held by the maker; mint B is TSLAx, held by the taker.
    mint_tokens(
        &mut svm,
        &payer,
        &mint_a,
        USDC_DECIMALS,
        &maker.pubkey(),
        MAKER_USDC,
    );
    mint_tokens(
        &mut svm,
        &payer,
        &mint_b,
        TSLAX_DECIMALS,
        &taker.pubkey(),
        TAKER_TSLAX,
    );
    // The taker's own 1,000 USDC, in an account the payer opens for them.
    fund_token_account(
        &mut svm,
        &payer,
        &mint_a.pubkey(),
        &taker.pubkey(),
        TAKER_USDC,
    );

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
    make_offer_instruction_for(
        es,
        USDC_OFFERED,
        TSLAX_WANTED,
        &es.mint_b.pubkey(),
        &es.maker_account_b,
    )
}

/// A `make_offer` instruction for the given amounts and wanted token, so a
/// test can build an offer the program should refuse.
fn make_offer_instruction_for(
    es: &EscrowSetup,
    token_a_offered_amount: u64,
    token_b_wanted_amount: u64,
    mint_b: &Pubkey,
    maker_account_b: &Pubkey,
) -> Instruction {
    make_offer_instruction_with_id(
        es,
        OFFER_ID,
        token_a_offered_amount,
        token_b_wanted_amount,
        mint_b,
        maker_account_b,
    )
}

/// The maker's offer `offer_id` and its vault, the offer PDA's associated
/// token account for mint A.
fn offer_and_vault(es: &EscrowSetup, offer_id: u64) -> (Pubkey, Pubkey) {
    let (offer, _bump) = Pubkey::find_program_address(
        &[
            b"offer",
            es.maker.pubkey().as_ref(),
            &offer_id.to_le_bytes(),
        ],
        &es.program_id,
    );
    (
        offer,
        get_associated_token_address(&offer, &es.mint_a.pubkey()),
    )
}

/// A `make_offer` instruction for offer `offer_id`, so a test can open more
/// than one offer from the same maker.
fn make_offer_instruction_with_id(
    es: &EscrowSetup,
    offer_id: u64,
    token_a_offered_amount: u64,
    token_b_wanted_amount: u64,
    mint_b: &Pubkey,
    maker_account_b: &Pubkey,
) -> Instruction {
    let (offer, vault) = offer_and_vault(es, offer_id);
    let mut make_data = vec![MAKE_OFFER];
    make_data.extend_from_slice(&offer_id.to_le_bytes());
    make_data.extend_from_slice(&token_a_offered_amount.to_le_bytes());
    make_data.extend_from_slice(&token_b_wanted_amount.to_le_bytes());

    Instruction {
        program_id: es.program_id,
        accounts: vec![
            AccountMeta::new(offer, false),
            AccountMeta::new_readonly(es.mint_a.pubkey(), false),
            AccountMeta::new_readonly(*mint_b, false),
            AccountMeta::new(es.maker_account_a, false),
            AccountMeta::new(*maker_account_b, false),
            AccountMeta::new(vault, false),
            AccountMeta::new(es.maker.pubkey(), true),
            AccountMeta::new_readonly(spl_token_interface::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account_interface::program::id(), false),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
        ],
        data: make_data,
    }
}

fn take_offer_instruction(es: &EscrowSetup) -> Instruction {
    take_offer_instruction_for(es, USDC_OFFERED, TSLAX_WANTED)
}

/// A `take_offer` instruction signed for the given terms: the least token A
/// the taker accepts and the most token B the taker pays.
fn take_offer_instruction_for(
    es: &EscrowSetup,
    minimum_token_a_out: u64,
    maximum_token_b_in: u64,
) -> Instruction {
    let mut take_data = vec![TAKE_OFFER];
    take_data.extend_from_slice(&minimum_token_a_out.to_le_bytes());
    take_data.extend_from_slice(&maximum_token_b_in.to_le_bytes());

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
        data: take_data,
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
    assert_eq!(token_amount(&es.svm, &es.vault), USDC_OFFERED);
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

    // Taker received the 250 USDC (mint A), ending at 1,250 USDC, and paid
    // the 1 TSLAx (mint B); maker received the TSLAx and is down the USDC.
    assert_eq!(
        token_amount(&es.svm, &es.taker_account_a),
        TAKER_USDC + USDC_OFFERED
    );
    assert_eq!(
        token_amount(&es.svm, &es.taker_account_b),
        TAKER_TSLAX - TSLAX_WANTED
    );
    assert_eq!(token_amount(&es.svm, &es.maker_account_b), TSLAX_WANTED);
    assert_eq!(
        token_amount(&es.svm, &es.maker_account_a),
        MAKER_USDC - USDC_OFFERED
    );

    // Rent destinations: the maker's lamports fully recover (the offer and
    // vault rent both come back to the maker). The taker already had their
    // Mint A (USDC) ATA, so the take costs them no rent at all.
    assert_eq!(
        lamports(&es.svm, &es.maker.pubkey()),
        maker_lamports_before_make
    );
    assert_eq!(
        lamports(&es.svm, &es.taker.pubkey()),
        taker_lamports_before_take
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
    assert_eq!(token_amount(&es.svm, &es.vault), USDC_OFFERED);
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
    assert_fails_with(result, MAKER_MISMATCH);

    // The vault still holds the offered tokens.
    assert_eq!(token_amount(&es.svm, &es.vault), USDC_OFFERED);
}

// `EscrowError` variants in declaration order, as the program reports them in
// `InstructionError::Custom`.
const TOKEN_ACCOUNT_MISMATCH: u32 = 1;
const MAKER_MISMATCH: u32 = 2;
const MINT_MISMATCH: u32 = 3;
const ZERO_AMOUNT: u32 = 7;
const SAME_MINT: u32 = 8;
const OFFER_TERMS_CHANGED: u32 = 9;

/// Check that a refused transaction failed with `expected_code`, so the test
/// knows it failed for the check under test and not for some other reason.
fn assert_fails_with(
    result: Result<(), Box<litesvm::types::FailedTransactionMetadata>>,
    expected_code: u32,
) {
    let error = format!(
        "{:?}",
        result.expect_err("transaction should have failed").err
    );
    assert!(
        error.contains(&format!("Custom({expected_code})")),
        "expected error {expected_code}, got: {error}"
    );
}

/// Send a `make_offer` the program should refuse, and check it failed with
/// `expected_code`, left the maker's tokens where they were, and created no
/// offer account.
fn assert_make_offer_refused(es: &mut EscrowSetup, instruction: Instruction, expected_code: u32) {
    let payer = es.payer.insecure_clone();
    let maker = es.maker.insecure_clone();
    let result = try_send(&mut es.svm, &payer, &[instruction], &[&maker]);
    assert_fails_with(result, expected_code);
    assert_eq!(token_amount(&es.svm, &es.maker_account_a), MAKER_USDC);
    assert_eq!(lamports(&es.svm, &es.offer), 0);
}

#[test]
fn test_make_offer_rejects_zero_offered_amount() {
    let mut es = setup();
    let instruction = make_offer_instruction_for(
        &es,
        0,
        TSLAX_WANTED,
        &es.mint_b.pubkey(),
        &es.maker_account_b,
    );
    assert_make_offer_refused(&mut es, instruction, ZERO_AMOUNT);
}

#[test]
fn test_make_offer_rejects_zero_wanted_amount() {
    let mut es = setup();
    let instruction = make_offer_instruction_for(
        &es,
        USDC_OFFERED,
        0,
        &es.mint_b.pubkey(),
        &es.maker_account_b,
    );
    assert_make_offer_refused(&mut es, instruction, ZERO_AMOUNT);
}

#[test]
fn test_make_offer_rejects_same_mint() {
    let mut es = setup();
    // The maker asks for token A in return for token A, so their token-B
    // account is their token-A account.
    let instruction = make_offer_instruction_for(
        &es,
        USDC_OFFERED,
        TSLAX_WANTED,
        &es.mint_a.pubkey(),
        &es.maker_account_a,
    );
    assert_make_offer_refused(&mut es, instruction, SAME_MINT);
}

/// The bait and switch: the maker makes an offer, the taker signs a
/// `take_offer` for its terms, and before the taker's transaction lands the
/// maker cancels and re-makes the same id at the switched terms. The offer is
/// at the same address, so the taker's transaction reaches the new offer; it
/// must fail with `OfferTermsChanged` and leave the taker's tokens where they
/// were.
fn assert_switched_offer_refused(switched_a_offered: u64, switched_b_wanted: u64) {
    let mut es = setup();
    let payer = es.payer.insecure_clone();
    let maker = es.maker.insecure_clone();
    let taker = es.taker.insecure_clone();

    let make_ix = make_offer_instruction(&es);
    send(&mut es.svm, &payer, &[make_ix], &[&maker]);

    // The taker signs for the terms they saw.
    let take_ix = take_offer_instruction_for(&es, USDC_OFFERED, TSLAX_WANTED);

    // The maker switches the offer before the taker's transaction lands.
    let cancel_ix = cancel_offer_instruction(&es, &es.maker.pubkey());
    send(&mut es.svm, &payer, &[cancel_ix], &[&maker]);
    let switched_make_ix = make_offer_instruction_for(
        &es,
        switched_a_offered,
        switched_b_wanted,
        &es.mint_b.pubkey(),
        &es.maker_account_b,
    );
    send(&mut es.svm, &payer, &[switched_make_ix], &[&maker]);

    let result = try_send(&mut es.svm, &payer, &[take_ix], &[&taker]);
    assert_fails_with(result, OFFER_TERMS_CHANGED);

    // The taker still holds their 1 TSLAx and their 1,000 USDC (so they
    // received no USDC), the maker's TSLAx account is still empty, and the
    // switched offer's USDC is still in the vault.
    assert_eq!(
        token_amount(&es.svm, &es.taker_account_b),
        TAKER_TSLAX,
        "the taker must not pay TSLAx (token B) for a switched offer"
    );
    assert_eq!(
        token_amount(&es.svm, &es.taker_account_a),
        TAKER_USDC,
        "the taker must receive no USDC (token A) from a switched offer"
    );
    assert_eq!(
        token_amount(&es.svm, &es.maker_account_b),
        0,
        "the maker must receive no TSLAx (token B) from a switched offer"
    );
    assert_eq!(
        token_amount(&es.svm, &es.vault),
        switched_a_offered,
        "the switched offer's USDC stays in the vault"
    );
}

#[test]
fn test_take_offer_rejects_switched_offer() {
    // The maker re-makes the offer with 1 USDC in the vault instead of 250,
    // for the same 1 TSLAx.
    assert_switched_offer_refused(ONE_USDC, TSLAX_WANTED);
}

#[test]
fn test_take_offer_rejects_switched_offer_wanting_more_token_b() {
    // The maker re-makes the offer asking for 2 TSLAx instead of 1, for the
    // same 250 USDC.
    assert_switched_offer_refused(USDC_OFFERED, 2 * TSLAX_WANTED);
}

// Positions in `take_offer_instruction`'s account list, the order the
// program unpacks them in, for the accounts an attack test swaps.
const TAKE_TOKEN_MINT_B: usize = 2;
const TAKE_MAKER_TOKEN_ACCOUNT_B: usize = 3;
const TAKE_TAKER_TOKEN_ACCOUNT_B: usize = 5;
const TAKE_VAULT: usize = 6;

/// Every token balance a take could move, in mint A and mint B: the vault,
/// the maker's two accounts, and the taker's two accounts.
fn token_balances(es: &EscrowSetup, vault: &Pubkey) -> [u64; 5] {
    [
        vault,
        &es.maker_account_a,
        &es.maker_account_b,
        &es.taker_account_a,
        &es.taker_account_b,
    ]
    .map(|address| token_amount(&es.svm, address))
}

/// Send a `take_offer` the program should refuse, and check it failed with
/// `expected_code` and moved no tokens in or out of `vault` or either party's
/// accounts.
fn assert_take_offer_refused(
    es: &mut EscrowSetup,
    instruction: Instruction,
    vault: &Pubkey,
    expected_code: u32,
) {
    let payer = es.payer.insecure_clone();
    let taker = es.taker.insecure_clone();
    let balances_before = token_balances(es, vault);
    let result = try_send(&mut es.svm, &payer, &[instruction], &[&taker]);
    assert_fails_with(result, expected_code);
    assert_eq!(
        token_balances(es, vault),
        balances_before,
        "a refused take moves no tokens"
    );
}

#[test]
fn test_take_offer_rejects_taker_account_as_maker_token_account_b() {
    let mut es = setup();
    let payer = es.payer.insecure_clone();
    let maker = es.maker.insecure_clone();
    let make_ix = make_offer_instruction(&es);
    send(&mut es.svm, &payer, &[make_ix], &[&maker]);

    // The taker names their own TSLAx account as the maker's, so the 1 TSLAx
    // they pay would come straight back to them and the 250 USDC would cost
    // nothing. The maker's token B account must be the maker's associated
    // token account for mint B.
    let mut take_ix = take_offer_instruction(&es);
    take_ix.accounts[TAKE_MAKER_TOKEN_ACCOUNT_B].pubkey = es.taker_account_b;
    let vault = es.vault;
    assert_take_offer_refused(&mut es, take_ix, &vault, TOKEN_ACCOUNT_MISMATCH);
}

#[test]
fn test_take_offer_rejects_vault_of_another_offer() {
    let mut es = setup();
    let payer = es.payer.insecure_clone();
    let maker = es.maker.insecure_clone();
    // The maker has two offers open: 250 USDC for 1 TSLAx, and 500 USDC for
    // 2 TSLAx.
    let second_offer_id: u64 = OFFER_ID + 1;
    let make_ix = make_offer_instruction(&es);
    let second_make_ix = make_offer_instruction_with_id(
        &es,
        second_offer_id,
        2 * USDC_OFFERED,
        2 * TSLAX_WANTED,
        &es.mint_b.pubkey(),
        &es.maker_account_b,
    );
    send(&mut es.svm, &payer, &[make_ix, second_make_ix], &[&maker]);
    let (_second_offer, second_vault) = offer_and_vault(&es, second_offer_id);

    // The taker takes the first offer, paying its 1 TSLAx, but names the
    // second offer's vault so they would receive its 500 USDC. The vault must
    // be the first offer's associated token account for mint A.
    let mut take_ix = take_offer_instruction(&es);
    take_ix.accounts[TAKE_VAULT].pubkey = second_vault;
    let first_vault = es.vault;
    assert_take_offer_refused(&mut es, take_ix, &first_vault, TOKEN_ACCOUNT_MISMATCH);
    assert_eq!(
        token_amount(&es.svm, &second_vault),
        2 * USDC_OFFERED,
        "the second offer's USDC stays in its vault"
    );
}

#[test]
fn test_take_offer_rejects_worthless_token_mint_b() {
    let mut es = setup();
    let payer = es.payer.insecure_clone();
    let maker = es.maker.insecure_clone();
    let taker = es.taker.insecure_clone();
    let make_ix = make_offer_instruction(&es);
    send(&mut es.svm, &payer, &[make_ix], &[&maker]);

    // The taker creates their own mint at TSLAx's decimals, mints themselves
    // 1 of its tokens, and opens the maker's account for it, so every
    // associated token account check on mint B holds for the taker's mint.
    let worthless_mint = Keypair::new();
    mint_tokens(
        &mut es.svm,
        &taker,
        &worthless_mint,
        TSLAX_DECIMALS,
        &taker.pubkey(),
        TSLAX_WANTED,
    );
    let create_maker_worthless_account = create_associated_token_account(
        &taker.pubkey(),
        &es.maker.pubkey(),
        &worthless_mint.pubkey(),
        &spl_token_interface::id(),
    );
    send(&mut es.svm, &taker, &[create_maker_worthless_account], &[]);
    let taker_worthless_account =
        get_associated_token_address(&taker.pubkey(), &worthless_mint.pubkey());
    let maker_worthless_account =
        get_associated_token_address(&es.maker.pubkey(), &worthless_mint.pubkey());

    // The mint must be the one the offer stored.
    let mut take_ix = take_offer_instruction(&es);
    take_ix.accounts[TAKE_TOKEN_MINT_B].pubkey = worthless_mint.pubkey();
    take_ix.accounts[TAKE_TAKER_TOKEN_ACCOUNT_B].pubkey = taker_worthless_account;
    take_ix.accounts[TAKE_MAKER_TOKEN_ACCOUNT_B].pubkey = maker_worthless_account;
    let vault = es.vault;
    assert_take_offer_refused(&mut es, take_ix, &vault, MINT_MISMATCH);
    assert_eq!(
        token_amount(&es.svm, &taker_worthless_account),
        TSLAX_WANTED,
        "the taker keeps their worthless tokens"
    );
    assert_eq!(
        token_amount(&es.svm, &maker_worthless_account),
        0,
        "the maker receives none of the taker's worthless tokens"
    );
}
