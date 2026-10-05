use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program, Address, ErrorCode,
        InstructionData, ToAccountMetas,
    },
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
};

fn token_program_id() -> Address {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

fn ata_program_id() -> Address {
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap()
}

// The story the tests tell: Alice offers 1 TSLAx and wants 1,000 USDC for it.
// TSLAx has 8 decimals and USDC has 6, so every amount below is in those
// minor units.
const TSLAX_DECIMALS: u8 = 8;
const USDC_DECIMALS: u8 = 6;
const ONE_TSLAX: u64 = 10u64.pow(TSLAX_DECIMALS as u32);
const ONE_USDC: u64 = 10u64.pow(USDC_DECIMALS as u32);
const TSLAX_OFFERED: u64 = ONE_TSLAX;
const USDC_WANTED: u64 = 1_000 * ONE_USDC;
// What each side holds before the offer: Alice 10 TSLAx, Bob 10,000 USDC.
const ALICE_TSLAX: u64 = 10 * ONE_TSLAX;
const BOB_USDC: u64 = 10_000 * ONE_USDC;

fn lamports(svm: &LiteSVM, address: &Address) -> u64 {
    svm.get_account(address).map(|a| a.lamports).unwrap_or(0)
}

fn derive_ata(wallet: &Address, mint: &Address) -> Address {
    let (ata, _bump) = Address::find_program_address(
        &[wallet.as_ref(), token_program_id().as_ref(), mint.as_ref()],
        &ata_program_id(),
    );
    ata
}

fn setup() -> (LiteSVM, Address, Keypair) {
    let program_id = escrow::id();
    let mut svm = anchor_v2_testing::svm();

    let program_bytes = include_bytes!("../../../target/deploy/escrow.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
    (svm, program_id, payer)
}

struct EscrowSetup {
    svm: LiteSVM,
    program_id: Address,
    payer: Keypair,
    alice: Keypair,
    bob: Keypair,
    mint_a: Address,
    mint_b: Address,
    alice_ata_a: Address,
    alice_ata_b: Address,
    bob_ata_a: Address,
    bob_ata_b: Address,
}

fn full_setup() -> EscrowSetup {
    let (mut svm, program_id, payer) = setup();

    let alice = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let bob = create_wallet(&mut svm, 10_000_000_000).unwrap();

    // Token A is TSLAx and token B is USDC; the payer is the mint authority
    // of both.
    let mint_a = create_token_mint(&mut svm, &payer, TSLAX_DECIMALS, None).unwrap();
    let mint_b = create_token_mint(&mut svm, &payer, USDC_DECIMALS, None).unwrap();

    // Create ATAs
    let alice_ata_a =
        create_associated_token_account(&mut svm, &alice.pubkey(), &mint_a, &payer).unwrap();
    let alice_ata_b =
        create_associated_token_account(&mut svm, &alice.pubkey(), &mint_b, &payer).unwrap();
    let bob_ata_b =
        create_associated_token_account(&mut svm, &bob.pubkey(), &mint_b, &payer).unwrap();

    // bob_ata_a is derived but not pre-created (program uses init_if_needed)
    let bob_ata_a = derive_ata(&bob.pubkey(), &mint_a);

    // Alice holds the TSLAx, Bob holds the USDC.
    mint_tokens_to_token_account(&mut svm, &mint_a, &alice_ata_a, ALICE_TSLAX, &payer).unwrap();
    mint_tokens_to_token_account(&mut svm, &mint_b, &bob_ata_b, BOB_USDC, &payer).unwrap();

    EscrowSetup {
        svm,
        program_id,
        payer,
        alice,
        bob,
        mint_a,
        mint_b,
        alice_ata_a,
        alice_ata_b,
        bob_ata_a,
        bob_ata_b,
    }
}

#[test]
fn test_make_offer() {
    let mut es = full_setup();

    let offer_id: u64 = 1;
    let token_a_offered_amount: u64 = TSLAX_OFFERED;
    let token_b_wanted_amount: u64 = USDC_WANTED;

    // Derive offer PDA
    let (offer_pda, _bump) = Address::find_program_address(
        &[
            b"offer",
            es.alice.pubkey().as_ref(),
            &offer_id.to_le_bytes(),
        ],
        &es.program_id,
    );

    // Vault is the ATA of the offer PDA for mint_a
    let vault = derive_ata(&offer_pda, &es.mint_a);

    let make_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::MakeOffer {
            id: offer_id,
            token_a_offered_amount,
            token_b_wanted_amount,
        }
        .data(),
        escrow::accounts::MakeOfferAccountConstraints {
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            token_mint_b: es.mint_b,
            maker_token_account_a: es.alice_ata_a,
            maker_token_account_b: es.alice_ata_b,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut es.svm,
        vec![make_offer_ix],
        &[&es.payer, &es.alice],
        &es.payer.pubkey(),
    )
    .unwrap();

    // Verify vault contains the offered tokens
    assert_eq!(
        get_token_account_balance(&es.svm, &vault).unwrap(),
        token_a_offered_amount
    );

    // Verify offer account data
    let offer_data = es.svm.get_account(&offer_pda).expect("Offer should exist");
    let data = &offer_data.data[8..]; // Skip 8-byte discriminator
    let stored_id = u64::from_le_bytes(data[0..8].try_into().unwrap());
    assert_eq!(stored_id, offer_id);
    let stored_maker = Address::try_from(&data[8..40]).unwrap();
    assert_eq!(stored_maker, es.alice.pubkey());
}

#[test]
fn test_take_offer() {
    let mut es = full_setup();

    let offer_id: u64 = 2;
    let token_a_offered_amount: u64 = TSLAX_OFFERED;
    let token_b_wanted_amount: u64 = USDC_WANTED;

    // Derive offer PDA
    let (offer_pda, _bump) = Address::find_program_address(
        &[
            b"offer",
            es.alice.pubkey().as_ref(),
            &offer_id.to_le_bytes(),
        ],
        &es.program_id,
    );

    let vault = derive_ata(&offer_pda, &es.mint_a);

    // Alice pays the offer + vault rent in make_offer and must recover it all
    // when the offer is taken. (Alice's token-B ATA already exists, and the
    // payer covers transaction fees, so her lamports should round-trip
    // exactly.)
    let alice_lamports_before_make = lamports(&es.svm, &es.alice.pubkey());
    let bob_lamports_before_take = lamports(&es.svm, &es.bob.pubkey());

    // Step 1: Alice makes the offer
    let make_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::MakeOffer {
            id: offer_id,
            token_a_offered_amount,
            token_b_wanted_amount,
        }
        .data(),
        escrow::accounts::MakeOfferAccountConstraints {
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            token_mint_b: es.mint_b,
            maker_token_account_a: es.alice_ata_a,
            maker_token_account_b: es.alice_ata_b,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut es.svm,
        vec![make_offer_ix],
        &[&es.payer, &es.alice],
        &es.payer.pubkey(),
    )
    .unwrap();

    // Verify vault has tokens
    assert_eq!(
        get_token_account_balance(&es.svm, &vault).unwrap(),
        token_a_offered_amount
    );

    // Step 2: Bob takes the offer
    let take_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::TakeOffer {
            minimum_token_a_out: token_a_offered_amount,
            maximum_token_b_in: token_b_wanted_amount,
        }
        .data(),
        escrow::accounts::TakeOfferAccountConstraints {
            taker: es.bob.pubkey(),
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            token_mint_b: es.mint_b,
            taker_token_account_a: es.bob_ata_a,
            taker_token_account_b: es.bob_ata_b,
            maker_token_account_b: es.alice_ata_b,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut es.svm,
        vec![take_offer_ix],
        &[&es.payer, &es.bob],
        &es.payer.pubkey(),
    )
    .unwrap();

    // Verify Bob received token A from vault
    assert_eq!(
        get_token_account_balance(&es.svm, &es.bob_ata_a).unwrap(),
        token_a_offered_amount
    );

    // Verify Alice received token B from Bob
    assert_eq!(
        get_token_account_balance(&es.svm, &es.alice_ata_b).unwrap(),
        token_b_wanted_amount
    );

    // Verify vault is closed
    assert!(
        es.svm.get_account(&vault).is_none(),
        "Vault should be closed after take_offer"
    );

    // Verify offer account is closed
    assert!(
        es.svm.get_account(&offer_pda).is_none(),
        "Offer should be closed after take_offer"
    );

    // Rent destinations: Alice (the maker) recovers the offer + vault rent in
    // full. Bob (the taker) only paid the rent of his own new token-A ATA.
    assert_eq!(
        lamports(&es.svm, &es.alice.pubkey()),
        alice_lamports_before_make,
        "maker must recover the offer and vault rent after take_offer"
    );
    let bob_ata_a_rent = lamports(&es.svm, &es.bob_ata_a);
    assert_eq!(
        lamports(&es.svm, &es.bob.pubkey()),
        bob_lamports_before_take - bob_ata_a_rent,
        "taker must only pay the rent of their own token-A ATA"
    );
}

#[test]
fn test_cancel_offer() {
    let mut es = full_setup();

    let offer_id: u64 = 3;
    let token_a_offered_amount: u64 = TSLAX_OFFERED;
    let token_b_wanted_amount: u64 = USDC_WANTED;

    let (offer_pda, _bump) = Address::find_program_address(
        &[
            b"offer",
            es.alice.pubkey().as_ref(),
            &offer_id.to_le_bytes(),
        ],
        &es.program_id,
    );
    let vault = derive_ata(&offer_pda, &es.mint_a);

    // Snapshot Alice's token-A balance and lamports before the offer.
    let alice_a_before = get_token_account_balance(&es.svm, &es.alice_ata_a).unwrap();
    let alice_lamports_before_make = lamports(&es.svm, &es.alice.pubkey());

    // Alice makes the offer.
    let make_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::MakeOffer {
            id: offer_id,
            token_a_offered_amount,
            token_b_wanted_amount,
        }
        .data(),
        escrow::accounts::MakeOfferAccountConstraints {
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            token_mint_b: es.mint_b,
            maker_token_account_a: es.alice_ata_a,
            maker_token_account_b: es.alice_ata_b,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut es.svm,
        vec![make_offer_ix],
        &[&es.payer, &es.alice],
        &es.payer.pubkey(),
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&es.svm, &vault).unwrap(),
        token_a_offered_amount
    );

    // Alice cancels the offer.
    let cancel_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::CancelOffer {}.data(),
        escrow::accounts::CancelOfferAccountConstraints {
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            maker_token_account_a: es.alice_ata_a,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut es.svm,
        vec![cancel_offer_ix],
        &[&es.payer, &es.alice],
        &es.payer.pubkey(),
    )
    .unwrap();

    // The offer and vault accounts should be closed.
    assert!(
        es.svm.get_account(&offer_pda).is_none(),
        "Offer should be closed after cancel"
    );
    assert!(
        es.svm.get_account(&vault).is_none(),
        "Vault should be closed after cancel"
    );

    // Alice should have her token-A back to its pre-make balance.
    let alice_a_after = get_token_account_balance(&es.svm, &es.alice_ata_a).unwrap();
    assert_eq!(alice_a_after, alice_a_before);

    // Rent destination: Alice recovers the offer + vault rent in full.
    assert_eq!(
        lamports(&es.svm, &es.alice.pubkey()),
        alice_lamports_before_make,
        "maker must recover the offer and vault rent after cancel_offer"
    );
}

#[test]
fn test_cancel_offer_rejects_non_maker() {
    let mut es = full_setup();

    let offer_id: u64 = 4;
    let token_a_offered_amount: u64 = TSLAX_OFFERED;
    let token_b_wanted_amount: u64 = USDC_WANTED;

    let (offer_pda, _bump) = Address::find_program_address(
        &[
            b"offer",
            es.alice.pubkey().as_ref(),
            &offer_id.to_le_bytes(),
        ],
        &es.program_id,
    );
    let vault = derive_ata(&offer_pda, &es.mint_a);

    // Alice makes the offer.
    let make_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::MakeOffer {
            id: offer_id,
            token_a_offered_amount,
            token_b_wanted_amount,
        }
        .data(),
        escrow::accounts::MakeOfferAccountConstraints {
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            token_mint_b: es.mint_b,
            maker_token_account_a: es.alice_ata_a,
            maker_token_account_b: es.alice_ata_b,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut es.svm,
        vec![make_offer_ix],
        &[&es.payer, &es.alice],
        &es.payer.pubkey(),
    )
    .unwrap();

    // Bob tries to cancel Alice's offer - the address = offer.maker / signer + seeds
    // constraints should reject this.
    let bob_ata_a =
        create_associated_token_account(&mut es.svm, &es.bob.pubkey(), &es.mint_a, &es.payer)
            .unwrap();
    let cancel_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::CancelOffer {}.data(),
        escrow::accounts::CancelOfferAccountConstraints {
            maker: es.bob.pubkey(),
            token_mint_a: es.mint_a,
            maker_token_account_a: bob_ata_a,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let result = send_transaction_from_instructions(
        &mut es.svm,
        vec![cancel_offer_ix],
        &[&es.payer, &es.bob],
        &es.payer.pubkey(),
    );
    assert_fails_with_anchor_error(result, ErrorCode::ConstraintAddress);
}

// Anchor numbers a program's errors from 6000 in declaration order, and a
// failed transaction reports the number as `Custom(n)`. Matching it shows the
// transaction failed for the check under test, not for some unrelated reason.
fn assert_fails_with(
    result: Result<(), solana_kite::SolanaKiteError>,
    expected: escrow::error::EscrowError,
) {
    assert_fails_with_program_error(result, &anchor_lang::Error::Custom(6000 + expected as u32));
}

// The same check for one of Anchor's own constraint errors. Anchor 2 reports
// a constraint failure as the `ProgramError` its `ErrorCode` converts to,
// which for most constraints is `Custom(n)` with Anchor 1's number for it.
fn assert_fails_with_anchor_error(
    result: Result<(), solana_kite::SolanaKiteError>,
    expected: ErrorCode,
) {
    assert_fails_with_program_error(result, &anchor_lang::Error::from(expected));
}

fn assert_fails_with_program_error(
    result: Result<(), solana_kite::SolanaKiteError>,
    expected: &anchor_lang::Error,
) {
    let error = format!("{:?}", result.expect_err("transaction should have failed"));
    assert!(
        error.contains(&format!("{expected:?}")),
        "expected error {expected:?}, got: {error}"
    );
}

// Alice sends `make_offer` for the given amounts and wanted token, and the
// result is returned rather than unwrapped so a test can check the refusal.
fn try_make_offer(
    es: &mut EscrowSetup,
    offer_id: u64,
    token_a_offered_amount: u64,
    token_b_wanted_amount: u64,
    token_mint_b: Address,
    maker_token_account_b: Address,
) -> Result<(), solana_kite::SolanaKiteError> {
    let (offer_pda, _bump) = Address::find_program_address(
        &[
            b"offer",
            es.alice.pubkey().as_ref(),
            &offer_id.to_le_bytes(),
        ],
        &es.program_id,
    );
    let vault = derive_ata(&offer_pda, &es.mint_a);
    let make_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::MakeOffer {
            id: offer_id,
            token_a_offered_amount,
            token_b_wanted_amount,
        }
        .data(),
        escrow::accounts::MakeOfferAccountConstraints {
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            token_mint_b,
            maker_token_account_a: es.alice_ata_a,
            maker_token_account_b,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut es.svm,
        vec![make_offer_ix],
        &[&es.payer, &es.alice],
        &es.payer.pubkey(),
    )
}

#[test]
fn test_make_offer_rejects_zero_offered_amount() {
    let mut es = full_setup();
    let (mint_b, alice_ata_b) = (es.mint_b, es.alice_ata_b);
    let alice_balance_before = get_token_account_balance(&es.svm, &es.alice_ata_a).unwrap();

    let result = try_make_offer(&mut es, 5, 0, USDC_WANTED, mint_b, alice_ata_b);

    assert_fails_with(result, escrow::error::EscrowError::ZeroAmount);
    assert_eq!(
        get_token_account_balance(&es.svm, &es.alice_ata_a).unwrap(),
        alice_balance_before
    );
}

#[test]
fn test_make_offer_rejects_zero_wanted_amount() {
    let mut es = full_setup();
    let (mint_b, alice_ata_b) = (es.mint_b, es.alice_ata_b);
    let alice_balance_before = get_token_account_balance(&es.svm, &es.alice_ata_a).unwrap();

    let result = try_make_offer(&mut es, 6, TSLAX_OFFERED, 0, mint_b, alice_ata_b);

    assert_fails_with(result, escrow::error::EscrowError::ZeroAmount);
    assert_eq!(
        get_token_account_balance(&es.svm, &es.alice_ata_a).unwrap(),
        alice_balance_before
    );
}

#[test]
fn test_make_offer_rejects_same_mint() {
    let mut es = full_setup();
    // Alice asks for token A in return for token A, so her token-B account is
    // her token-A account.
    let (mint_a, alice_ata_a) = (es.mint_a, es.alice_ata_a);
    let alice_balance_before = get_token_account_balance(&es.svm, &alice_ata_a).unwrap();

    let result = try_make_offer(
        &mut es,
        7,
        TSLAX_OFFERED,
        2 * TSLAX_OFFERED,
        mint_a,
        alice_ata_a,
    );

    // Anchor refuses the same mutable account twice before the handler runs.
    assert_fails_with_anchor_error(result, ErrorCode::ConstraintDuplicateMutableAccount);
    assert_eq!(
        get_token_account_balance(&es.svm, &alice_ata_a).unwrap(),
        alice_balance_before
    );
}

// Bob's `take_offer` for Alice's offer `offer_id`, signed for the given terms.
fn take_offer_instruction(
    es: &EscrowSetup,
    offer_id: u64,
    minimum_token_a_out: u64,
    maximum_token_b_in: u64,
) -> Instruction {
    let (offer_pda, _bump) = Address::find_program_address(
        &[
            b"offer",
            es.alice.pubkey().as_ref(),
            &offer_id.to_le_bytes(),
        ],
        &es.program_id,
    );
    let vault = derive_ata(&offer_pda, &es.mint_a);
    Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::TakeOffer {
            minimum_token_a_out,
            maximum_token_b_in,
        }
        .data(),
        escrow::accounts::TakeOfferAccountConstraints {
            taker: es.bob.pubkey(),
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            token_mint_b: es.mint_b,
            taker_token_account_a: es.bob_ata_a,
            taker_token_account_b: es.bob_ata_b,
            maker_token_account_b: es.alice_ata_b,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

// Alice's `cancel_offer` for her offer `offer_id`, sent and unwrapped.
fn cancel_offer(es: &mut EscrowSetup, offer_id: u64) {
    let (offer_pda, _bump) = Address::find_program_address(
        &[
            b"offer",
            es.alice.pubkey().as_ref(),
            &offer_id.to_le_bytes(),
        ],
        &es.program_id,
    );
    let vault = derive_ata(&offer_pda, &es.mint_a);
    let cancel_offer_ix = Instruction::new_with_bytes(
        es.program_id,
        &escrow::instruction::CancelOffer {}.data(),
        escrow::accounts::CancelOfferAccountConstraints {
            maker: es.alice.pubkey(),
            token_mint_a: es.mint_a,
            maker_token_account_a: es.alice_ata_a,
            offer: offer_pda,
            vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut es.svm,
        vec![cancel_offer_ix],
        &[&es.payer, &es.alice],
        &es.payer.pubkey(),
    )
    .unwrap();
}

// The bait and switch: Alice makes an offer, Bob signs a `take_offer` for its
// terms, and before Bob's transaction lands Alice cancels and re-makes the
// same id at the switched terms. The offer is at the same address, so Bob's
// transaction reaches the new offer; it must fail with `OfferTermsChanged`
// and leave Bob's tokens where they were.
fn assert_switched_offer_refused(switched_a_offered: u64, switched_b_wanted: u64) {
    let mut es = full_setup();
    let (mint_b, alice_ata_b) = (es.mint_b, es.alice_ata_b);
    let offer_id: u64 = 8;
    let token_a_offered_amount: u64 = TSLAX_OFFERED;
    let token_b_wanted_amount: u64 = USDC_WANTED;

    try_make_offer(
        &mut es,
        offer_id,
        token_a_offered_amount,
        token_b_wanted_amount,
        mint_b,
        alice_ata_b,
    )
    .unwrap();

    // Bob signs for the terms he saw.
    let take_offer_ix =
        take_offer_instruction(&es, offer_id, token_a_offered_amount, token_b_wanted_amount);

    // Alice switches the offer before Bob's transaction lands.
    cancel_offer(&mut es, offer_id);
    try_make_offer(
        &mut es,
        offer_id,
        switched_a_offered,
        switched_b_wanted,
        mint_b,
        alice_ata_b,
    )
    .unwrap();

    let bob_b_before = get_token_account_balance(&es.svm, &es.bob_ata_b).unwrap();
    let alice_b_before = get_token_account_balance(&es.svm, &es.alice_ata_b).unwrap();

    let result = send_transaction_from_instructions(
        &mut es.svm,
        vec![take_offer_ix],
        &[&es.payer, &es.bob],
        &es.payer.pubkey(),
    );

    assert_fails_with(result, escrow::error::EscrowError::OfferTermsChanged);
    assert_eq!(
        get_token_account_balance(&es.svm, &es.bob_ata_b).unwrap(),
        bob_b_before,
        "the taker must not pay token B for a switched offer"
    );
    assert!(
        es.svm.get_account(&es.bob_ata_a).is_none(),
        "the taker must receive no token A from a switched offer"
    );
    assert_eq!(
        get_token_account_balance(&es.svm, &es.alice_ata_b).unwrap(),
        alice_b_before
    );
}

#[test]
fn test_take_offer_rejects_switched_offer() {
    // Alice re-makes the offer putting a thousandth of the TSLAx in the vault.
    assert_switched_offer_refused(TSLAX_OFFERED / 1_000, USDC_WANTED);
}

#[test]
fn test_take_offer_rejects_switched_offer_wanting_more_token_b() {
    // Alice re-makes the offer asking for twice the USDC.
    assert_switched_offer_refused(TSLAX_OFFERED, 2 * USDC_WANTED);
}
