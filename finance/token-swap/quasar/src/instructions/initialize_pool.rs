use {
    crate::{
        error::AmmError,
        liquidity::{deposit_and_mint_lp_tokens, initial_lp_amount, LiquidityDepositAccounts},
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
    /// Makes the first deposit and receives the first LP tokens.
    pub creator: Signer,
    /// The creator's token A account, the source of the first deposit.
    #[account(mut)]
    pub creator_token_a: Account<Token>,
    /// The creator's token B account, the source of the first deposit.
    #[account(mut)]
    pub creator_token_b: Account<Token>,
    /// The creator's LP token account, created here because the LP mint did
    /// not exist before this instruction.
    #[account(
        mut,
        init,
        payer = payer,
        token(mint = liquidity_provider_mint, authority = creator, token_program = token_program),
    )]
    pub liquidity_provider_token: Account<Token>,
    #[account(mut)]
    pub payer: Signer,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub rent: Sysvar<Rent>,
}

/// Creates the pool and takes the creator's first deposit in the same
/// instruction. The first deposit sets the pool's price (the ratio of its
/// reserves), so a pool that existed empty between two transactions would let
/// whoever deposited first set the price for its creator. Taking the deposit
/// here means every pool has both reserves positive from the moment it
/// exists, and `deposit_liquidity` only ever clamps to a price the creator
/// chose.
#[inline(always)]
pub fn handle_initialize_pool(
    accounts: &mut InitializePoolAccountConstraints,
    amount_a: u64,
    amount_b: u64,
    bumps: &InitializePoolAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require!(amount_a > 0 && amount_b > 0, AmmError::EmptyInitialDeposit);
    // Fail fast if the creator lacks the balance, as `deposit_liquidity` does.
    if amount_a > accounts.creator_token_a.amount() || amount_b > accounts.creator_token_b.amount()
    {
        return Err(AmmError::InsufficientBalance.into());
    }
    let lp_amount = initial_lp_amount(amount_a, amount_b)?;

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

    deposit_and_mint_lp_tokens(
        LiquidityDepositAccounts {
            token_program: &accounts.token_program,
            pool_config: &accounts.pool_config,
            pool_config_bump: bumps.pool_config,
            mint_a: &accounts.mint_a,
            mint_b: &accounts.mint_b,
            pool_a: &accounts.pool_a,
            pool_b: &accounts.pool_b,
            depositor: &accounts.creator,
            depositor_token_a: &accounts.creator_token_a,
            depositor_token_b: &accounts.creator_token_b,
            liquidity_provider_mint: &accounts.liquidity_provider_mint,
            liquidity_provider_token: &accounts.liquidity_provider_token,
        },
        amount_a,
        amount_b,
        lp_amount,
    )
}
