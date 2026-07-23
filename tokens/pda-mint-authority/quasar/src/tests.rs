use {
    crate::{
        cpi::{CreateMintInstruction, MintTokensInstruction},
        MintPda,
    },
    quasar_test::prelude::*,
    spl_token::{solana_program::program_pack::Pack, state::Mint as MintState},
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([2; 32]);

#[quasar_test]
fn create_mint_initializes_the_pda_mint_as_its_own_authority(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let mint_pda = test.derive_pda(MintPda::seeds());

    // Deliberately not 9: proves the decimals instruction argument reaches
    // the initialize_mint2 CPI instead of being hardcoded.
    let requested_decimals = 6u8;

    // The mint PDA and both programs are canonical derivations, so the
    // generated instruction only asks for the payer.
    test.send(CreateMintInstruction {
        payer: PAYER,
        decimals: requested_decimals,
    })
    .succeeds();

    // The created mint must carry the requested decimals, and be its own
    // mint authority.
    let created_mint = test.account(mint_pda).expect("mint should exist");
    let mint_state = MintState::unpack(&created_mint.data).expect("valid mint");
    assert_eq!(mint_state.decimals, requested_decimals);
    assert_eq!(mint_state.mint_authority, Some(mint_pda).into());
}

#[quasar_test]
fn mint_tokens_mints_with_the_pda_authority(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let mint_pda = test.derive_pda(MintPda::seeds());

    // The mint authority is the mint PDA itself.
    test.add(Mint::new(mint_pda).at(mint_pda).decimals(9));
    test.add(TokenAccount::new(mint_pda, PAYER).at(TOKEN_ACCOUNT));

    let amount = 1_000_000_000u64;

    // The handler mints exactly the minor-unit amount passed: no decimal
    // scaling.
    test.send(MintTokensInstruction {
        payer: PAYER,
        token_account: TOKEN_ACCOUNT,
        amount,
    })
    .succeeds()
    .has_tokens(TOKEN_ACCOUNT, amount)
    .has_supply(mint_pda, amount);
}
