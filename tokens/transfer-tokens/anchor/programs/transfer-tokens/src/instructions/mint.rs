use {
    anchor_lang::prelude::*,
    anchor_spl::{
        associated_token::AssociatedToken,
        token_interface::{mint_to, Mint, MintTo, TokenAccount, TokenInterface},
    },
};

#[derive(Accounts)]
pub struct MintTokenAccountConstraints {
    #[account(mut)]
    pub mint_authority: Signer,

    pub recipient: SystemAccount,

    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,

    #[account(
        init_if_needed,
        payer = mint_authority,
        associated_token::mint = mint_account,
        associated_token::authority = recipient,
        associated_token::token_program = token_program,
    )]
    pub associated_token_account: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}

/// Mints `amount` tokens to the recipient's associated token account.
///
/// `amount` is in minor units (the raw integer the token program operates
/// on). Clients convert from major units, e.g. 1 token with 9 decimals is
/// `1 * 10u64.pow(9)` minor units.
pub fn handle_mint_token(
    context: &mut Context<MintTokenAccountConstraints>,
    amount: u64,
) -> Result<()> {
    msg!("Minting tokens to associated token account...");
    msg!("Mint: {}", &context.accounts.mint_account.address());
    msg!(
        "Token Address: {}",
        &context.accounts.associated_token_account.address()
    );

    // Invoke the mint_to instruction on the token program
    mint_to(
        CpiContext::new(
            context.accounts.token_program.address(),
            MintTo {
                mint: context.accounts.mint_account.cpi_handle_mut(),
                to: context.accounts.associated_token_account.cpi_handle_mut(),
                authority: context.accounts.mint_authority.cpi_handle(),
            },
        ),
        amount,
    )?;

    msg!("Token minted successfully.");

    Ok(())
}
