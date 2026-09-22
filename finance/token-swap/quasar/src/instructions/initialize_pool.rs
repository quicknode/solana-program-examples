use {
    crate::{
        state::{Config, PoolConfig, PoolConfigInner},
        ConfigPda, LiquidityMintPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Seeds:
/// - `pool_config = [config, mint_a, mint_b]`
/// - `liquidity_provider_mint = [b"liquidity", config, mint_a, mint_b]`
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
    pub mint_b: Account<Mint>,
    /// Pool's token A reserve, owned by `pool_config`.
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_a, authority = pool_config, token_program = token_program),
    )]
    pub pool_a: Account<Token>,
    /// Pool's token B reserve, owned by `pool_config`.
    #[account(
        mut,
        init(idempotent),
        payer = payer,
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
        // No swaps have happened yet, so the admin has no fee claim. These
        // accumulators are written by `swap_tokens` and zeroed by
        // `claim_admin_fees`.
        admin_fees_owed_a: 0,
        admin_fees_owed_b: 0,
    });
    Ok(())
}
