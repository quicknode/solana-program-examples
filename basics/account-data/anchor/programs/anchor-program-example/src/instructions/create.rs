use crate::state::AddressInfo;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CreateAddressInfoAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        init,
        payer = payer,
        space = AddressInfo::DISCRIMINATOR.len() + AddressInfo::INIT_SPACE,
    )]
    pub address_info: BorshAccount<AddressInfo>,
    pub system_program: Program<System>,
}

pub fn handle_create_address_info(
    context: &mut Context<CreateAddressInfoAccountConstraints>,
    name: String,
    house_number: u8,
    street: String,
    city: String,
) -> Result<()> {
    *context.accounts.address_info = AddressInfo {
        name,
        house_number,
        street,
        city,
    };
    Ok(())
}
