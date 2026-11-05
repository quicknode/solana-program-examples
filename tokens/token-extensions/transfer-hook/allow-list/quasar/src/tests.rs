use {
    crate::cpi::{
        AllowWalletInstruction, DisallowWalletInstruction,
        InitializeExtraAccountMetaListInstruction, TransferHookInstruction,
    },
    quasar_test::prelude::*,
};

// The harness runs each instruction directly, so these tests call the hook the
// way Token Extensions would during a transfer, with the accounts it would pass.

// Deterministic addresses keep tests independent of discovery order.
const ISSUER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const SOURCE_TOKEN: Pubkey = Pubkey::new_from_array([3; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([4; 32]);
const OTHER_MINT: Pubkey = Pubkey::new_from_array([5; 32]);

const RECIPIENT_NOT_ALLOWED: ProgramError = ProgramError::Custom(6000);
const UNAUTHORIZED: ProgramError = ProgramError::Custom(6001);

fn pda(seeds: &[&[u8]]) -> Pubkey {
    let program_id: Pubkey = crate::ID.into();
    Pubkey::find_program_address(seeds, &program_id).0
}

fn meta_list(mint: &Pubkey) -> Pubkey {
    pda(&[b"extra-account-metas", mint.as_ref()])
}

fn allow_list(mint: &Pubkey) -> Pubkey {
    pda(&[b"allow_list", mint.as_ref()])
}

fn allowed_wallet(mint: &Pubkey, wallet: &Pubkey) -> Pubkey {
    pda(&[b"allowed_wallet", mint.as_ref(), wallet.as_ref()])
}

fn add_mint(test: &mut Test, mint: Pubkey) {
    test.add(
        Mint::new(ISSUER)
            .at(mint)
            .token_program(TokenProgram::Token2022),
    );
}

fn initialize(payer: Pubkey, mint: Pubkey) -> InitializeExtraAccountMetaListInstruction {
    InitializeExtraAccountMetaListInstruction {
        payer,
        extra_account_meta_list: meta_list(&mint),
        mint,
        allow_list: allow_list(&mint),
    }
}

fn allow(authority: Pubkey, mint: Pubkey, wallet: Pubkey) -> AllowWalletInstruction {
    AllowWalletInstruction {
        authority,
        mint,
        allow_list: allow_list(&mint),
        wallet,
        allowed_wallet: allowed_wallet(&mint, &wallet),
    }
}

fn disallow(authority: Pubkey, mint: Pubkey, wallet: Pubkey) -> DisallowWalletInstruction {
    DisallowWalletInstruction {
        authority,
        mint,
        allow_list: allow_list(&mint),
        wallet,
        allowed_wallet: allowed_wallet(&mint, &wallet),
    }
}

/// A recipient wallet with a token account for `mint`; returns (wallet, token account).
fn add_recipient(test: &mut Test, mint: Pubkey) -> (Pubkey, Pubkey) {
    let wallet = test.add(Wallet::new());
    let token_account =
        test.add(TokenAccount::new(mint, wallet).token_program(TokenProgram::Token2022));
    (wallet, token_account)
}

fn hook(mint: Pubkey, wallet: Pubkey, destination_token: Pubkey) -> TransferHookInstruction {
    TransferHookInstruction {
        source_token: SOURCE_TOKEN,
        mint,
        destination_token,
        owner: ISSUER,
        extra_account_meta_list: meta_list(&mint),
        allowed_wallet: allowed_wallet(&mint, &wallet),
        _amount: 100,
    }
}

/// An issuer with a mint whose allow list is set up.
fn setup(test: &mut Test) {
    test.add(Wallet::new().at(ISSUER));
    add_mint(test, MINT);
    test.send(initialize(ISSUER, MINT)).succeeds();
}

#[quasar_test]
fn transfer_to_an_allowed_wallet_succeeds(test: &mut Test) {
    setup(test);
    let (wallet, token_account) = add_recipient(test, MINT);
    test.send(allow(ISSUER, MINT, wallet)).succeeds();

    test.send(hook(MINT, wallet, token_account)).succeeds();
}

#[quasar_test]
fn transfer_to_a_wallet_not_on_the_list_is_rejected(test: &mut Test) {
    setup(test);
    let (wallet, token_account) = add_recipient(test, MINT);

    test.send(hook(MINT, wallet, token_account))
        .fails(RECIPIENT_NOT_ALLOWED);
}

#[quasar_test]
fn disallowing_a_wallet_stops_transfers_to_it(test: &mut Test) {
    setup(test);
    let (wallet, token_account) = add_recipient(test, MINT);
    test.send(allow(ISSUER, MINT, wallet)).succeeds();
    test.send(hook(MINT, wallet, token_account)).succeeds();

    test.send(disallow(ISSUER, MINT, wallet)).succeeds();

    test.send(hook(MINT, wallet, token_account))
        .fails(RECIPIENT_NOT_ALLOWED);
    assert_eq!(test.lamports(allowed_wallet(&MINT, &wallet)), 0);
}

/// The list has no size limit: each allowed wallet is its own account, so the
/// fiftieth costs the same as the first. The single-account design this
/// example replaced held eleven.
#[quasar_test]
fn the_list_holds_more_wallets_than_one_account_could(test: &mut Test) {
    setup(test);
    let mut recipients = Vec::new();
    for _ in 0..50 {
        let recipient = add_recipient(test, MINT);
        test.send(allow(ISSUER, MINT, recipient.0)).succeeds();
        recipients.push(recipient);
    }

    for (wallet, token_account) in recipients {
        test.send(hook(MINT, wallet, token_account)).succeeds();
    }
}

/// Allowing a wallet for one mint says nothing about another mint.
#[quasar_test]
fn an_allowed_wallet_is_allowed_for_one_mint_only(test: &mut Test) {
    setup(test);
    add_mint(test, OTHER_MINT);
    test.send(initialize(ISSUER, OTHER_MINT)).succeeds();
    let (wallet, _) = add_recipient(test, MINT);
    test.send(allow(ISSUER, MINT, wallet)).succeeds();
    let other_token_account =
        test.add(TokenAccount::new(OTHER_MINT, wallet).token_program(TokenProgram::Token2022));

    test.send(hook(OTHER_MINT, wallet, other_token_account))
        .fails(RECIPIENT_NOT_ALLOWED);
}

/// The hook reads the recipient from the destination token account, so the
/// AllowedWallet of some other wallet does not unlock a transfer.
#[quasar_test]
fn another_wallets_entry_does_not_unlock_a_transfer(test: &mut Test) {
    setup(test);
    let (allowed, _) = add_recipient(test, MINT);
    let (_, stranger_token_account) = add_recipient(test, MINT);
    test.send(allow(ISSUER, MINT, allowed)).succeeds();

    test.send(hook(MINT, allowed, stranger_token_account))
        .fails(ProgramError::InvalidSeeds);
}

/// Only the mint authority can set up a mint's allow list, so nobody can
/// claim the list for a mint they do not issue.
#[quasar_test]
fn only_the_mint_authority_can_create_the_allow_list(test: &mut Test) {
    test.add(Wallet::new().at(ISSUER));
    test.add(Wallet::new().at(ATTACKER));
    add_mint(test, MINT);

    test.send(initialize(ATTACKER, MINT)).fails(UNAUTHORIZED);
    test.send(initialize(ISSUER, MINT)).succeeds();
}

#[quasar_test]
fn only_the_authority_can_allow_a_wallet(test: &mut Test) {
    setup(test);
    test.add(Wallet::new().at(ATTACKER));
    let (wallet, token_account) = add_recipient(test, MINT);

    test.send(allow(ATTACKER, MINT, wallet)).fails(UNAUTHORIZED);
    test.send(hook(MINT, wallet, token_account))
        .fails(RECIPIENT_NOT_ALLOWED);
}

#[quasar_test]
fn only_the_authority_can_disallow_a_wallet(test: &mut Test) {
    setup(test);
    test.add(Wallet::new().at(ATTACKER));
    let (wallet, token_account) = add_recipient(test, MINT);
    test.send(allow(ISSUER, MINT, wallet)).succeeds();

    test.send(disallow(ATTACKER, MINT, wallet))
        .fails(UNAUTHORIZED);
    test.send(hook(MINT, wallet, token_account)).succeeds();
}
