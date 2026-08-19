#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod instructions;
use instructions::*;
pub mod state;
#[cfg(test)]
mod tests;

declare_id!("ww9C83noARSQVBnqmCUmaVdbJjmiwcV9j2LkXYMoUCV");

#[program]
mod quasar_favorites {
    use super::*;

    /// Set the user's favourite number and colour.
    ///
    /// The Anchor version also takes `hobbies: Vec<String>`, but Quasar doesn't
    /// support nested dynamic types. See state.rs for details.
    #[instruction(discriminator = 0)]
    pub fn set_favorites(
        ctx: Ctx<SetFavoritesAccountConstraints>,
        number: u64,
        color: String<50>,
    ) -> Result<(), ProgramError> {
        instructions::handle_set_favorites(&mut ctx.accounts, number, color)
    }
}
