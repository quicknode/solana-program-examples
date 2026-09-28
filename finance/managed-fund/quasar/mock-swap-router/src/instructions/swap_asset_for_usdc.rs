use quasar_lang::cpi::Seed;
use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::RouterError;
use crate::state::{AssetRate, RouterConfig, TreasuryPda, ROUTER_CONFIG_SEED};

#[derive(Accounts)]
pub struct SwapAssetForUsdcAccountConstraints {
    pub caller: Signer,

    // Owns the USDC treasury and is the mint authority of every asset the
    // router mints; signs the treasury transfer below with its own seeds.
    #[account(address = RouterConfig::seeds())]
    pub router_config: Account<RouterConfig>,

    pub asset_rate: Account<AssetRate>,

    pub usdc_mint: Account<Mint>,

    #[account(mut)]
    pub asset_mint: Account<Mint>,

    #[account(mut)]
    pub caller_asset_account: Account<Token>,

    #[account(mut)]
    pub caller_usdc_account: Account<Token>,

    #[account(mut, address = TreasuryPda::seeds())]
    pub router_usdc_treasury: InterfaceAccount<Token>,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_swap_asset_for_usdc(
    accounts: &mut SwapAssetForUsdcAccountConstraints,
    asset_amount_in: u64,
    minimum_usdc_out: u64,
) -> Result<(), ProgramError> {
    require_keys_eq!(
        accounts.asset_rate.mint,
        *accounts.asset_mint.address(),
        RouterError::InvalidAssetMint
    );
    require_keys_eq!(
        *accounts.usdc_mint.address(),
        accounts.router_config.usdc_mint,
        RouterError::WrongUsdcMint
    );

    let rate = u64::from(accounts.asset_rate.usdc_per_token);
    require!(rate > 0, RouterError::ZeroRate);
    require!(asset_amount_in > 0, RouterError::ZeroAmount);

    // usdc_out = asset_amount_in * rate.
    let usdc_out: u64 = (asset_amount_in as u128)
        .checked_mul(rate as u128)
        .ok_or(RouterError::MathOverflow)?
        .try_into()
        .map_err(|_| RouterError::MathOverflow)?;
    require!(usdc_out >= minimum_usdc_out, RouterError::SlippageExceeded);

    // Burn the caller's asset tokens (caller signs).
    accounts
        .token_program
        .burn(
            &accounts.caller_asset_account,
            &accounts.asset_mint,
            &accounts.caller,
            asset_amount_in,
        )
        .invoke()?;

    // Pay USDC from the treasury to the caller; the router config account owns
    // the treasury and signs.
    let bump = [accounts.router_config.bump];
    let seeds = [Seed::from(ROUTER_CONFIG_SEED), Seed::from(bump.as_ref())];
    accounts
        .token_program
        .transfer_checked(
            &accounts.router_usdc_treasury,
            &accounts.usdc_mint,
            &accounts.caller_usdc_account,
            &accounts.router_config,
            usdc_out,
            accounts.usdc_mint.decimals,
        )
        .invoke_signed(&seeds)?;

    Ok(())
}
