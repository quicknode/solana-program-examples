use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

/// Accounts for minting tokens to a recipient's token account.
#[derive(Accounts)]
pub struct MintTokenAccountConstraints {
    #[account(mut)]
    pub mint_authority: Signer,
    pub recipient: UncheckedAccount,
    #[account(mut)]
    pub mint_account: Account<Mint>,
    #[account(
        mut,
        init(idempotent),
        payer = mint_authority,
        token(mint = mint_account, authority = recipient, token_program = token_program),
    )]
    pub associated_token_account: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

/// Mints `amount` tokens to the recipient's associated token account.
///
/// `amount` is in minor units (the raw integer the token program operates
/// on). Clients convert from major units, e.g. 1 token with 9 decimals is
/// `1 * 10u64.pow(9)` minor units.
#[inline(always)]
pub fn handle_mint_token(
    accounts: &mut MintTokenAccountConstraints,
    amount: u64,
) -> Result<(), ProgramError> {
    log("Minting tokens to associated token account...");

    accounts
        .token_program
        .mint_to(
            &accounts.mint_account,
            &accounts.associated_token_account,
            &accounts.mint_authority,
            amount,
        )
        .invoke()?;

    log("Token minted successfully.");
    Ok(())
}
