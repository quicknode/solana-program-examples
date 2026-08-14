use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    harvest_withheld_tokens_to_mint, HarvestWithheldTokensToMint, Mint, Token2022, TokenAccount,
};

#[derive(Accounts)]
pub struct HarvestAccountConstraints {
    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
}

// transfer fees are stored directly on the recipient token account and must be "harvested"
// "harvesting" transfers fees accumulated on token accounts to the mint account
pub fn process_harvest(context: &mut Context<HarvestAccountConstraints>) -> Result<()> {
    // Using remaining accounts to allow for passing in an unknown number of token accounts to harvest from
    // Check that remaining accounts are token accounts for the mint to harvest to
    let sources = context
        .remaining_accounts
        .iter()
        .filter_map(|account| {
            InterfaceAccount::<TokenAccount>::try_from(account)
                .ok()
                .filter(|token_account| {
                    token_account.mint == context.accounts.mint_account.address()
                })
                .map(|_| account.cpi_handle_mut())
        })
        .collect::<Vec<_>>();

    harvest_withheld_tokens_to_mint(
        CpiContext::new(
            context.accounts.token_program.address(),
            HarvestWithheldTokensToMint {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        sources, // token accounts to harvest from
    )?;
    Ok(())
}
