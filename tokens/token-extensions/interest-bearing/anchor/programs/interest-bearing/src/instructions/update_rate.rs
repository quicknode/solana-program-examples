use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    interest_bearing_mint_update_rate, InterestBearingMintUpdateRate, Mint, Token2022,
};

use crate::check_mint_data;

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

    check_mint_data(
        context.accounts.mint_account.account(),
        &context.accounts.authority.address(),
    )?;
    Ok(())
}
