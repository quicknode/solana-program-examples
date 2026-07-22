use {
    crate::cpi::{InitializeExtraAccountMetaListInstruction, TransferHookInstruction},
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const SOURCE_TOKEN: Pubkey = Pubkey::new_from_array([3; 32]);
const DESTINATION_TOKEN: Pubkey = Pubkey::new_from_array([4; 32]);
const OWNER: Pubkey = Pubkey::new_from_array([5; 32]);

/// (extra_account_meta_list, counter) PDAs. The program derives these with
/// raw seed literals, so the test mirrors the derivation directly.
fn pdas() -> (Pubkey, Pubkey) {
    let program_id: Pubkey = crate::ID.into();
    let (meta_list, _) =
        Pubkey::find_program_address(&[b"extra-account-metas", MINT.as_ref()], &program_id);
    let (counter, _) = Pubkey::find_program_address(&[b"counter", PAYER.as_ref()], &program_id);
    (meta_list, counter)
}

#[quasar_test]
fn initialize_then_transfer_hook_increments_the_counter(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let (meta_list, counter) = pdas();

    test.send(InitializeExtraAccountMetaListInstruction {
        payer: PAYER,
        extra_account_meta_list: meta_list,
        mint: MINT,
        counter_account: counter,
    })
    .succeeds();

    test.send(TransferHookInstruction {
        source_token: SOURCE_TOKEN,
        mint: MINT,
        destination_token: DESTINATION_TOKEN,
        owner: OWNER,
        extra_account_meta_list: meta_list,
        counter_account: counter,
        _amount: 1,
    })
    .succeeds();

    // Layout is [8-byte header][u64 counter]; the byte offset is the point of
    // this program, so check the bytes directly.
    let account = test.account(counter).expect("counter missing");
    let count = u64::from_le_bytes(account.data[8..16].try_into().unwrap());
    assert_eq!(count, 1, "counter should be 1");
}
