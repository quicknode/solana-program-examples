use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::interest_bearing_mint::InterestBearingConfig;
use anchor_spl::token_interface::{
    get_mint_extension_data, interest_bearing_mint_update_rate, InterestBearingMintUpdateRate, Mint,
    Token2022,
};

use crate::check_rate_authority;

#[derive(Accounts)]
pub struct UpdateRateAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub mint_account: InterfaceAccount<'info, Mint>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn handler(context: Context<UpdateRateAccountConstraints>, rate: i16) -> Result<()> {
    interest_bearing_mint_update_rate(
        CpiContext::new(
            context.accounts.token_program.key(),
            InterestBearingMintUpdateRate {
                token_program_id: context.accounts.token_program.to_account_info(),
                mint: context.accounts.mint_account.to_account_info(),
                rate_authority: context.accounts.authority.to_account_info(),
            },
        ),
        rate,
    )?;

    let config = get_mint_extension_data::<InterestBearingConfig>(
        &context.accounts.mint_account.to_account_info(),
    )?;
    check_rate_authority(&config, &context.accounts.authority.key())?;
    Ok(())
}
