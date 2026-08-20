#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

#[cfg(test)]
mod tests;

declare_id!("nHi9DdNjuupjQ3c8AJU9sChB5gLbZvTLsJQouY4hU67");

/// Demonstrates minting tokens and transferring them between accounts.
///
/// The Anchor variant also creates Metaplex metadata for the mint; this
/// variant focuses on the core token operations - minting and transferring -
/// and leaves metadata out. Both handlers take `amount` in minor units (the
/// raw integer the token program operates on); no scaling happens onchain.
#[program]
mod quasar_transfer_tokens {
    use super::*;

    /// Mint `amount` minor units to a recipient's token account.
    #[instruction(discriminator = 0)]
    pub fn mint_tokens(
        ctx: Ctx<MintTokensAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        handle_mint_tokens(&mut ctx.accounts, amount)
    }

    /// Transfer `amount` minor units from sender to recipient.
    #[instruction(discriminator = 1)]
    pub fn transfer_tokens(
        ctx: Ctx<TransferTokensAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        handle_transfer_tokens(&mut ctx.accounts, amount)
    }
}

/// Accounts for minting tokens to a recipient.
#[derive(Accounts)]
pub struct MintTokensAccountConstraints {
    #[account(mut)]
    pub mint_authority: Signer,
    #[account(mut)]
    pub mint: Account<Mint>,
    /// The recipient's token account. Must already exist.
    #[account(mut)]
    pub recipient_token_account: Account<Token>,
    pub token_program: Program<TokenProgram>,
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
            &accounts.recipient_token_account,
            &accounts.mint_authority,
            amount,
        )
        .invoke()
}

/// Accounts for transferring tokens between two token accounts.
#[derive(Accounts)]
pub struct TransferTokensAccountConstraints {
    #[account(mut)]
    pub sender: Signer,
    #[account(mut)]
    pub sender_token_account: Account<Token>,
    #[account(mut)]
    pub recipient_token_account: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
fn handle_transfer_tokens(
    accounts: &mut TransferTokensAccountConstraints,
    amount: u64,
) -> Result<(), ProgramError> {
    accounts
        .token_program
        .transfer(
            &accounts.sender_token_account,
            &accounts.recipient_token_account,
            &accounts.sender,
            amount,
        )
        .invoke()
}
