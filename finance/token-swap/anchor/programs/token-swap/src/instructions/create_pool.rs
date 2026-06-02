use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED, LIQUIDITY_SEED},
    errors::AmmError,
    state::{Config, PoolConfig},
};

pub fn handle_create_pool(context: Context<CreatePoolAccountConstraints>) -> Result<()> {
    let bump = context.bumps.pool_config;
    let pool_config = &mut context.accounts.pool_config;
    pool_config.config = context.accounts.config.key();
    pool_config.mint_a = context.accounts.mint_a.key();
    pool_config.mint_b = context.accounts.mint_b.key();
    pool_config.bump = bump;

    Ok(())
}

#[derive(Accounts)]
pub struct CreatePoolAccountConstraints<'info> {
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

    /// CHECK: Read only authority
    #[account(
        seeds = [
            config.key().as_ref(),
            mint_a.key().as_ref(),
            mint_b.key().as_ref(),
            AUTHORITY_SEED,
        ],
        bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

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
        mint::authority = pool_authority,
    )]
    pub liquidity_provider_mint: Box<InterfaceAccount<'info, Mint>>,

    pub mint_a: Box<InterfaceAccount<'info, Mint>>,

    pub mint_b: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
        associated_token::token_program = token_program,
    )]
    pub pool_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
        associated_token::token_program = token_program,
    )]
    pub pool_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Solana ecosystem accounts
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
