#![cfg_attr(not(test), no_std)]

use quasar_lang::{prelude::*, sysvars::Sysvar};
use quasar_spl::{initialize_mint2, prelude::*};

#[cfg(test)]
mod tests;

declare_id!("22222222222222222222222222222222222222222222");

/// SPL Mint account size in bytes.
const MINT_SPACE: usize = 82;

/// Creates a token mint and mints initial tokens to the creator's token account.
///
/// The Anchor version uses Metaplex for onchain metadata. Quasar's metadata
/// crate is demonstrated in the `nft-operations` example; this example focuses
/// on the core SPL Token operations: creating a mint and minting tokens.
#[program]
mod quasar_create_token {
    use super::*;

    /// Create a new token mint with the caller-supplied number of decimals.
    #[instruction(discriminator = 0)]
    pub fn create_token(
        ctx: Ctx<CreateTokenAccountConstraints>,
        decimals: u8,
    ) -> Result<(), ProgramError> {
        handle_create_token(&mut ctx.accounts, decimals)
    }

    /// Mint `amount` minor units to the creator's token account.
    #[instruction(discriminator = 1)]
    pub fn mint_tokens(
        ctx: Ctx<MintTokensAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        handle_mint_tokens(&mut ctx.accounts, amount)
    }
}

/// Accounts for creating a new token mint.
///
/// The mint is created and initialized in the handler (create_account +
/// initialize_mint2 CPIs) rather than through Quasar's `mint(...)` init
/// constraint, because constraint arguments must be account fields or
/// literals and cannot reference the `decimals` instruction argument.
#[derive(Accounts)]
pub struct CreateTokenAccountConstraints {
    #[account(mut)]
    pub payer: Signer,
    /// The new mint. Must sign (it is a fresh keypair account).
    #[account(mut)]
    pub mint: UncheckedAccount,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

/// Accounts for minting tokens to an existing token account.
#[derive(Accounts)]
pub struct MintTokensAccountConstraints {
    #[account(mut)]
    pub authority: Signer,
    #[account(mut)]
    pub mint: Account<Mint>,
    #[account(mut)]
    pub token_account: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
fn handle_create_token(
    accounts: &mut CreateTokenAccountConstraints,
    decimals: u8,
) -> Result<(), ProgramError> {
    let payer_address = *accounts.payer.address();

    let rent = Rent::get()?;
    let lamports = rent.minimum_balance_unchecked(MINT_SPACE);

    accounts
        .system_program
        .create_account(
            &accounts.payer,
            &accounts.mint,
            lamports,
            MINT_SPACE as u64,
            accounts.token_program.address(),
        )
        .invoke()?;

    initialize_mint2(
        accounts.token_program.to_account_view(),
        accounts.mint.to_account_view(),
        decimals,
        &payer_address,
        None,
    )
    .invoke()
}

#[inline(always)]
fn handle_mint_tokens(
    accounts: &mut MintTokensAccountConstraints,
    amount: u64,
) -> Result<(), ProgramError> {
    accounts
        .token_program
        .mint_to(
            &accounts.mint,
            &accounts.token_account,
            &accounts.authority,
            amount,
        )
        .invoke()
}
