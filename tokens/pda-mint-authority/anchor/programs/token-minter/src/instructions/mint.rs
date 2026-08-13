use {
    anchor_lang::prelude::*,
    anchor_spl::{
        associated_token::AssociatedToken,
        token::{mint_to, Mint, MintTo, Token, TokenAccount},
    },
};

#[derive(Accounts)]
pub struct MintTokenAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    // Mint account address is a PDA
    #[account(
        mut,
        seeds = [b"mint"],
        bump
    )]
    pub mint_account: Account<Mint>,

    // Create Associated Token Account, if needed
    // This is the account that will hold the minted tokens
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_account,
        associated_token::authority = payer,
    )]
    pub associated_token_account: Account<TokenAccount>,

    pub token_program: Program<Token>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}

/// Mints `amount` tokens to the payer's associated token account, signed by
/// the PDA mint authority.
///
/// `amount` is in minor units (the raw integer the token program operates
/// on). Clients convert from major units, e.g. 1 token with 9 decimals is
/// `1 * 10u64.pow(9)` minor units.
pub fn handle_mint_token(
    context: &mut Context<MintTokenAccountConstraints>,
    amount: u64,
) -> Result<()> {
    msg!("Minting token to associated token account...");
    msg!("Mint: {}", &context.accounts.mint_account.address());
    msg!(
        "Token Address: {}",
        &context.accounts.associated_token_account.address()
    );

    // PDA signer seeds
    let signer_seeds: &[&[&[u8]]] = &[&[b"mint", &[context.bumps.mint_account]]];

    // Invoke the mint_to instruction on the token program
    mint_to(
        CpiContext::new(
            context.accounts.token_program.address(),
            MintTo {
                mint: context.accounts.mint_account.cpi_handle_mut(),
                to: context.accounts.associated_token_account.cpi_handle_mut(),
                authority: context.accounts.mint_account.cpi_handle(), // PDA mint authority, required as signer
            },
        )
        .with_signer(signer_seeds), // using PDA to sign
        amount,
    )?;

    msg!("Token minted successfully.");

    Ok(())
}
