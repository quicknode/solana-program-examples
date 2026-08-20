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
    // `remaining_accounts()` takes `&mut context` and hands back an owned vec,
    // so collect it before anything borrows `context.accounts`.
    let mut candidates = context.remaining_accounts()?;
    let mint_address = *context.accounts.mint_account.address();

    // v2 has no `InterfaceAccount::try_from`; `AnchorAccount::load` is the
    // equivalent for an account reached through remaining_accounts. The
    // wrapper is dropped at the end of each iteration, so it holds no borrow
    // across the CPI: only the verdict escapes.
    let keep: Vec<bool> = candidates
        .iter()
        .map(|account| {
            InterfaceAccount::<TokenAccount>::load(*account)
                .map(|token_account| *token_account.mint() == mint_address)
                .unwrap_or(false)
        })
        .collect();

    let sources: Vec<CpiHandleMut> = candidates
        .iter_mut()
        .zip(keep)
        .filter(|(_, keep)| *keep)
        .map(|(account, _)| CpiHandleMut::writable(account))
        .collect();

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
