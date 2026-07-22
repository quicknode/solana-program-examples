use {
    crate::{cpi::CreateAddressInfoInstruction, state::AddressInfo},
    quasar_lang::client::DynString,
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);

#[quasar_test]
fn create_address_info_stores_the_compact_dynamic_layout(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let address_info = test.derive_pda(AddressInfo::seeds(&PAYER));

    // The address-info PDA and system program are canonical derivations, so
    // the generated instruction only asks for the payer and the args.
    test.send(CreateAddressInfoInstruction {
        payer: PAYER,
        house_number: 42,
        name: DynString::new("Alice"),
        street: DynString::new("Main Street"),
        city: DynString::new("New York"),
    })
    .succeeds();

    // The byte layout is the point of this example: a Quasar `#[account]`
    // with dynamic fields uses the compact "header then tail" format. Length
    // prefixes are grouped in the header, the actual bytes follow in the
    // tail.
    //   header: [disc: 1][house_number: u8][name_len: u8][street_len: u8][city_len: u8]
    //   tail:   [name bytes][street bytes][city bytes]
    // String<50> defaults to a u8 length prefix because MAX (50) fits in a
    // byte.
    let account = test.account(address_info).unwrap();
    assert_eq!(account.data[0], 1, "discriminator");
    assert_eq!(account.data[1], 42, "house_number");
    let name_len = account.data[2] as usize;
    let street_len = account.data[3] as usize;
    let city_len = account.data[4] as usize;
    assert_eq!(name_len, 5);
    assert_eq!(street_len, 11);
    assert_eq!(city_len, 8);

    let header_end = 5;
    assert_eq!(&account.data[header_end..header_end + name_len], b"Alice");
    let street_start = header_end + name_len;
    assert_eq!(
        &account.data[street_start..street_start + street_len],
        b"Main Street"
    );
    let city_start = street_start + street_len;
    assert_eq!(
        &account.data[city_start..city_start + city_len],
        b"New York"
    );
}
