use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::RouterError;
use crate::state::{
    AssetRate, RouterAuthorityPda, RouterConfig, TreasuryPda, ROUTER_AUTHORITY_SEED,
};

#[derive(Accounts)]
pub struct SwapUsdcForAssetAccountConstraints {
    // The caller - the vault-strategy PDA when invoked via CPI (a PDA signer).
    pub caller: Signer,

    #[account(address = RouterConfig::seeds())]
    pub router_config: Account<RouterConfig>,

    pub asset_rate: Account<AssetRate>,

    pub usdc_mint: Account<Mint>,

    #[account(mut)]
    pub asset_mint: Account<Mint>,

    #[account(mut)]
    pub caller_usdc_account: Account<Token>,

    #[account(mut)]
    pub caller_asset_account: Account<Token>,

    #[account(mut, address = TreasuryPda::seeds())]
    pub router_usdc_treasury: InterfaceAccount<Token>,

    #[account(address = RouterAuthorityPda::seeds())]
    pub router_authority: UncheckedAccount,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_swap_usdc_for_asset(
    accounts: &mut SwapUsdcForAssetAccountConstraints,
    usdc_amount_in: u64,
    minimum_asset_out: u64,
    bumps: &SwapUsdcForAssetAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require_keys_eq!(
        accounts.asset_rate.mint,
        *accounts.asset_mint.address(),
        RouterError::InvalidAssetMint
    );

    let rate = u64::from(accounts.asset_rate.usdc_per_token);
    require!(rate > 0, RouterError::ZeroRate);

    // asset_out = usdc_amount_in / rate (floor).
    let asset_out: u64 = (usdc_amount_in as u128)
        .checked_div(rate as u128)
        .ok_or(RouterError::MathOverflow)?
        .try_into()
        .map_err(|_| RouterError::MathOverflow)?;
    require!(
        asset_out >= minimum_asset_out,
        RouterError::SlippageExceeded
    );

    // USDC from caller to the router treasury (caller signs).
    accounts
        .token_program
        .transfer_checked(
            &accounts.caller_usdc_account,
            &accounts.usdc_mint,
            &accounts.router_usdc_treasury,
            &accounts.caller,
            usdc_amount_in,
            accounts.usdc_mint.decimals,
        )
        .invoke()?;

    // Mint asset tokens to the caller; the router-authority PDA is the mint
    // authority and signs.
    let bump = [bumps.router_authority];
    let seeds = [Seed::from(ROUTER_AUTHORITY_SEED), Seed::from(bump.as_ref())];
    accounts
        .token_program
        .mint_to(
            &accounts.asset_mint,
            &accounts.caller_asset_account,
            &accounts.router_authority,
            asset_out,
        )
        .invoke_signed(&seeds)?;

    Ok(())
}
