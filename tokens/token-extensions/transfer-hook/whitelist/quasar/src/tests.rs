use {
    crate::cpi::{
        AddToWhitelistInstruction, InitializeExtraAccountMetaListInstruction,
        TransferHookInstruction,
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const SOURCE_TOKEN: Pubkey = Pubkey::new_from_array([3; 32]);
const DESTINATION_TOKEN: Pubkey = Pubkey::new_from_array([4; 32]);
const OWNER: Pubkey = Pubkey::new_from_array([5; 32]);
const BAD_DEST: Pubkey = Pubkey::new_from_array([6; 32]);

/// (extra_account_meta_list, white_list) PDAs. The program derives these with
/// raw seed literals, so the test mirrors the derivation directly.
fn pdas() -> (Pubkey, Pubkey) {
    let program_id: Pubkey = crate::ID.into();
    let (meta_list, _) =
        Pubkey::find_program_address(&[b"extra-account-metas", MINT.as_ref()], &program_id);
    let (white_list, _) = Pubkey::find_program_address(&[b"white_list"], &program_id);
    (meta_list, white_list)
}

fn hook_instruction(
    destination_token: Pubkey,
    meta_list: Pubkey,
    white_list: Pubkey,
) -> TransferHookInstruction {
    TransferHookInstruction {
        source_token: SOURCE_TOKEN,
        mint: MINT,
        destination_token,
        owner: OWNER,
        extra_account_meta_list: meta_list,
        white_list,
        _amount: 100,
    }
}

#[quasar_test]
fn whitelist_gates_transfers_by_destination(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let (meta_list, white_list) = pdas();

    // 1. Initialize the meta list and whitelist (payer becomes authority).
    test.send(InitializeExtraAccountMetaListInstruction {
        payer: PAYER,
        extra_account_meta_list: meta_list,
        mint: MINT,
        white_list,
    })
    .succeeds();

    // 2. Add the destination token account to the whitelist.
    test.send(AddToWhitelistInstruction {
        signer: PAYER,
        new_account: DESTINATION_TOKEN,
        white_list,
    })
    .succeeds();

    // 3. Transfer hook with a whitelisted destination succeeds.
    test.send(hook_instruction(DESTINATION_TOKEN, meta_list, white_list))
        .succeeds();

    // 4. Transfer hook with a non-whitelisted destination is rejected.
    test.send(hook_instruction(BAD_DEST, meta_list, white_list))
        .fails(ProgramError::InvalidArgument);
}
