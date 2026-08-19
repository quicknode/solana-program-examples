#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod instructions;
use instructions::*;
pub mod state;
#[cfg(test)]
mod tests;

declare_id!("GpVcgWdgVErgLqsn8VYUch6EqDerMgNqoLSmGyKrd6MR");

#[program]
mod quasar_account_data {
    use super::*;

    /// Create an address info account with name, house number, street, and city.
    ///
    /// Uses Quasar's `String` marker type for instruction args to get u32 length
    /// prefixes in the wire format. `&str` would be a Tail type (no prefix) which
    /// only works for a single dynamic argument.
    ///
    /// After macro expansion, `String` args become `&str` local bindings, so we
    /// pass them directly (not by reference) to the handler.
    #[instruction(discriminator = 0)]
    pub fn create_address_info(
        ctx: Ctx<CreateAddressInfoAccountConstraints>,
        house_number: u8,
        name: String<50>,
        street: String<50>,
        city: String<50>,
    ) -> Result<(), ProgramError> {
        instructions::handle_create_address_info(
            &mut ctx.accounts,
            name,
            house_number,
            street,
            city,
        )
    }
}
