use {
    anchor_lang::prelude::*,
    anchor_spl::{
        associated_token::AssociatedToken,
        token::{mint_to, Mint, MintTo, Token, TokenAccount},
    },
};

#[derive(Accounts)]
pub struct MintTokenAccountConstraints<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    // Mint account address is a PDA
    #[account(
        mut,
        seeds = [b"mint"],
        bump
    )]
    pub mint_account: Account<'info, Mint>,

    // Create Associated Token Account, if needed
    // This is the account that will hold the minted tokens
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_account,
        associated_token::authority = payer,
    )]
    pub associated_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Mints `amount` tokens to the payer's associated token account, signed by
/// the PDA mint authority.
///
/// `amount` is in minor units (the raw integer the token program operates
/// on). Clients convert from major units, e.g. 1 token with 9 decimals is
/// `1 * 10u64.pow(9)` minor units.
pub fn handle_mint_token(
    context: Context<MintTokenAccountConstraints>,
    amount: u64,
) -> Result<()> {
    msg!("Minting token to associated token account...");
    msg!("Mint: {}", &context.accounts.mint_account.key());
    msg!(
        "Token Address: {}",
        &context.accounts.associated_token_account.key()
    );

    // PDA signer seeds
    let signer_seeds: &[&[&[u8]]] = &[&[b"mint", &[context.bumps.mint_account]]];

    // Invoke the mint_to instruction on the token program
    mint_to(
        CpiContext::new(
            context.accounts.token_program.key(),
            MintTo {
                mint: context.accounts.mint_account.to_account_info(),
                to: context.accounts.associated_token_account.to_account_info(),
                authority: context.accounts.mint_account.to_account_info(), // PDA mint authority, required as signer
            },
        )
        .with_signer(signer_seeds), // using PDA to sign
        amount,
    )?;

    msg!("Token minted successfully.");

    Ok(())
}
