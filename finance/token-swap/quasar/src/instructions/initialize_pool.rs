use {
    crate::{
        error::AmmError,
        state::{Config, PoolConfig, PoolConfigInner},
        ConfigPda, LiquidityMintPda, PoolAPda, PoolBPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Seeds:
/// - `pool_config = [config, mint_a, mint_b]`
/// - `liquidity_provider_mint = [b"liquidity", config, mint_a, mint_b]`
/// - `pool_a = [b"pool_a", pool_config]`, `pool_b = [b"pool_b", pool_config]`
///
/// `pool_config` owns both reserves and is the LP mint's mint authority; it
/// signs for them with its own seeds. `liquidity_provider_mint` derives at a
/// different onchain address than the Anchor sibling because
/// `#[derive(Seeds)]` emits the literal prefix first. Internally consistent
/// within this program.
#[derive(Accounts)]
pub struct InitializePoolAccountConstraints {
    #[account(address = ConfigPda::seeds())]
    pub config: Account<Config>,
    #[account(
        mut,
        init,
        payer = payer,
        address = PoolPda::seeds(config.address(), mint_a.address(), mint_b.address()),
    )]
    pub pool_config: Account<PoolConfig>,
    /// Liquidity token mint - created at a PDA; `pool_config` is its mint
    /// authority.
    #[account(
        mut,
        init,
        payer = payer,
        address = LiquidityMintPda::seeds(config.address(), mint_a.address(), mint_b.address()),
        mint(decimals = 6, authority = pool_config, freeze_authority = None, token_program = token_program),
    )]
    pub liquidity_provider_mint: Account<Mint>,
    pub mint_a: Account<Mint>,
    /// `mint_a` must sort strictly below `mint_b`, so each pair of mints has
    /// exactly one pool. Without it an (X, Y) pool and a (Y, X) pool would
    /// both be valid and split the pair's liquidity between them.
    #[account(
        constraints(mint_a.address().as_ref() < mint_b.address().as_ref()) @ AmmError::InvalidMintOrder,
    )]
    pub mint_b: Account<Mint>,
    /// Pool's token A reserve, owned by `pool_config`, at a PDA of the pool.
    #[account(
        mut,
        init,
        payer = payer,
        address = PoolAPda::seeds(pool_config.address()),
        token(mint = mint_a, authority = pool_config, token_program = token_program),
    )]
    pub pool_a: Account<Token>,
    /// Pool's token B reserve, owned by `pool_config`, at a PDA of the pool.
    #[account(
        mut,
        init,
        payer = payer,
        address = PoolBPda::seeds(pool_config.address()),
        token(mint = mint_b, authority = pool_config, token_program = token_program),
    )]
    pub pool_b: Account<Token>,
    #[account(mut)]
    pub payer: Signer,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
pub fn handle_initialize_pool(
    accounts: &mut InitializePoolAccountConstraints,
) -> Result<(), ProgramError> {
    accounts.pool_config.set_inner(PoolConfigInner {
        config: *accounts.config.address(),
        mint_a: *accounts.mint_a.address(),
        mint_b: *accounts.mint_b.address(),
        // Recorded so every later handler can check the reserves it is
        // handed are these two (`has_one(pool_a)`, `has_one(pool_b)`).
        pool_a: *accounts.pool_a.address(),
        pool_b: *accounts.pool_b.address(),
        // No swaps have happened yet, so the admin has no fee claim. These
        // accumulators are written by `swap_tokens` and zeroed by
        // `claim_admin_fees`.
        admin_fees_owed_a: 0,
        admin_fees_owed_b: 0,
    });
    Ok(())
}
