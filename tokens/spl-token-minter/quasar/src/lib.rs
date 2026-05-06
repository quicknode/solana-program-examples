#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod instructions;
use instructions::*;
#[cfg(test)]
mod tests;

declare_id!("22222222222222222222222222222222222222222222");

/// SPL token minter with Metaplex metadata.
///
/// Two instructions:
/// - `create_token` — creates a mint and associated Metaplex metadata account
/// - `mint_token` — mints tokens to a recipient's associated token account
#[program]
mod quasar_spl_token_minter {
    use super::*;

    // String capacities follow Metaplex Token Metadata limits:
    // name ≤ 32, symbol ≤ 10, uri ≤ 200. PodString<N> requires an explicit
    // capacity since PR #195 — `String` (no <N>) is no longer accepted.
    #[instruction(discriminator = 0)]
    pub fn create_token(
        ctx: Ctx<CreateToken>,
        token_name: String<32>,
        token_symbol: String<10>,
        token_uri: String<200>,
    ) -> Result<(), ProgramError> {
        instructions::handle_create_token(
            &mut ctx.accounts,
            &token_name,
            &token_symbol,
            &token_uri,
        )
    }

    #[instruction(discriminator = 1)]
    pub fn mint_token(ctx: Ctx<MintToken>, amount: u64) -> Result<(), ProgramError> {
        instructions::handle_mint_token(&mut ctx.accounts, amount)
    }
}
