#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod instructions;
use instructions::*;
#[cfg(test)]
mod tests;

declare_id!("3of89Z9jwek9zrFgpCWc9jZvQvitpVMxpZNsrAD2vQUD");

/// Token minter with Metaplex metadata.
///
/// Two instructions:
/// - `create_token` - creates a mint and associated Metaplex metadata account
/// - `mint_token` - mints tokens to a recipient's associated token account
#[program]
mod quasar_token_minter {
    use super::*;

    // String capacities follow Metaplex Token Metadata limits:
    // name ≤ 32, symbol ≤ 10, uri ≤ 200. PodString<N> requires an explicit
    // capacity - bare `String` (no <N>) is not accepted.
    #[instruction(discriminator = 0)]
    pub fn create_token(
        ctx: Ctx<CreateTokenAccountConstraints>,
        token_name: String<32>,
        token_symbol: String<10>,
        token_uri: String<200>,
    ) -> Result<(), ProgramError> {
        instructions::handle_create_token(&mut ctx.accounts, token_name, token_symbol, token_uri)
    }

    /// Mint `amount` minor units of the token to the recipient.
    #[instruction(discriminator = 1)]
    pub fn mint_token(
        ctx: Ctx<MintTokenAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_mint_token(&mut ctx.accounts, amount)
    }
}
