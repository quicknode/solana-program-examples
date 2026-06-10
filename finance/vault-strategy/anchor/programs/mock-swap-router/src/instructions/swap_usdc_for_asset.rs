use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::error::RouterError;
use crate::state::{AssetRate, RouterConfig};

#[derive(Accounts)]
pub struct SwapUsdcForAssetAccountConstraints<'info> {
    /// The caller - e.g. the vault strategy PDA (can be a signer or a PDA signer via CPI)
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

    pub usdc_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub asset_mint: InterfaceAccount<'info, Mint>,

    /// Caller's USDC token account - USDC flows from here to the treasury
    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = caller,
        associated_token::token_program = token_program
    )]
    pub caller_usdc_account: InterfaceAccount<'info, TokenAccount>,

    /// Caller's asset token account - minted asset tokens land here
    #[account(
        mut,
        associated_token::mint = asset_mint,
        associated_token::authority = caller,
        associated_token::token_program = token_program
    )]
    pub caller_asset_account: InterfaceAccount<'info, TokenAccount>,

    /// Router's USDC treasury - receives the USDC payment
    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = router_authority,
        associated_token::token_program = token_program
    )]
    pub router_usdc_treasury: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: PDA used as mint authority - validated by seeds constraint
    #[account(
        seeds = [b"router_authority"],
        bump
    )]
    pub router_authority: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_swap_usdc_for_asset(
    context: Context<SwapUsdcForAssetAccountConstraints>,
    usdc_amount_in: u64,
    minimum_asset_out: u64,
) -> Result<()> {
    let rate = context.accounts.asset_rate.usdc_per_token;
    require!(rate > 0, RouterError::ZeroRate);

    // asset_out = usdc_amount_in / rate  (u128 intermediate, user gets floor)
    let asset_out: u64 = (usdc_amount_in as u128)
        .checked_mul(1u128)
        .ok_or(RouterError::MathOverflow)?
        .checked_div(rate as u128)
        .ok_or(RouterError::MathOverflow)? as u64;

    require!(
        asset_out >= minimum_asset_out,
        RouterError::SlippageExceeded
    );

    // Transfer USDC from caller to router treasury
    let transfer_accounts = TransferChecked {
        from: context.accounts.caller_usdc_account.to_account_info(),
        mint: context.accounts.usdc_mint.to_account_info(),
        to: context.accounts.router_usdc_treasury.to_account_info(),
        authority: context.accounts.caller.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(context.accounts.token_program.key(), transfer_accounts);
    transfer_checked(cpi_ctx, usdc_amount_in, context.accounts.usdc_mint.decimals)?;

    // Mint asset tokens to caller - router_authority PDA signs
    let router_authority_bump = context.bumps.router_authority;
    let signer_seeds: &[&[&[u8]]] = &[&[b"router_authority", &[router_authority_bump]]];

    let mint_accounts = MintTo {
        mint: context.accounts.asset_mint.to_account_info(),
        to: context.accounts.caller_asset_account.to_account_info(),
        authority: context.accounts.router_authority.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        mint_accounts,
        signer_seeds,
    );
    mint_to(cpi_ctx, asset_out)?;

    Ok(())
}
