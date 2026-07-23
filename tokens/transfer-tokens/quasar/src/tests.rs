use {
    crate::cpi::{MintTokensInstruction, TransferTokensInstruction},
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const AUTHORITY: Pubkey = Pubkey::new_from_array([1; 32]);
const SENDER: Pubkey = Pubkey::new_from_array([2; 32]);
const RECIPIENT: Pubkey = Pubkey::new_from_array([3; 32]);
const MINT: Pubkey = Pubkey::new_from_array([4; 32]);
const SENDER_TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([5; 32]);
const RECIPIENT_TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([6; 32]);

#[quasar_test]
fn mint_tokens_mints_the_exact_minor_unit_amount(test: &mut Test) {
    test.add(Wallet::new().at(AUTHORITY));
    test.add(Mint::new(AUTHORITY).at(MINT).decimals(9));
    test.add(TokenAccount::new(MINT, AUTHORITY).at(RECIPIENT_TOKEN_ACCOUNT));

    let amount = 1_000_000_000u64;

    test.send(MintTokensInstruction {
        mint_authority: AUTHORITY,
        mint: MINT,
        recipient_token_account: RECIPIENT_TOKEN_ACCOUNT,
        amount,
    })
    .succeeds()
    .has_tokens(RECIPIENT_TOKEN_ACCOUNT, amount)
    .has_supply(MINT, amount);
}

#[quasar_test]
fn transfer_tokens_moves_tokens_between_accounts(test: &mut Test) {
    test.add(Wallet::new().at(SENDER));
    test.add(Mint::new(AUTHORITY).at(MINT).decimals(9).supply(10_000));
    test.add(
        TokenAccount::new(MINT, SENDER)
            .at(SENDER_TOKEN_ACCOUNT)
            .amount(10_000),
    );
    test.add(TokenAccount::new(MINT, RECIPIENT).at(RECIPIENT_TOKEN_ACCOUNT));

    let amount = 500u64;

    test.send(TransferTokensInstruction {
        sender: SENDER,
        sender_token_account: SENDER_TOKEN_ACCOUNT,
        recipient_token_account: RECIPIENT_TOKEN_ACCOUNT,
        amount,
    })
    .succeeds()
    .has_tokens(SENDER_TOKEN_ACCOUNT, 10_000 - amount)
    .has_tokens(RECIPIENT_TOKEN_ACCOUNT, amount);
}
