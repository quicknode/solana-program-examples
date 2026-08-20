use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    interest_bearing_mint_update_rate, InterestBearingMintUpdateRate, Mint, Token2022,
};

use anchor_spl::token_interface::TokenInterfaceAccountExtensions;

use crate::check_rate_authority;

#[derive(Accounts)]
pub struct UpdateRateAccountConstraints {
    #[account(mut)]
    pub authority: Signer,
    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,

    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

pub fn handler(context: &mut Context<UpdateRateAccountConstraints>, rate: i16) -> Result<()> {
    interest_bearing_mint_update_rate(
        CpiContext::new(
            context.accounts.token_program.address(),
            InterestBearingMintUpdateRate {
                mint: context.accounts.mint_account.cpi_handle_mut(),
                rate_authority: context.accounts.authority.cpi_handle(),
            },
        ),
        rate,
    )?;

    // `mint_account` is an `InterfaceAccount<Mint>` declared `mut`, so it holds
    // the account's exclusive borrow and the program cannot take a second one.
    // anchor-spl's accessor parses the TLV through that same borrow, and checks
    // the mint is owned by Token-2022 on the way.
    let authority_address = *context.accounts.authority.address();
    check_rate_authority(
        context.accounts.mint_account.get_extension()?,
        &authority_address,
    )?;
    Ok(())
}
