use {
    crate::{
        constants::{BASIS_POINTS_DENOMINATOR, MAX_LEVERAGE_CEILING},
        instructions::shared::{err, error},
        state::{Pool, PoolInner},
        LpMintPda, PoolAuthorityPda, VaultPda,
    },
    quasar_lang::{prelude::*, sysvars::clock::Clock},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct InitializePool {
    #[account(mut)]
    pub authority: Signer,
    #[account(
        mut,
        init,
        payer = authority,
        address = Pool::seeds(collateral_mint.address(), oracle_feed.address()),
    )]
    pub pool: Account<Pool>,
    pub collateral_mint: Account<Mint>,
    /// CHECK: stored on the pool; every read validates layout, scale, freshness.
    pub oracle_feed: UncheckedAccount,
    /// Authority PDA over the vault and liquidity-provider mint.
    #[account(address = PoolAuthorityPda::seeds(pool.address()))]
    pub pool_authority: UncheckedAccount,
    #[account(
        mut,
        init,
        payer = authority,
        address = LpMintPda::seeds(pool.address()),
        mint(decimals = 6, authority = pool_authority, freeze_authority = None, token_program = token_program),
    )]
    pub lp_mint: Account<Mint>,
    #[account(
        mut,
        init(idempotent),
        payer = authority,
        address = VaultPda::seeds(pool.address()),
        token(mint = collateral_mint, authority = pool_authority, token_program = token_program),
    )]
    pub custody_vault: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub clock: Sysvar<Clock>,
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub fn handle_initialize_pool(
    accounts: &mut InitializePool,
    oracle_scale: u32,
    funding_rate_per_slot: u64,
    open_fee_bps: u16,
    close_fee_bps: u16,
    max_leverage: u16,
    maintenance_margin_bps: u16,
    liquidation_fee_bps: u16,
    max_confidence_bps: u16,
    insurance_fee_bps: u16,
    profit_warmup_slots: u64,
    bumps: &InitializePoolBumps,
) -> Result<(), ProgramError> {
    let denominator = BASIS_POINTS_DENOMINATOR as u16;
    if !(1..=MAX_LEVERAGE_CEILING).contains(&max_leverage) {
        return Err(err(error::INVALID_PARAMETER));
    }
    if open_fee_bps >= denominator
        || close_fee_bps >= denominator
        || liquidation_fee_bps >= denominator
    {
        return Err(err(error::INVALID_PARAMETER));
    }
    if maintenance_margin_bps == 0 || maintenance_margin_bps >= denominator {
        return Err(err(error::INVALID_PARAMETER));
    }
    // close_position deducts the close fee from equity and refuses a
    // non-positive payout, while liquidation only acts at or below the
    // maintenance margin. The margin must therefore exceed the close fee, or a
    // position could be stranded in between: too healthy to liquidate, too poor
    // to pay the fee to close.
    if maintenance_margin_bps <= close_fee_bps {
        return Err(err(error::INVALID_PARAMETER));
    }
    if max_confidence_bps == 0 || max_confidence_bps >= denominator {
        return Err(err(error::INVALID_PARAMETER));
    }
    // The insurance cut is a fraction of the fee, so it cannot exceed the whole
    // fee; `denominator` (100%) routes every fee to insurance.
    if insurance_fee_bps > denominator {
        return Err(err(error::INVALID_PARAMETER));
    }

    let slot = accounts.clock.slot.get();
    accounts.pool.set_inner(PoolInner {
        authority: *accounts.authority.address(),
        collateral_mint: *accounts.collateral_mint.address(),
        oracle_feed: *accounts.oracle_feed.address(),
        custody_vault: *accounts.custody_vault.address(),
        lp_mint: *accounts.lp_mint.address(),
        oracle_scale,
        liquidity: 0,
        insurance_fund: 0,
        total_collateral: 0,
        protocol_fees: 0,
        long_size: 0,
        short_size: 0,
        long_size_scaled: 0,
        short_size_scaled: 0,
        cumulative_funding: 0,
        last_funding_slot: slot,
        funding_rate_per_slot,
        open_fee_bps,
        close_fee_bps,
        max_leverage,
        maintenance_margin_bps,
        liquidation_fee_bps,
        max_confidence_bps,
        insurance_fee_bps,
        profit_warmup_slots,
        bump: bumps.pool,
        authority_bump: bumps.pool_authority,
    });
    Ok(())
}
