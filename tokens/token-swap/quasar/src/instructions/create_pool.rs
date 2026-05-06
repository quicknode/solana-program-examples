use {
    crate::{
        state::{Amm, Pool, PoolInner},
        AmmPda, LiquidityMintPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for creating a new liquidity pool.
///
/// Seeds are based on account addresses: pool = [amm, mint_a, mint_b],
/// pool_authority = [b"authority", amm, mint_a, mint_b],
/// mint_liquidity = [b"liquidity", amm, mint_a, mint_b].
///
/// Note: post-PR-#195 the seed prefix is always emitted first by
/// `#[derive(Seeds)]`, so pool_authority/mint_liquidity now derive with
/// the literal prefix in front (different on-chain addresses than the
/// Anchor sibling, but internally consistent within this program).
#[derive(Accounts)]
pub struct CreatePool {
    #[account(address = AmmPda::seeds())]
    pub amm: Account<Amm>,
    #[account(
        mut,
        init,
        payer = payer,
        address = PoolPda::seeds(amm.address(), mint_a.address(), mint_b.address()),
    )]
    pub pool: Account<Pool>,
    /// Pool authority PDA — signs for pool token operations.
    #[account(
        address = PoolAuthorityPda::seeds(amm.address(), mint_a.address(), mint_b.address()),
    )]
    pub pool_authority: UncheckedAccount,
    /// Liquidity token mint — created at a PDA.
    #[account(
        mut,
        init,
        payer = payer,
        address = LiquidityMintPda::seeds(amm.address(), mint_a.address(), mint_b.address()),
        mint(decimals = 6, authority = pool_authority, freeze_authority = None, token_program = token_program),
    )]
    pub mint_liquidity: Account<Mint>,
    pub mint_a: Account<Mint>,
    pub mint_b: Account<Mint>,
    /// Pool's token A account.
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_a, authority = pool_authority, token_program = token_program),
    )]
    pub pool_account_a: Account<Token>,
    /// Pool's token B account.
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_b, authority = pool_authority, token_program = token_program),
    )]
    pub pool_account_b: Account<Token>,
    #[account(mut)]
    pub payer: Signer,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
pub fn handle_create_pool(accounts: &mut CreatePool) -> Result<(), ProgramError> {
    accounts.pool.set_inner(PoolInner {
        amm: *accounts.amm.address(),
        mint_a: *accounts.mint_a.address(),
        mint_b: *accounts.mint_b.address(),
    });
    Ok(())
}
