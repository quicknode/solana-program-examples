use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::state::{
    AssetRate, AssetRateInner, RouterAuthorityPda, RouterConfig, TreasuryPda,
};

#[derive(Accounts)]
pub struct SetRateAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(address = RouterConfig::seeds(), has_one(authority))]
    pub router_config: Account<RouterConfig>,

    pub asset_mint: Account<Mint>,
    pub usdc_mint: Account<Mint>,

    #[account(
        init(idempotent),
        payer = authority,
        address = AssetRate::seeds(asset_mint.address()),
    )]
    pub asset_rate: Account<AssetRate>,

    #[account(address = RouterAuthorityPda::seeds())]
    pub router_authority: UncheckedAccount,

    #[account(
        init(idempotent),
        payer = authority,
        token(mint = usdc_mint, authority = router_authority, token_program = token_program),
        address = TreasuryPda::seeds(),
    )]
    pub router_usdc_treasury: InterfaceAccount<Token>,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_set_rate(
    accounts: &mut SetRateAccountConstraints,
    usdc_per_token: u64,
    bumps: &SetRateAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    accounts.asset_rate.set_inner(AssetRateInner {
        mint: *accounts.asset_mint.address(),
        usdc_per_token,
        bump: bumps.asset_rate,
    });
    Ok(())
}
