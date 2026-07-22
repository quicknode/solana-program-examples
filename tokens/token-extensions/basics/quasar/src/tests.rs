use {
    crate::cpi::{MintTokenInstruction, TransferTokenInstruction},
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const AUTHORITY: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const RECEIVER_TA: Pubkey = Pubkey::new_from_array([3; 32]);
const SENDER: Pubkey = Pubkey::new_from_array([4; 32]);
const FROM_TA: Pubkey = Pubkey::new_from_array([5; 32]);
const TO_TA: Pubkey = Pubkey::new_from_array([6; 32]);

#[quasar_test]
fn mint_token_mints_to_the_receiver(test: &mut Test) {
    test.add(Wallet::new().at(AUTHORITY));
    test.add(
        Mint::new(AUTHORITY)
            .at(MINT)
            .decimals(6)
            .token_program(TokenProgram::Token2022),
    );
    test.add(
        TokenAccount::new(MINT, AUTHORITY)
            .at(RECEIVER_TA)
            .token_program(TokenProgram::Token2022),
    );

    test.send(MintTokenInstruction {
        authority: AUTHORITY,
        mint: MINT,
        receiver: RECEIVER_TA,
        amount: 1_000_000,
    })
    .succeeds()
    .has_tokens(RECEIVER_TA, 1_000_000)
    .has_supply(MINT, 1_000_000);
}

#[quasar_test]
fn transfer_token_moves_tokens_via_transfer_checked(test: &mut Test) {
    test.add(Wallet::new().at(SENDER));
    test.add(
        Mint::new(SENDER)
            .at(MINT)
            .decimals(6)
            .token_program(TokenProgram::Token2022),
    );
    test.add(
        TokenAccount::new(MINT, SENDER)
            .at(FROM_TA)
            .amount(1_000)
            .token_program(TokenProgram::Token2022),
    );
    test.add(
        TokenAccount::new(MINT, SENDER)
            .at(TO_TA)
            .token_program(TokenProgram::Token2022),
    );

    test.send(TransferTokenInstruction {
        sender: SENDER,
        from: FROM_TA,
        mint: MINT,
        to: TO_TA,
        amount: 500,
    })
    .succeeds()
    .has_tokens(FROM_TA, 500)
    .has_tokens(TO_TA, 500);
}
