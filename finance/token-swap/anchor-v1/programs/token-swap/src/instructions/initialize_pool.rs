use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{CONFIG_SEED, LIQUIDITY_SEED},
    errors::AmmError,
    liquidity::{deposit_and_mint_lp_tokens, initial_lp_amount, LiquidityDepositAccounts},
    state::{Config, PoolConfig},
};

/// Creates the pool and takes the creator's first deposit in the same
/// instruction. The first deposit sets the pool's price (the ratio of its
/// reserves), so a pool that existed empty between two transactions would let
/// whoever deposited first set the price for its creator. Taking the deposit
/// here means every pool has both reserves positive from the moment it
/// exists, and `deposit_liquidity` only ever clamps to a price the creator
/// chose.
pub fn handle_initialize_pool(
    context: Context<InitializePoolAccountConstraints>,
    amount_a: u64,
    amount_b: u64,
) -> Result<()> {
    require!(amount_a > 0 && amount_b > 0, AmmError::EmptyInitialDeposit);
    if amount_a > context.accounts.creator_token_a.amount
        || amount_b > context.accounts.creator_token_b.amount
    {
        return err!(AmmError::InsufficientBalance);
    }
    let lp_amount = initial_lp_amount(amount_a, amount_b)?;

    let bump = context.bumps.pool_config;
    let pool_config = &mut context.accounts.pool_config;
    pool_config.config = context.accounts.config.key();
    pool_config.mint_a = context.accounts.mint_a.key();
    pool_config.mint_b = context.accounts.mint_b.key();
    pool_config.bump = bump;

    deposit_and_mint_lp_tokens(
        LiquidityDepositAccounts {
            token_program: &context.accounts.token_program,
            pool_config: &context.accounts.pool_config,
            mint_a: &context.accounts.mint_a,
            mint_b: &context.accounts.mint_b,
            pool_a: &context.accounts.pool_a,
            pool_b: &context.accounts.pool_b,
            depositor: &context.accounts.creator,
            depositor_token_a: &context.accounts.creator_token_a,
            depositor_token_b: &context.accounts.creator_token_b,
            liquidity_provider_mint: &context.accounts.liquidity_provider_mint,
            liquidity_provider_token: &context.accounts.liquidity_provider_token,
        },
        amount_a,
        amount_b,
        lp_amount,
    )
}

#[derive(Accounts)]
pub struct InitializePoolAccountConstraints<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        init,
        payer = payer,
        space = PoolConfig::DISCRIMINATOR.len() + PoolConfig::INIT_SPACE,
        seeds = [
            config.key().as_ref(),
            mint_a.key().as_ref(),
            mint_b.key().as_ref(),
        ],
        bump,
        constraint = mint_a.key() < mint_b.key() @ AmmError::InvalidMintOrder,
    )]
    pub pool_config: Box<Account<'info, PoolConfig>>,

    /// The LP mint. `pool_config` is its mint authority and signs every
    /// mint_to with its own seeds.
    #[account(
        init,
        payer = payer,
        seeds = [
            config.key().as_ref(),
            mint_a.key().as_ref(),
            mint_b.key().as_ref(),
            LIQUIDITY_SEED,
        ],
        bump,
        mint::decimals = 6,
        mint::authority = pool_config,
    )]
    pub liquidity_provider_mint: Box<InterfaceAccount<'info, Mint>>,

    pub mint_a: Box<InterfaceAccount<'info, Mint>>,

    pub mint_b: Box<InterfaceAccount<'info, Mint>>,

    /// The pool's token-A reserve: the associated token account of
    /// `pool_config`, which signs every transfer out of it.
    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = pool_config,
        associated_token::token_program = token_program,
    )]
    pub pool_a: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The pool's token-B reserve, likewise owned by `pool_config`.
    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = pool_config,
        associated_token::token_program = token_program,
    )]
    pub pool_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Makes the first deposit and receives the first LP tokens.
    pub creator: Signer<'info>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = creator,
        associated_token::token_program = token_program,
    )]
    pub creator_token_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = creator,
        associated_token::token_program = token_program,
    )]
    pub creator_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The creator's LP token account, created here because the LP mint did
    /// not exist before this instruction.
    #[account(
        init,
        payer = payer,
        associated_token::mint = liquidity_provider_mint,
        associated_token::authority = creator,
        associated_token::token_program = token_program,
    )]
    pub liquidity_provider_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Solana ecosystem accounts
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
