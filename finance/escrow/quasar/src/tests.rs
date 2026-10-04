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
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const MAKER: Pubkey = Pubkey::new_from_array([1; 32]);
const TAKER: Pubkey = Pubkey::new_from_array([2; 32]);
const TOKEN_MINT_A: Pubkey = Pubkey::new_from_array([3; 32]);
const TOKEN_MINT_B: Pubkey = Pubkey::new_from_array([4; 32]);
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
const DEPOSIT_AMOUNT: u64 = 1337;
const RECEIVE_AMOUNT: u64 = 1337;

/// Register the maker and both mints.
fn base_world(test: &mut Test) {
    test.add(Wallet::new().at(MAKER));
    test.add(
        Mint::new(MAKER)
            .at(TOKEN_MINT_A)
            .supply(1_000_000_000)
            .decimals(9),
    );
    test.add(
        Mint::new(MAKER)
            .at(TOKEN_MINT_B)
            .supply(1_000_000_000)
            .decimals(9),
    );
}

/// Register a live offer holding `DEPOSIT_AMOUNT` vault tokens, exactly as
/// `make_offer` leaves it.
fn live_offer(test: &mut Test) -> Pubkey {
    let (offer, bump) = test.derive_pda_with_bump(Offer::seeds(&MAKER, OFFER_ID));
    test.write(
        offer,
        OfferData {
            id: OFFER_ID.into(),
            maker: MAKER,
            token_mint_a: TOKEN_MINT_A,
            token_mint_b: TOKEN_MINT_B,
            maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
            vault: VAULT,
            receive: RECEIVE_AMOUNT.into(),
            bump,
        },
    );
    test.add(
        TokenAccount::new(TOKEN_MINT_A, offer)
            .at(VAULT)
            .amount(DEPOSIT_AMOUNT),
    );
    offer
}

#[quasar_test]
fn make_offer_records_the_offer_and_funds_the_vault(test: &mut Test) {
    base_world(test);
    test.add(
        TokenAccount::new(TOKEN_MINT_A, MAKER)
            .at(MAKER_TOKEN_ACCOUNT_A)
            .amount(1_000_000),
    );
    let (offer, bump) = test.derive_pda_with_bump(Offer::seeds(&MAKER, OFFER_ID));

    test.send(MakeOfferInstruction {
        maker: MAKER,
        token_mint_a: TOKEN_MINT_A,
        token_mint_b: TOKEN_MINT_B,
        maker_token_account_a: MAKER_TOKEN_ACCOUNT_A,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: VAULT,
        id: OFFER_ID,
        deposit: DEPOSIT_AMOUNT,
        receive: RECEIVE_AMOUNT,
    })
    .succeeds()
    // The deposit landed in the vault.
    .has_tokens(VAULT, DEPOSIT_AMOUNT);

    // Verify the recorded offer state.
    let state = test.read::<Offer>(offer);
    assert_eq!(u64::from(state.id), OFFER_ID, "id");
    assert_eq!(state.maker, MAKER, "maker");
    assert_eq!(state.token_mint_a, TOKEN_MINT_A, "token_mint_a");
    assert_eq!(state.token_mint_b, TOKEN_MINT_B, "token_mint_b");
    assert_eq!(
        state.maker_token_account_b, MAKER_TOKEN_ACCOUNT_B,
        "maker_token_account_b"
    );
    assert_eq!(state.vault, VAULT, "vault");
    assert_eq!(u64::from(state.receive), RECEIVE_AMOUNT, "receive");
    assert_eq!(state.bump, bump, "bump");
}

#[quasar_test]
fn take_offer_swaps_tokens_and_returns_rent_to_the_maker(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(TAKER));
    let offer = live_offer(test);
    test.add(
        TokenAccount::new(TOKEN_MINT_B, TAKER)
            .at(TAKER_TOKEN_ACCOUNT_B)
            .amount(10_000),
    );
    test.add(TokenAccount::new(TOKEN_MINT_B, MAKER).at(MAKER_TOKEN_ACCOUNT_B));

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
        token_mint_a: TOKEN_MINT_A,
        token_mint_b: TOKEN_MINT_B,
        taker_token_account_a: TAKER_TOKEN_ACCOUNT_A,
        taker_token_account_b: TAKER_TOKEN_ACCOUNT_B,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: VAULT,
        minimum_token_a_out: DEPOSIT_AMOUNT,
        maximum_token_b_in: RECEIVE_AMOUNT,
    })
    .succeeds()
    // Token balances: the taker received the vault's mint A, the maker
    // received the wanted mint B.
    .has_tokens(TAKER_TOKEN_ACCOUNT_A, DEPOSIT_AMOUNT)
    .has_tokens(MAKER_TOKEN_ACCOUNT_B, RECEIVE_AMOUNT)
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
    test.add(
        TokenAccount::new(TOKEN_MINT_B, TAKER)
            .at(TAKER_TOKEN_ACCOUNT_B)
            .amount(10_000),
    );
    test.add(TokenAccount::new(TOKEN_MINT_B, MAKER).at(MAKER_TOKEN_ACCOUNT_B));

    // The attacker substitutes a different mint for token_mint_a. The
    // has_one(token_mint_a) binding to the offer state must reject it.
    test.add(
        Mint::new(MAKER)
            .at(WRONG_MINT)
            .supply(1_000_000_000)
            .decimals(9),
    );

    let result = test.send(TakeOfferInstruction {
        taker: TAKER,
        offer_id_seed: OFFER_ID,
        maker: MAKER,
        token_mint_a: WRONG_MINT,
        token_mint_b: TOKEN_MINT_B,
        taker_token_account_a: TAKER_TOKEN_ACCOUNT_A,
        taker_token_account_b: TAKER_TOKEN_ACCOUNT_B,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: VAULT,
        minimum_token_a_out: DEPOSIT_AMOUNT,
        maximum_token_b_in: RECEIVE_AMOUNT,
    });
    assert!(
        result.is_err(),
        "take_offer must reject a mint that does not match the offer state"
    );
}

#[quasar_test]
fn take_offer_rejects_a_vault_that_does_not_match_the_offer(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(TAKER));
    let offer = live_offer(test);
    test.add(
        TokenAccount::new(TOKEN_MINT_B, TAKER)
            .at(TAKER_TOKEN_ACCOUNT_B)
            .amount(10_000),
    );
    test.add(TokenAccount::new(TOKEN_MINT_B, MAKER).at(MAKER_TOKEN_ACCOUNT_B));

    // The attacker substitutes a different token account (same mint, also
    // owned by the offer PDA) for the vault. The has_one(vault) binding to
    // the offer state must reject it.
    test.add(
        TokenAccount::new(TOKEN_MINT_A, offer)
            .at(WRONG_VAULT)
            .amount(DEPOSIT_AMOUNT),
    );

    let result = test.send(TakeOfferInstruction {
        taker: TAKER,
        offer_id_seed: OFFER_ID,
        maker: MAKER,
        token_mint_a: TOKEN_MINT_A,
        token_mint_b: TOKEN_MINT_B,
        taker_token_account_a: TAKER_TOKEN_ACCOUNT_A,
        taker_token_account_b: TAKER_TOKEN_ACCOUNT_B,
        maker_token_account_b: MAKER_TOKEN_ACCOUNT_B,
        vault: WRONG_VAULT,
        minimum_token_a_out: DEPOSIT_AMOUNT,
        maximum_token_b_in: RECEIVE_AMOUNT,
    });
    assert!(
        result.is_err(),
        "take_offer must reject a vault that does not match the offer state"
    );
}

#[quasar_test]
fn cancel_offer_returns_deposit_and_rent_to_the_maker(test: &mut Test) {
    base_world(test);
    let offer = live_offer(test);
    // Pre-created with a zero balance so the maker's tokens can be compared
    // exactly after the cancel.
    test.add(TokenAccount::new(TOKEN_MINT_A, MAKER).at(MAKER_TOKEN_ACCOUNT_A));

    let offer_rent = test.lamports(offer);
    let vault_rent = test.lamports(VAULT);
    let maker_lamports_before = test.lamports(MAKER);

    test.send(CancelOfferInstruction {
        maker: MAKER,
        offer_id_seed: OFFER_ID,
        token_mint_a: TOKEN_MINT_A,
        maker_token_account_a: MAKER_TOKEN_ACCOUNT_A,
        vault: VAULT,
    })
    .succeeds()
    // The maker got their mint A tokens back.
    .has_tokens(MAKER_TOKEN_ACCOUNT_A, DEPOSIT_AMOUNT)
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
    test.add(TokenAccount::new(TOKEN_MINT_A, ATTACKER).at(ATTACKER_TOKEN_ACCOUNT_A));

    // The attacker signs as the "maker" but passes the real maker's offer.
    // has_one(maker) and the offer's PDA seeds both fail to match. The
    // builder would derive the attacker's own (nonexistent) offer PDA, so
    // the real offer address is substituted at the instruction level.
    let mut ix: Instruction = CancelOfferInstruction {
        maker: ATTACKER,
        offer_id_seed: OFFER_ID,
        token_mint_a: TOKEN_MINT_A,
        maker_token_account_a: ATTACKER_TOKEN_ACCOUNT_A,
        vault: VAULT,
    }
    .into();
    // Account order = the accounts-struct field order: maker, offer, ...
    ix.accounts[1].pubkey = offer;

    let result = test.send(ix);
    assert!(
        result.is_err(),
        "cancel_offer must reject a signer who is not the offer's maker"
    );
}

/// Send `make_offer` for the given amounts and wanted token, from a maker who
/// holds token A.
fn make_offer(test: &mut Test, deposit: u64, receive: u64, token_mint_b: Pubkey) -> Outcome {
    test.send(MakeOfferInstruction {
        maker: MAKER,
        token_mint_a: TOKEN_MINT_A,
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
        TokenAccount::new(TOKEN_MINT_A, MAKER)
            .at(MAKER_TOKEN_ACCOUNT_A)
            .amount(1_000_000),
    );
}

#[quasar_test]
fn make_offer_rejects_a_zero_deposit(test: &mut Test) {
    maker_holding_token_a(test);
    make_offer(test, 0, RECEIVE_AMOUNT, TOKEN_MINT_B).fails_with(EscrowError::ZeroAmount);
}

#[quasar_test]
fn make_offer_rejects_a_zero_receive_amount(test: &mut Test) {
    maker_holding_token_a(test);
    make_offer(test, DEPOSIT_AMOUNT, 0, TOKEN_MINT_B).fails_with(EscrowError::ZeroAmount);
}

#[quasar_test]
fn make_offer_rejects_an_offer_of_a_token_for_itself(test: &mut Test) {
    maker_holding_token_a(test);
    // Both mint slots hold the same account, and loading it twice fails
    // before the handler runs.
    make_offer(test, DEPOSIT_AMOUNT, RECEIVE_AMOUNT, TOKEN_MINT_A)
        .fails(ProgramError::Runtime("AccountBorrowFailed".into()));
}

/// The taker's `take_offer`, signed for the given terms.
fn take_offer_instruction(minimum_token_a_out: u64, maximum_token_b_in: u64) -> Instruction {
    TakeOfferInstruction {
        taker: TAKER,
        offer_id_seed: OFFER_ID,
        maker: MAKER,
        token_mint_a: TOKEN_MINT_A,
        token_mint_b: TOKEN_MINT_B,
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
    test.add(
        TokenAccount::new(TOKEN_MINT_B, TAKER)
            .at(TAKER_TOKEN_ACCOUNT_B)
            .amount(10_000),
    );

    make_offer(test, DEPOSIT_AMOUNT, RECEIVE_AMOUNT, TOKEN_MINT_B).succeeds();

    // The taker signs for the terms they saw.
    let take = take_offer_instruction(DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    // The maker switches the offer before the taker's transaction lands.
    test.send(CancelOfferInstruction {
        maker: MAKER,
        offer_id_seed: OFFER_ID,
        token_mint_a: TOKEN_MINT_A,
        maker_token_account_a: MAKER_TOKEN_ACCOUNT_A,
        vault: VAULT,
    })
    .succeeds();
    make_offer(test, switched_deposit, switched_receive, TOKEN_MINT_B).succeeds();

    test.send(take).fails_with(EscrowError::OfferTermsChanged);

    assert_eq!(
        test.tokens(TAKER_TOKEN_ACCOUNT_B),
        10_000,
        "the taker must not pay token B for a switched offer"
    );
    assert!(
        test.account(TAKER_TOKEN_ACCOUNT_A)
            .is_none_or(|account| account.lamports == 0),
        "the taker must receive no token A from a switched offer"
    );
    assert_eq!(test.tokens(MAKER_TOKEN_ACCOUNT_B), 0);
    assert_eq!(test.tokens(VAULT), switched_deposit);
}

#[quasar_test]
fn test_take_offer_rejects_switched_offer(test: &mut Test) {
    // The maker re-makes the offer with a single token A in the vault.
    assert_switched_offer_refused(test, 1, RECEIVE_AMOUNT);
}

#[quasar_test]
fn test_take_offer_rejects_switched_offer_wanting_more_token_b(test: &mut Test) {
    // The maker re-makes the offer asking for twice the token B.
    assert_switched_offer_refused(test, DEPOSIT_AMOUNT, 2 * RECEIVE_AMOUNT);
}
