use {
    crate::state::{AddressInfo, AddressInfoInner},
    quasar_lang::{prelude::*, sysvars::Sysvar},
};

/// Accounts for creating a new address info account.
#[derive(Accounts)]
pub struct CreateAddressInfo {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut, init, payer = payer, address = AddressInfo::seeds(payer.address()))]
    pub address_info: Account<AddressInfo>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_create_address_info(
    accounts: &mut CreateAddressInfo,
    name: &str,
    house_number: u8,
    street: &str,
    city: &str,
) -> Result<(), ProgramError> {
    let rent = Rent::get()?;
    accounts.address_info.set_inner(
        AddressInfoInner { house_number, name, street, city },
        accounts.payer.to_account_view(),
        rent.lamports_per_byte(),
        rent.exemption_threshold_raw(),
    )
}
