use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::error::RouterError;
use crate::state::{AssetRate, RouterConfig};

#[derive(Accounts)]
pub struct SwapAssetForUsdcAccountConstraints<'info> {
    pub caller: Signer<'info>,

    #[account(
        seeds = [b"router_config"],
        bump = router_config.bump
    )]
    pub router_config: Account<'info, RouterConfig>,

    #[account(
        constraint = asset_rate.mint == asset_mint.key() @ RouterError::InvalidAssetMint
    )]
    pub asset_rate: Account<'info, AssetRate>,

    #[account(constraint = usdc_mint.key() == router_config.usdc_mint @ RouterError::WrongUsdcMint)]
    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub asset_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Caller's asset token account - asset tokens are burned from here
    #[account(
        mut,
        associated_token::mint = asset_mint,
        associated_token::authority = caller,
        associated_token::token_program = token_program
    )]
    pub caller_asset_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Caller's USDC account - receives the USDC
    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = caller,
        associated_token::token_program = token_program
    )]
    pub caller_usdc_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Router's USDC treasury - sends the USDC
    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = router_authority,
        associated_token::token_program = token_program
    )]
    pub router_usdc_treasury: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: PDA used as treasury authority - validated by seeds constraint
    #[account(
        seeds = [b"router_authority"],
        bump
    )]
    pub router_authority: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_swap_asset_for_usdc(
    context: Context<SwapAssetForUsdcAccountConstraints>,
    asset_amount_in: u64,
    minimum_usdc_out: u64,
) -> Result<()> {
    let rate = context.accounts.asset_rate.usdc_per_token;
    require!(rate > 0, RouterError::ZeroRate);
    require!(asset_amount_in > 0, RouterError::ZeroAmount);

    // usdc_out = asset_amount_in * rate  (u128 intermediate, protocol gets ceil on sell)
    let usdc_out: u64 = (asset_amount_in as u128)
        .checked_mul(rate as u128)
        .ok_or(RouterError::MathOverflow)? as u64;

    require!(usdc_out >= minimum_usdc_out, RouterError::SlippageExceeded);

    // Burn asset tokens from caller
    let burn_accounts = Burn {
        mint: context.accounts.asset_mint.to_account_info(),
        from: context.accounts.caller_asset_account.to_account_info(),
        authority: context.accounts.caller.to_account_info(),
    };
    burn(
        CpiContext::new(context.accounts.token_program.key(), burn_accounts),
        asset_amount_in,
    )?;

    // Transfer USDC from router treasury to caller - router_authority PDA signs
    let router_authority_bump = context.bumps.router_authority;
    let signer_seeds: &[&[&[u8]]] = &[&[b"router_authority", &[router_authority_bump]]];

    let transfer_accounts = TransferChecked {
        from: context.accounts.router_usdc_treasury.to_account_info(),
        mint: context.accounts.usdc_mint.to_account_info(),
        to: context.accounts.caller_usdc_account.to_account_info(),
        authority: context.accounts.router_authority.to_account_info(),
    };
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            transfer_accounts,
            signer_seeds,
        ),
        usdc_out,
        context.accounts.usdc_mint.decimals,
    )?;

    Ok(())
}
