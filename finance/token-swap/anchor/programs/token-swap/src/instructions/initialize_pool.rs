use anchor_lang::prelude::*;
use anchor_spl::mint;
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
    context: &mut Context<InitializePoolAccountConstraints>,
    amount_a: u64,
    amount_b: u64,
) -> Result<()> {
    require!(amount_a > 0 && amount_b > 0, AmmError::EmptyInitialDeposit);
    if amount_a > context.accounts.creator_token_a.amount()
        || amount_b > context.accounts.creator_token_b.amount()
    {
        return err!(AmmError::InsufficientBalance);
    }
    let lp_amount = initial_lp_amount(amount_a, amount_b)?;

    let bump = context.bumps.pool_config;
    let pool_config = &mut context.accounts.pool_config;
    pool_config.config = *context.accounts.config.address();
    pool_config.mint_a = *context.accounts.mint_a.address();
    pool_config.mint_b = *context.accounts.mint_b.address();
    pool_config.bump = bump;

    deposit_and_mint_lp_tokens(
        LiquidityDepositAccounts {
            token_program: &context.accounts.token_program,
            pool_config: &mut context.accounts.pool_config,
            mint_a: &context.accounts.mint_a,
            mint_b: &context.accounts.mint_b,
            pool_a: &mut context.accounts.pool_a,
            pool_b: &mut context.accounts.pool_b,
            depositor: &context.accounts.creator,
            depositor_token_a: &mut context.accounts.creator_token_a,
            depositor_token_b: &mut context.accounts.creator_token_b,
            liquidity_provider_mint: &mut context.accounts.liquidity_provider_mint,
            liquidity_provider_token: &mut context.accounts.liquidity_provider_token,
        },
        amount_a,
        amount_b,
        lp_amount,
    )?;

    // `pool_config` was created here, so it is loaded mutably and the derive
    // writes it back when the handler returns: take the borrow the mint
    // released back before then.
    context.accounts.pool_config.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializePoolAccountConstraints {
    #[account(
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Box<BorshAccount<Config>>,

    #[account(
        init,
        payer = payer,
        space = PoolConfig::DISCRIMINATOR.len() + PoolConfig::INIT_SPACE,
        seeds = [
            config.address().as_ref(),
            mint_a.address().as_ref(),
            mint_b.address().as_ref(),
        ],
        bump,
        constraint = mint_a.address() < mint_b.address() @ AmmError::InvalidMintOrder,
    )]
    pub pool_config: Box<BorshAccount<PoolConfig>>,

    /// The LP mint. `pool_config` is its mint authority and signs every
    /// mint_to with its own seeds.
    #[account(
        init,
        payer = payer,
        seeds = [
            config.address().as_ref(),
            mint_a.address().as_ref(),
            mint_b.address().as_ref(),
            LIQUIDITY_SEED,
        ],
        bump,
        mint::decimals = 6,
        mint::authority = pool_config,
        // Required when the token program is an `Interface`: without it the
        // init CPI is rejected with InvalidArgument.
        mint::token_program = token_program,
    )]
    pub liquidity_provider_mint: Box<InterfaceAccount<Mint>>,

    pub mint_a: Box<InterfaceAccount<Mint>>,

    pub mint_b: Box<InterfaceAccount<Mint>>,

    /// The pool's token-A reserve: the associated token account of
    /// `pool_config`, which signs every transfer out of it.
    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = pool_config,
        associated_token::token_program = token_program,
    )]
    pub pool_a: Box<InterfaceAccount<TokenAccount>>,

    /// The pool's token-B reserve, likewise owned by `pool_config`.
    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = pool_config,
        associated_token::token_program = token_program,
    )]
    pub pool_b: Box<InterfaceAccount<TokenAccount>>,

    /// Makes the first deposit and receives the first LP tokens.
    pub creator: Signer,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = creator,
        associated_token::token_program = token_program,
    )]
    pub creator_token_a: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = creator,
        associated_token::token_program = token_program,
    )]
    pub creator_token_b: Box<InterfaceAccount<TokenAccount>>,

    /// The creator's LP token account, created here because the LP mint did
    /// not exist before this instruction.
    #[account(
        init,
        payer = payer,
        associated_token::mint = liquidity_provider_mint,
        associated_token::authority = creator,
        associated_token::token_program = token_program,
    )]
    pub liquidity_provider_token: Box<InterfaceAccount<TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer,

    /// Solana ecosystem accounts
    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
