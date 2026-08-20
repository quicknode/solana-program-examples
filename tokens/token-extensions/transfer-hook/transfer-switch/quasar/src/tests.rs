use {
    crate::cpi::{
        ConfigureAdminInstruction, InitializeExtraAccountMetasListInstruction, SwitchInstruction,
        TransferHookInstruction,
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const ADMIN: Pubkey = Pubkey::new_from_array([1; 32]);
const NEW_ADMIN: Pubkey = Pubkey::new_from_array([2; 32]);
const WALLET: Pubkey = Pubkey::new_from_array([3; 32]);
const MINT: Pubkey = Pubkey::new_from_array([4; 32]);
const SOURCE_TOKEN: Pubkey = Pubkey::new_from_array([5; 32]);
const DEST_TOKEN: Pubkey = Pubkey::new_from_array([6; 32]);

/// (admin_config, extra_account_metas_list, wallet_switch) PDAs. The program
/// derives these with raw seed literals, so the test mirrors the derivation.
fn pdas() -> (Pubkey, Pubkey, Pubkey) {
    let program_id: Pubkey = crate::ID.into();
    let (admin_config, _) = Pubkey::find_program_address(&[b"admin-config"], &program_id);
    let (meta_list, _) =
        Pubkey::find_program_address(&[b"extra-account-metas", MINT.as_ref()], &program_id);
    let (wallet_switch, _) = Pubkey::find_program_address(&[WALLET.as_ref()], &program_id);
    (admin_config, meta_list, wallet_switch)
}

fn hook_instruction(meta_list: Pubkey, wallet_switch: Pubkey) -> TransferHookInstruction {
    TransferHookInstruction {
        source_token_account: SOURCE_TOKEN,
        token_mint: MINT,
        receiver_token_account: DEST_TOKEN,
        wallet: WALLET,
        extra_account_metas_list: meta_list,
        wallet_switch,
        _amount: 100,
    }
}

#[quasar_test]
fn transfer_switch_gates_transfers_per_wallet(test: &mut Test) {
    test.add(Wallet::new().at(ADMIN));
    test.add(Wallet::new().at(NEW_ADMIN));
    let (admin_config, meta_list, wallet_switch) = pdas();

    // 1. Configure admin: the first caller installs NEW_ADMIN as the admin.
    test.send(ConfigureAdminInstruction {
        admin: ADMIN,
        new_admin: NEW_ADMIN,
        admin_config,
    })
    .succeeds();

    // 2. Initialize the extra account metas list.
    test.send(InitializeExtraAccountMetasListInstruction {
        payer: ADMIN,
        token_mint: MINT,
        extra_account_metas_list: meta_list,
    })
    .succeeds();

    // 3. Turn the switch ON for the wallet (NEW_ADMIN is now the admin).
    test.send(SwitchInstruction {
        admin: NEW_ADMIN,
        wallet: WALLET,
        admin_config,
        wallet_switch,
        on: 1,
    })
    .succeeds();

    // 4. Transfer hook with the switch ON succeeds.
    test.send(hook_instruction(meta_list, wallet_switch))
        .succeeds();

    // 5. Turn the switch OFF.
    test.send(SwitchInstruction {
        admin: NEW_ADMIN,
        wallet: WALLET,
        admin_config,
        wallet_switch,
        on: 0,
    })
    .succeeds();

    // 6. Transfer hook with the switch OFF is rejected.
    test.send(hook_instruction(meta_list, wallet_switch))
        .fails(ProgramError::InvalidArgument);
}
