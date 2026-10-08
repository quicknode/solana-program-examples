//! quasar-test integration tests: make an offer (deposit into the vault),
//! take it (swap the tokens, close the offer and vault back to the maker),
//! cancel it, and reject substituted accounts, non-maker signers, and a take
//! that lands on an offer the maker switched to worse terms.

use {
    crate::{
        cpi::{CancelOfferInstruction, MakeOfferInstruction, TakeOfferInstruction},
        error::EscrowError,
        state::{Offer, OfferData},
    },
    quasar_lang::error::QuasarError,
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const MAKER: Pubkey = Pubkey::new_from_array([1; 32]);
const TAKER: Pubkey = Pubkey::new_from_array([2; 32]);
const USDC_MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const TSLAX_MINT: Pubkey = Pubkey::new_from_array([4; 32]);
const MAKER_TOKEN_ACCOUNT_A: Pubkey = Pubkey::new_from_array([5; 32]);
const MAKER_TOKEN_ACCOUNT_B: Pubkey = Pubkey::new_from_array([6; 32]);
const VAULT: Pubkey = Pubkey::new_from_array([7; 32]);
const TAKER_TOKEN_ACCOUNT_A: Pubkey = Pubkey::new_from_array([8; 32]);
const TAKER_TOKEN_ACCOUNT_B: Pubkey = Pubkey::new_from_array([9; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([10; 32]);
const ATTACKER_TOKEN_ACCOUNT_A: Pubkey = Pubkey::new_from_array([11; 32]);
const WRONG_MINT: Pubkey = Pubkey::new_from_array([12; 32]);
const WRONG_VAULT: Pubkey = Pubkey::new_from_array([13; 32]);

const OFFER_ID: u64 = 7;

// The story the tests tell: the maker (Alice) offers 250 USDC (token A) and
// wants 1 TSLAx (token B) for it, and the taker (Bob) takes the offer. USDC
// has 6 decimals and TSLAx has 8, so every amount below is in those minor
// units.
const USDC_DECIMALS: u8 = 6;
const TSLAX_DECIMALS: u8 = 8;
const ONE_USDC: u64 = 10u64.pow(USDC_DECIMALS as u32);
const ONE_TSLAX: u64 = 10u64.pow(TSLAX_DECIMALS as u32);
const USDC_OFFERED: u64 = 250 * ONE_USDC;
const TSLAX_WANTED: u64 = ONE_TSLAX;
// What each side holds before the offer: both start with the standard
// wallet of 1,000 USDC, and the taker also holds 1 TSLAx. Each mint's supply
// is what its holders were given.
const MAKER_USDC: u64 = 1_000 * ONE_USDC;
const TAKER_USDC: u64 = 1_000 * ONE_USDC;
const TAKER_TSLAX: u64 = ONE_TSLAX;

/// Register the maker and both mints: USDC at 6 decimals and TSLAx at 8.
fn base_world(test: &mut Test) {
    test.add(Wallet::new().at(MAKER));
    test.add(
        Mint::new(MAKER)
            .at(USDC_MINT)
            .supply(MAKER_USDC + TAKER_USDC)
            .decimals(USDC_DECIMALS),
    );
    test.add(
        Mint::new(MAKER)
            .at(TSLAX_MINT)
            .supply(TAKER_TSLAX)
            .decimals(TSLAX_DECIMALS),
    );
}

/// Register the taker's starting tokens: 1,000 USDC and 1 TSLAx.
fn taker_holding_usdc_and_tslax(test: &mut Test) {
    test.add(
        TokenAccount::new(USDC_MINT, TAKER)
            .at(TAKER_TOKEN_ACCOUNT_A)
            .amount(TAKER_USDC),
    );
    test.add(
        TokenAccount::new(TSLAX_MINT, TAKER)
            .at(TAKER_TOKEN_ACCOUNT_B)
            .amount(TAKER_TSLAX),
    );
}

/// Register a live offer holding `USDC_OFFERED` in the vault, exactly as
/// `make_offer` leaves it.
fn live_offer(test: &mut Test) -> Pubkey {
    let (offer, bump) = test.derive_pda_with_bump(Offer::seeds(&MAKER, OFFER_ID));
    test.write(
        offer,
        OfferData {
            id: OFFER_ID.into(),
            maker: MAKER,
            token_mint_a: USDC_MINT,
            token_mint_b: TSLAX_MINT,
            maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
            vault: VAULT,
            receive: TSLAX_WANTED.into(),
            bump,
        },
    );
    test.add(
        TokenAccount::new(USDC_MINT, offer)
            .at(VAULT)
            .amount(USDC_OFFERED),
    );
    offer
}

#[quasar_test]
fn make_offer_records_the_offer_and_funds_the_vault(test: &mut Test) {
    base_world(test);
    test.add(
        TokenAccount::new(USDC_MINT, MAKER)
            .at(MAKER_TOKEN_ACCOUNT_A)
            .amount(MAKER_USDC),
    );
    let (offer, bump) = test.derive_pda_with_bump(Offer::seeds(&MAKER, OFFER_ID));

    test.send(MakeOfferInstruction {
        maker: MAKER,
        token_mint_a: USDC_MINT,
        token_mint_b: TSLAX_MINT,
        maker_token_account_a: MAKER_TOKEN_ACCOUNT_A,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: VAULT,
        id: OFFER_ID,
        deposit: USDC_OFFERED,
        receive: TSLAX_WANTED,
    })
    .succeeds()
    // The deposit landed in the vault.
    .has_tokens(VAULT, USDC_OFFERED);

    // Verify the recorded offer state.
    let state = test.read::<Offer>(offer);
    assert_eq!(u64::from(state.id), OFFER_ID, "id");
    assert_eq!(state.maker, MAKER, "maker");
    assert_eq!(state.token_mint_a, USDC_MINT, "token_mint_a");
    assert_eq!(state.token_mint_b, TSLAX_MINT, "token_mint_b");
    assert_eq!(
        state.maker_token_account_b, MAKER_TOKEN_ACCOUNT_B,
        "maker_token_account_b"
    );
    assert_eq!(state.vault, VAULT, "vault");
    assert_eq!(u64::from(state.receive), TSLAX_WANTED, "receive");
    assert_eq!(state.bump, bump, "bump");
}

#[quasar_test]
fn take_offer_swaps_tokens_and_returns_rent_to_the_maker(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(TAKER));
    let offer = live_offer(test);
    taker_holding_usdc_and_tslax(test);
    test.add(TokenAccount::new(TSLAX_MINT, MAKER).at(MAKER_TOKEN_ACCOUNT_B));

    // Rent destinations are asserted exactly: the maker paid the offer and
    // vault rent in make_offer and must recover both on close.
    let offer_rent = test.lamports(offer);
    let vault_rent = test.lamports(VAULT);
    let maker_lamports_before = test.lamports(MAKER);
    let taker_lamports_before = test.lamports(TAKER);

    test.send(TakeOfferInstruction {
        taker: TAKER,
        offer_id_seed: OFFER_ID,
        maker: MAKER,
        token_mint_a: USDC_MINT,
        token_mint_b: TSLAX_MINT,
        taker_token_account_a: TAKER_TOKEN_ACCOUNT_A,
        taker_token_account_b: TAKER_TOKEN_ACCOUNT_B,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: VAULT,
        minimum_token_a_out: USDC_OFFERED,
        maximum_token_b_in: TSLAX_WANTED,
    })
    .succeeds()
    // Token balances: the taker received the vault's 250 USDC (mint A),
    // ending at 1,250 USDC, and paid the 1 TSLAx (mint B); the maker received
    // the TSLAx.
    .has_tokens(TAKER_TOKEN_ACCOUNT_A, TAKER_USDC + USDC_OFFERED)
    .has_tokens(TAKER_TOKEN_ACCOUNT_B, TAKER_TSLAX - TSLAX_WANTED)
    .has_tokens(MAKER_TOKEN_ACCOUNT_B, TSLAX_WANTED)
    // The offer and vault are closed.
    .is_closed(offer)
    .is_closed(VAULT);

    assert_eq!(
        test.lamports(MAKER),
        maker_lamports_before + offer_rent + vault_rent,
        "maker must recover the offer and vault rent"
    );
    assert!(
        test.lamports(TAKER) <= taker_lamports_before,
        "taker must not gain lamports from closing the maker's accounts"
    );
}

#[quasar_test]
fn take_offer_rejects_a_mint_that_does_not_match_the_offer(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(TAKER));
    live_offer(test);
    taker_holding_usdc_and_tslax(test);
    test.add(TokenAccount::new(TSLAX_MINT, MAKER).at(MAKER_TOKEN_ACCOUNT_B));

    // The attacker substitutes a different mint, with USDC's decimals, for
    // token_mint_a. The has_one(token_mint_a) binding to the offer state
    // refuses it.
    test.add(
        Mint::new(MAKER)
            .at(WRONG_MINT)
            .supply(MAKER_USDC)
            .decimals(USDC_DECIMALS),
    );

    test.send(TakeOfferInstruction {
        taker: TAKER,
        offer_id_seed: OFFER_ID,
        maker: MAKER,
        token_mint_a: WRONG_MINT,
        token_mint_b: TSLAX_MINT,
        taker_token_account_a: TAKER_TOKEN_ACCOUNT_A,
        taker_token_account_b: TAKER_TOKEN_ACCOUNT_B,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: VAULT,
        minimum_token_a_out: USDC_OFFERED,
        maximum_token_b_in: TSLAX_WANTED,
    })
    .fails_with(QuasarError::HasOneMismatch);
}

#[quasar_test]
fn take_offer_rejects_a_vault_that_does_not_match_the_offer(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(TAKER));
    let offer = live_offer(test);
    taker_holding_usdc_and_tslax(test);
    test.add(TokenAccount::new(TSLAX_MINT, MAKER).at(MAKER_TOKEN_ACCOUNT_B));

    // The attacker substitutes a different token account (same mint, also
    // owned by the offer PDA) for the vault. The has_one(vault) binding to
    // the offer state refuses it.
    test.add(
        TokenAccount::new(USDC_MINT, offer)
            .at(WRONG_VAULT)
            .amount(USDC_OFFERED),
    );

    test.send(TakeOfferInstruction {
        taker: TAKER,
        offer_id_seed: OFFER_ID,
        maker: MAKER,
        token_mint_a: USDC_MINT,
        token_mint_b: TSLAX_MINT,
        taker_token_account_a: TAKER_TOKEN_ACCOUNT_A,
        taker_token_account_b: TAKER_TOKEN_ACCOUNT_B,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: WRONG_VAULT,
        minimum_token_a_out: USDC_OFFERED,
        maximum_token_b_in: TSLAX_WANTED,
    })
    .fails_with(QuasarError::HasOneMismatch);
}

#[quasar_test]
fn cancel_offer_returns_deposit_and_rent_to_the_maker(test: &mut Test) {
    base_world(test);
    let offer = live_offer(test);
    // Pre-created with a zero balance so the maker's tokens can be compared
    // exactly after the cancel.
    test.add(TokenAccount::new(USDC_MINT, MAKER).at(MAKER_TOKEN_ACCOUNT_A));

    let offer_rent = test.lamports(offer);
    let vault_rent = test.lamports(VAULT);
    let maker_lamports_before = test.lamports(MAKER);

    test.send(CancelOfferInstruction {
        maker: MAKER,
        offer_id_seed: OFFER_ID,
        token_mint_a: USDC_MINT,
        maker_token_account_a: MAKER_TOKEN_ACCOUNT_A,
        vault: VAULT,
    })
    .succeeds()
    // The maker got their mint A tokens back.
    .has_tokens(MAKER_TOKEN_ACCOUNT_A, USDC_OFFERED)
    // The offer and vault are closed.
    .is_closed(offer)
    .is_closed(VAULT);

    assert_eq!(
        test.lamports(MAKER),
        maker_lamports_before + offer_rent + vault_rent,
        "maker must recover the offer and vault rent"
    );
}

#[quasar_test]
fn cancel_offer_rejects_a_signer_who_is_not_the_maker(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(ATTACKER));
    let offer = live_offer(test);
    test.add(TokenAccount::new(USDC_MINT, ATTACKER).at(ATTACKER_TOKEN_ACCOUNT_A));

    // The attacker signs as the "maker" but passes the real maker's offer.
    // The offer's PDA seeds no longer derive its address with the attacker
    // as maker, so the PDA check refuses it before has_one(maker) runs. The
    // builder would derive the attacker's own (nonexistent) offer PDA, so the
    // real offer address is substituted at the instruction level.
    let mut ix: Instruction = CancelOfferInstruction {
        maker: ATTACKER,
        offer_id_seed: OFFER_ID,
        token_mint_a: USDC_MINT,
        maker_token_account_a: ATTACKER_TOKEN_ACCOUNT_A,
        vault: VAULT,
    }
    .into();
    // Account order = the accounts-struct field order: maker, offer, ...
    ix.accounts[1].pubkey = offer;

    test.send(ix).fails_with(QuasarError::InvalidPda);
}

/// Send `make_offer` for the given amounts and wanted token, from a maker who
/// holds token A.
fn make_offer(test: &mut Test, deposit: u64, receive: u64, token_mint_b: Pubkey) -> Outcome {
    test.send(MakeOfferInstruction {
        maker: MAKER,
        token_mint_a: USDC_MINT,
        token_mint_b,
        maker_token_account_a: MAKER_TOKEN_ACCOUNT_A,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: VAULT,
        id: OFFER_ID,
        deposit,
        receive,
    })
}

fn maker_holding_token_a(test: &mut Test) {
    base_world(test);
    test.add(
        TokenAccount::new(USDC_MINT, MAKER)
            .at(MAKER_TOKEN_ACCOUNT_A)
            .amount(MAKER_USDC),
    );
}

#[quasar_test]
fn make_offer_rejects_a_zero_deposit(test: &mut Test) {
    maker_holding_token_a(test);
    make_offer(test, 0, TSLAX_WANTED, TSLAX_MINT).fails_with(EscrowError::ZeroAmount);
}

#[quasar_test]
fn make_offer_rejects_a_zero_receive_amount(test: &mut Test) {
    maker_holding_token_a(test);
    make_offer(test, USDC_OFFERED, 0, TSLAX_MINT).fails_with(EscrowError::ZeroAmount);
}

#[quasar_test]
fn make_offer_rejects_an_offer_of_a_token_for_itself(test: &mut Test) {
    maker_holding_token_a(test);
    // Both mint slots hold the same account, and loading it twice fails
    // before the handler runs.
    make_offer(test, USDC_OFFERED, TSLAX_WANTED, USDC_MINT)
        .fails(ProgramError::Runtime("AccountBorrowFailed".into()));
}

/// The taker's `take_offer`, signed for the given terms.
fn take_offer_instruction(minimum_token_a_out: u64, maximum_token_b_in: u64) -> Instruction {
    TakeOfferInstruction {
        taker: TAKER,
        offer_id_seed: OFFER_ID,
        maker: MAKER,
        token_mint_a: USDC_MINT,
        token_mint_b: TSLAX_MINT,
        taker_token_account_a: TAKER_TOKEN_ACCOUNT_A,
        taker_token_account_b: TAKER_TOKEN_ACCOUNT_B,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: VAULT,
        minimum_token_a_out,
        maximum_token_b_in,
    }
    .into()
}

/// The bait and switch: the maker makes an offer, the taker signs a
/// `take_offer` for its terms, and before the taker's transaction lands the
/// maker cancels and re-makes the same id at the switched terms. The offer is
/// at the same address, so the taker's transaction reaches the new offer; it
/// must fail with `OfferTermsChanged` and leave the taker's tokens where they
/// were.
fn assert_switched_offer_refused(test: &mut Test, switched_deposit: u64, switched_receive: u64) {
    maker_holding_token_a(test);
    test.add(Wallet::new().at(TAKER));
    taker_holding_usdc_and_tslax(test);

    make_offer(test, USDC_OFFERED, TSLAX_WANTED, TSLAX_MINT).succeeds();

    // The taker signs for the terms they saw.
    let take = take_offer_instruction(USDC_OFFERED, TSLAX_WANTED);

    // The maker switches the offer before the taker's transaction lands.
    test.send(CancelOfferInstruction {
        maker: MAKER,
        offer_id_seed: OFFER_ID,
        token_mint_a: USDC_MINT,
        maker_token_account_a: MAKER_TOKEN_ACCOUNT_A,
        vault: VAULT,
    })
    .succeeds();
    make_offer(test, switched_deposit, switched_receive, TSLAX_MINT).succeeds();

    test.send(take).fails_with(EscrowError::OfferTermsChanged);

    // The taker still holds their 1 TSLAx and their 1,000 USDC (so they
    // received no USDC), the maker's TSLAx account is still empty, and the
    // switched offer's USDC is still in the vault.
    assert_eq!(
        test.tokens(TAKER_TOKEN_ACCOUNT_B),
        TAKER_TSLAX,
        "the taker must not pay TSLAx (token B) for a switched offer"
    );
    assert_eq!(
        test.tokens(TAKER_TOKEN_ACCOUNT_A),
        TAKER_USDC,
        "the taker must receive no USDC (token A) from a switched offer"
    );
    assert_eq!(
        test.tokens(MAKER_TOKEN_ACCOUNT_B),
        0,
        "the maker must receive no TSLAx (token B) from a switched offer"
    );
    assert_eq!(
        test.tokens(VAULT),
        switched_deposit,
        "the switched offer's USDC stays in the vault"
    );
}

#[quasar_test]
fn test_take_offer_rejects_switched_offer(test: &mut Test) {
    // The maker re-makes the offer with 1 USDC in the vault instead of 250,
    // for the same 1 TSLAx.
    assert_switched_offer_refused(test, ONE_USDC, TSLAX_WANTED);
}

#[quasar_test]
fn test_take_offer_rejects_switched_offer_wanting_more_token_b(test: &mut Test) {
    // The maker re-makes the offer asking for 2 TSLAx instead of 1, for the
    // same 250 USDC.
    assert_switched_offer_refused(test, USDC_OFFERED, 2 * TSLAX_WANTED);
}
