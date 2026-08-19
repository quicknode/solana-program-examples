#![cfg_attr(not(test), no_std)]

use quasar_lang::{cpi::Seed, prelude::*, sysvars::Sysvar};
use quasar_spl::prelude::*;

#[cfg(test)]
mod tests;

declare_id!("3LFrPHqwk5jMrmiz48BFj6NV2k4NjobgTe1jChzx3JGD");

/// SPL Mint account size in bytes.
const MINT_SPACE: usize = 82;

/// Marker for the PDA at seeds = ["mint"]; used by the new
/// `address = MintPda::seeds()` form (post-PR-#195) to derive the mint PDA.
#[derive(Seeds)]
#[seeds(b"mint")]
pub struct MintPda;

/// Demonstrates using a PDA as the mint authority for an SPL token.
///
/// The mint account is created at the PDA address derived from `["mint"]`.
/// The same PDA serves as both the mint address AND the mint authority,
/// so minting requires PDA signing.
#[program]
mod quasar_pda_mint_authority {
    use super::*;

    /// Create a token mint at a PDA with the caller-supplied number of
    /// decimals. The PDA is its own mint authority.
    #[instruction(discriminator = 0)]
    pub fn create_mint(
        ctx: Ctx<CreateMintAccountConstraints>,
        decimals: u8,
    ) -> Result<(), ProgramError> {
        handle_create_mint(&mut ctx.accounts, decimals, ctx.bumps.mint)
    }

    /// Mint `amount` minor units using the PDA mint authority.
    #[instruction(discriminator = 1)]
    pub fn mint_tokens(
        ctx: Ctx<MintTokensAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        handle_mint_tokens(&mut ctx.accounts, amount, ctx.bumps.mint)
    }
}

/// Create the mint at a PDA. Manually created and initialized to avoid
/// a borrow conflict from `mint(authority = mint)` in the init constraint.
#[derive(Accounts)]
pub struct CreateMintAccountConstraints {
    #[account(mut)]
    pub payer: Signer,
    /// The PDA that will become the mint (and its own authority).
    #[account(mut, address = MintPda::seeds())]
    pub mint: UncheckedAccount,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
fn handle_create_mint(
    accounts: &mut CreateMintAccountConstraints,
    decimals: u8,
    bump: u8,
) -> Result<(), ProgramError> {
    let mint_address = *accounts.mint.address();
    let bump_bytes = [bump];
    let seeds: &[Seed] = &[
        Seed::from(b"mint" as &[u8]),
        Seed::from(&bump_bytes as &[u8]),
    ];

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
        .invoke_signed(seeds)?;

    accounts
        .token_program
        .initialize_mint2(&accounts.mint, decimals, &mint_address, None)
        .invoke()
}

/// Mint tokens to a token account, signing with the PDA mint authority.
#[derive(Accounts)]
pub struct MintTokensAccountConstraints {
    #[account(mut)]
    pub payer: Signer,
    /// The PDA mint whose authority is itself.
    ///
    /// Typed as `InterfaceAccount<Mint>` rather than `Account<Mint>` because
    /// newer quasar-lang requires `T: Discriminator` when combining `address =`
    /// with `Account<T>` (it reads `T::BUMP_OFFSET`). SPL `Mint` doesn't
    /// implement `Discriminator`; `InterfaceAccount` takes the generic
    /// existing-account verifier path that doesn't need it.
    #[account(mut, address = MintPda::seeds())]
    pub mint: InterfaceAccount<Mint>,
    /// Recipient token account (must already exist).
    #[account(mut)]
    pub token_account: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
fn handle_mint_tokens(
    accounts: &mut MintTokensAccountConstraints,
    amount: u64,
    mint_bump: u8,
) -> Result<(), ProgramError> {
    let bump = [mint_bump];
    let seeds: &[Seed] = &[Seed::from(b"mint" as &[u8]), Seed::from(&bump as &[u8])];

    let mint_view = accounts.mint.to_account_view().clone();
    accounts
        .token_program
        .mint_to(&mint_view, &accounts.token_account, &mint_view, amount)
        .invoke_signed(seeds)
}
