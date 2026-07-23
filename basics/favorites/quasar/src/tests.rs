use {
    crate::{cpi::SetFavoritesInstruction, state::Favorites},
    quasar_lang::client::DynString,
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const USER: Pubkey = Pubkey::new_from_array([1; 32]);

#[quasar_test]
fn set_favorites_stores_number_and_color(test: &mut Test) {
    test.add(Wallet::new().at(USER));
    let favorites = test.derive_pda(Favorites::seeds(&USER));

    // The favorites PDA and system program are canonical derivations, so the
    // generated instruction only asks for the user and the args.
    test.send(SetFavoritesInstruction {
        user: USER,
        number: 42,
        color: DynString::new("blue"),
    })
    .succeeds();

    // The byte layout is part of what this example demonstrates: a fixed
    // field (number) followed by a dynamic field (color).
    //   [disc(1)] [ZC: number(8 bytes)] [color: u8 prefix + bytes]
    let account = test.account(favorites).unwrap();
    assert_eq!(account.data[0], 1, "discriminator");
    let number = u64::from_le_bytes(account.data[1..9].try_into().unwrap());
    assert_eq!(number, 42, "favourite number");
    assert_eq!(account.data[9], 4, "color length");
    assert_eq!(&account.data[10..14], b"blue", "color data");
}
