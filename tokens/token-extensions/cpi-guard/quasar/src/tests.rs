use {crate::cpi::CpiTransferInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const SENDER: Pubkey = Pubkey::new_from_array([1; 32]);
const SENDER_TA: Pubkey = Pubkey::new_from_array([2; 32]);
const MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const RECIPIENT_TA: Pubkey = Pubkey::new_from_array([4; 32]);

/// CPI transfer_checked without CPI Guard enabled succeeds.
#[quasar_test]
fn cpi_transfer_succeeds_without_cpi_guard(test: &mut Test) {
    test.add(Wallet::new().at(SENDER));
    test.add(
        Mint::new(SENDER)
            .at(MINT)
            .supply(1_000)
            .decimals(9)
            .token_program(TokenProgram::Token2022),
    );
    test.add(
        TokenAccount::new(MINT, SENDER)
            .at(SENDER_TA)
            .amount(100)
            .token_program(TokenProgram::Token2022),
    );
    test.add(
        TokenAccount::new(MINT, SENDER)
            .at(RECIPIENT_TA)
            .token_program(TokenProgram::Token2022),
    );

    test.send(CpiTransferInstruction {
        sender: SENDER,
        sender_token_account: SENDER_TA,
        mint_account: MINT,
        recipient_token_account: RECIPIENT_TA,
    })
    .succeeds()
    .has_tokens(SENDER_TA, 99)
    .has_tokens(RECIPIENT_TA, 1);
}
