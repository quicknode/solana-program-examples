use {
    crate::{
        constants::{BASIS_POINTS_DENOMINATOR, MAX_FUNDING_RATE_PER_SECOND},
        instructions::shared::{err, error, read_feed_price},
        state::{Pool, PoolInner},
        LpMintPda, VaultPda,
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
    /// CHECK: stored on the pool; every read, including the one here that seeds
    /// the average price, validates layout, scale, freshness.
    pub oracle_feed: UncheckedAccount,
    /// Liquidity-provider share mint; the pool account is its mint authority.
    #[account(
        mut,
        init,
        payer = authority,
        address = LpMintPda::seeds(pool.address()),
        mint(decimals = 6, authority = pool, freeze_authority = None, token_program = token_program),
    )]
    pub lp_mint: Account<Mint>,
    /// Custody vault for all collateral; the pool account owns it.
    #[account(
        mut,
        init(idempotent),
        payer = authority,
        address = VaultPda::seeds(pool.address()),
        token(mint = collateral_mint, authority = pool, token_program = token_program),
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
    funding_rate_per_second: u64,
    open_fee_bps: u16,
    close_fee_bps: u16,
    initial_margin_bps: u16,
    maintenance_margin_bps: u16,
    liquidation_fee_bps: u16,
    max_confidence_bps: u16,
    max_price_deviation_bps: u16,
    bumps: &InitializePoolBumps,
) -> Result<(), ProgramError> {
    let denominator = BASIS_POINTS_DENOMINATOR as u16;
    // The rate never changes after this, so bounding it here bounds it for the
    // life of the pool.
    if funding_rate_per_second > MAX_FUNDING_RATE_PER_SECOND {
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
    // A position must open with more margin than it is liquidated at, or it
    // could be liquidated in the same slot it opened. At most 100% of
    // notional: more than that would demand collateral above the position's
    // size.
    if initial_margin_bps <= maintenance_margin_bps {
        return Err(err(error::INITIAL_MARGIN_NOT_ABOVE_MAINTENANCE));
    }
    if initial_margin_bps > denominator {
        return Err(err(error::INVALID_PARAMETER));
    }
    if max_confidence_bps == 0 || max_confidence_bps >= denominator {
        return Err(err(error::INVALID_PARAMETER));
    }
    // Zero would refuse every price move, however small. At 100% or more the
    // band could never refuse a fall, since the oracle price is always
    // positive.
    if max_price_deviation_bps == 0 || max_price_deviation_bps >= denominator {
        return Err(err(error::INVALID_PRICE_DEVIATION));
    }

    // Seed the average with a validated oracle price, so the band is in force
    // from the first trade.
    let initial_price = read_feed_price(
        &accounts.oracle_feed,
        oracle_scale,
        accounts.clock.slot.get(),
        max_confidence_bps,
    )?;
    let unix_timestamp = accounts.clock.unix_timestamp.get();
    accounts.pool.set_inner(PoolInner {
        authority: *accounts.authority.address(),
        collateral_mint: *accounts.collateral_mint.address(),
        oracle_feed: *accounts.oracle_feed.address(),
        custody_vault: *accounts.custody_vault.address(),
        lp_mint: *accounts.lp_mint.address(),
        oracle_scale,
        liquidity: 0,
        reserved_liquidity: 0,
        total_collateral: 0,
        program_fees: 0,
        long_size: 0,
        short_size: 0,
        long_size_scaled: 0,
        short_size_scaled: 0,
        cumulative_funding: 0,
        last_funding_timestamp: unix_timestamp,
        average_price: initial_price,
        last_oracle_price: initial_price,
        average_price_timestamp: unix_timestamp,
        funding_rate_per_second,
        open_fee_bps,
        close_fee_bps,
        initial_margin_bps,
        maintenance_margin_bps,
        liquidation_fee_bps,
        max_confidence_bps,
        max_price_deviation_bps,
        bump: bumps.pool,
    });
    Ok(())
}
