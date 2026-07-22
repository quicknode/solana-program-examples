use {
    crate::{
        cpi::{CloseUserInstruction, CreateUserInstruction},
        state::User,
    },
    quasar_lang::{client::DynString, prelude::QuasarError},
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const USER: Pubkey = Pubkey::new_from_array([1; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([2; 32]);

fn create_user_instruction(user: Pubkey) -> CreateUserInstruction {
    CreateUserInstruction {
        user,
        name: DynString::new("Alice"),
    }
}

#[quasar_test]
fn create_user_stores_bump_owner_and_name(test: &mut Test) {
    test.add(Wallet::new().at(USER));
    let user_account = test.derive_pda(User::seeds(&USER));

    // The user PDA and system program are canonical derivations, so the
    // generated instruction only asks for the signer and the name.
    test.send(create_user_instruction(USER)).succeeds();

    // The byte layout is part of what this example demonstrates:
    //   [disc(1)] [ZC: bump(1) + user(32)] [name: u8 prefix + bytes]
    let account = test.account(user_account).unwrap();
    assert_eq!(account.data[0], 1, "discriminator should be 1");
    let bump = account.data[1];
    assert_ne!(bump, 0, "bump should be nonzero");
    assert_eq!(
        &account.data[2..34],
        USER.as_ref(),
        "stored user should match signer"
    );
    assert_eq!(account.data[34], 5, "name length");
    assert_eq!(&account.data[35..40], b"Alice", "name data");
}

#[quasar_test]
fn close_user_drains_the_account_back_to_the_user(test: &mut Test) {
    test.add(Wallet::new().at(USER));
    let user_account = test.derive_pda(User::seeds(&USER));
    test.send(create_user_instruction(USER)).succeeds();
    let rent = test.lamports(user_account);
    assert_ne!(rent, 0, "creation should have funded the PDA");
    let user_lamports = test.lamports(USER);

    test.send(CloseUserInstruction { user: USER })
        .succeeds()
        .is_closed(user_account)
        .has_lamports(USER, user_lamports + rent);
}

#[quasar_test]
fn close_user_rejects_a_non_owner(test: &mut Test) {
    test.add(Wallet::new().at(USER));
    test.add(Wallet::new().at(ATTACKER));
    let victim_account = test.derive_pda(User::seeds(&USER));
    test.send(create_user_instruction(USER)).succeeds();

    // The attacker signs as `user` but passes the victim's account: the PDA
    // derivation check must reject it before any lamports move. Account
    // order matches CloseUserAccountConstraints: [user, user_account].
    let mut instruction: Instruction = CloseUserInstruction { user: ATTACKER }.into();
    instruction.accounts[1].pubkey = victim_account;

    test.send(instruction).fails_with(QuasarError::InvalidPda);
    assert!(
        test.account(victim_account).is_some_and(|account| !account.data.is_empty()),
        "the victim's account must survive the failed close"
    );
}
