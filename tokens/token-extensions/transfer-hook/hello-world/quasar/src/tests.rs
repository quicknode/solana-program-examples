use {
    crate::cpi::{
        InitializeExtraAccountMetaListInstruction, InitializeInstruction, TransferHookInstruction,
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const SOURCE_TOKEN: Pubkey = Pubkey::new_from_array([3; 32]);
const DESTINATION_TOKEN: Pubkey = Pubkey::new_from_array([4; 32]);
const OWNER: Pubkey = Pubkey::new_from_array([5; 32]);

/// ExtraAccountMetaList PDA. The program derives it with raw seed literals,
/// so the test mirrors the derivation directly.
fn meta_list_pda() -> Pubkey {
    let program_id: Pubkey = crate::ID.into();
    Pubkey::find_program_address(&[b"extra-account-metas", MINT.as_ref()], &program_id).0
}

#[quasar_test]
fn initialize_creates_a_mint_with_the_transfer_hook_extension(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    test.send(InitializeInstruction {
        payer: PAYER,
        mint_account: MINT,
        decimals: 2,
    })
    .succeeds();

    // The mint is now owned by Token-2022 and sized for the extension TLV.
    let mint = test.account(MINT).expect("mint missing");
    assert_eq!(mint.owner, SPL_TOKEN_2022_PROGRAM_ID);
    assert_eq!(mint.data.len(), 234);
}

#[quasar_test]
fn initialize_extra_account_meta_list_creates_the_pda(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    // The mint does not need to exist for PDA derivation, just an address.
    test.send(InitializeExtraAccountMetaListInstruction {
        payer: PAYER,
        extra_account_meta_list: meta_list_pda(),
        mint: MINT,
    })
    .succeeds();
}

#[quasar_test]
fn transfer_hook_logs_and_succeeds(test: &mut Test) {
    test.add(Wallet::new().at(OWNER));

    test.send(TransferHookInstruction {
        source_token: SOURCE_TOKEN,
        mint: MINT,
        destination_token: DESTINATION_TOKEN,
        owner: OWNER,
        extra_account_meta_list: meta_list_pda(),
        _amount: 1,
    })
    .succeeds();
}
