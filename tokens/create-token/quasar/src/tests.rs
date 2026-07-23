use {
    crate::cpi::{CreateTokenInstruction, MintTokensInstruction},
    quasar_test::prelude::*,
    spl_token::{solana_program::program_pack::Pack, state::Mint as MintState},
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([3; 32]);

#[quasar_test]
fn create_token_initializes_the_mint_with_requested_decimals(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    // Deliberately not 9: proves the decimals instruction argument reaches
    // the initialize_mint2 CPI instead of being hardcoded.
    let requested_decimals = 6u8;

    // The mint is a fresh keypair account that must co-sign create_account.
    // The accounts struct types it UncheckedAccount, so the generated builder
    // marks it writable only; flip the signer bit on the built instruction
    // (field order: payer, mint, token_program, system_program).
    let mut instruction: Instruction = CreateTokenInstruction {
        payer: PAYER,
        mint: MINT,
        decimals: requested_decimals,
    }
    .into();
    instruction.accounts[1].is_signer = true;

    test.send(instruction).succeeds();

    // The created mint must carry the requested decimals and the payer as
    // its mint authority.
    let mint_account = test.account(MINT).expect("mint should exist");
    let mint_state = MintState::unpack(&mint_account.data).expect("valid mint");
    assert_eq!(mint_state.decimals, requested_decimals);
    assert_eq!(mint_state.mint_authority, Some(PAYER).into());
}

#[quasar_test]
fn mint_tokens_mints_the_exact_minor_unit_amount(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    test.add(Mint::new(PAYER).at(MINT).decimals(9));
    test.add(TokenAccount::new(MINT, PAYER).at(TOKEN_ACCOUNT));

    let amount = 1_000_000_000u64;

    // The handler mints exactly the minor-unit amount passed: no decimal
    // scaling.
    test.send(MintTokensInstruction {
        authority: PAYER,
        mint: MINT,
        token_account: TOKEN_ACCOUNT,
        amount,
    })
    .succeeds()
    .has_tokens(TOKEN_ACCOUNT, amount)
    .has_supply(MINT, amount);
}
