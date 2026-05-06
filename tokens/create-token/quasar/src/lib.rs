#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

#[cfg(test)]
mod tests;

declare_id!("22222222222222222222222222222222222222222222");

/// Creates a token mint and mints initial tokens to the creator's token account.
///
/// The Anchor version uses Metaplex for onchain metadata. Quasar's metadata
/// crate is demonstrated in the `nft-minter` and `spl-token-minter` examples;
/// this example focuses on the core SPL Token operations: creating a mint and
/// minting tokens.
#[program]
mod quasar_create_token {
    use super::*;

    /// Create a new token mint (account init handled by Quasar's `#[account(init)]`).
    #[instruction(discriminator = 0)]
    pub fn create_token(ctx: Ctx<CreateToken>, _decimals: u8) -> Result<(), ProgramError> {
        handle_create_token(&mut ctx.accounts)
    }

    /// Mint tokens to the creator's token account.
    #[instruction(discriminator = 1)]
    pub fn mint_tokens(ctx: Ctx<MintTokens>, amount: u64) -> Result<(), ProgramError> {
        handle_mint_tokens(&mut ctx.accounts, amount)
    }
}

/// Accounts for creating a new token mint.
/// Quasar's `#[account(init)]` handles the create_account + initialize_mint CPI.
#[derive(Accounts)]
pub struct CreateToken {
    #[account(mut)]
    pub payer: Signer,
    #[account(
        mut,
        init,
        payer = payer,
        mint(decimals = 9, authority = payer, freeze_authority = None, token_program = token_program),
    )]
    pub mint: Account<Mint>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

/// Accounts for minting tokens to an existing token account.
#[derive(Accounts)]
pub struct MintTokens {
    #[account(mut)]
    pub authority: Signer,
    #[account(mut)]
    pub mint: Account<Mint>,
    #[account(mut)]
    pub token_account: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
fn handle_create_token(_accounts: &mut CreateToken) -> Result<(), ProgramError> {
    // Mint account is created and initialised by Quasar's account init.
    Ok(())
}

#[inline(always)]
fn handle_mint_tokens(accounts: &mut MintTokens, amount: u64) -> Result<(), ProgramError> {
    accounts.token_program
        .mint_to(&accounts.mint, &accounts.token_account, &accounts.authority, amount)
        .invoke()
}
