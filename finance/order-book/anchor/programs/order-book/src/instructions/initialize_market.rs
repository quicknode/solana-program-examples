use anchor_lang::prelude::*;
use anchor_spl::token;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::errors::ErrorCode;
use crate::state::{Market, OrderBook, MARKET_SEED};

// Basis points are hundredths of a percent; 10000 bps == 100%. Fees above 100%
// would be nonsensical, so we cap here.
const MAX_FEE_BASIS_POINTS: u16 = 10_000;

pub fn handle_initialize_market(
    context: &mut Context<InitializeMarketAccountConstraints>,
    fee_basis_points: u16,
    tick_size: u64,
    base_lot_size: u64,
    quote_lot_size: u64,
    min_order_size: u64,
) -> Result<()> {
    require!(tick_size > 0, ErrorCode::InvalidTickSize);
    require!(base_lot_size > 0, ErrorCode::InvalidBaseLotSize);
    require!(quote_lot_size > 0, ErrorCode::InvalidQuoteLotSize);
    require!(min_order_size > 0, ErrorCode::BelowMinOrderSize);
    require!(
        fee_basis_points <= MAX_FEE_BASIS_POINTS,
        ErrorCode::InvalidFeeBasisPoints
    );

    let market = &mut context.accounts.market;
    market.authority = *context.accounts.authority.address();
    market.base_mint = *context.accounts.base_mint.address();
    market.quote_mint = *context.accounts.quote_mint.address();
    market.base_vault = *context.accounts.base_vault.address();
    market.quote_vault = *context.accounts.quote_vault.address();
    market.fee_vault = *context.accounts.fee_vault.address();
    market.order_book = context.accounts.order_book.address();
    market.fee_basis_points = fee_basis_points;
    market.tick_size = tick_size;
    market.base_lot_size = base_lot_size;
    market.quote_lot_size = quote_lot_size;
    market.min_order_size = min_order_size;
    market.is_active = true;
    market.bump = context.bumps.market;

    // Zero-copy account: initialize the slab in place. `load_init` is the
    // first-write path - every subsequent handler uses `load` / `load_mut`.
    // The order book is not a PDA (see the comment on the `order_book`
    // account below), so `bump` is unused and stored as 0.
    let mut order_book = context.accounts.order_book.load_init()?;
    order_book.initialize(context.accounts.market.address(), 0);

    Ok(())
}

#[derive(Accounts)]
pub struct InitializeMarketAccountConstraints {
    #[account(
        init,
        payer = authority,
        space = Market::DISCRIMINATOR.len() + Market::INIT_SPACE,
        seeds = [MARKET_SEED, base_mint.address().as_ref(), quote_mint.address().as_ref()],
        bump
    )]
    pub market: BorshAccount<Market>,

    // The order book is a zero-copy account (~180 KB: two 1024-slot critbit
    // slabs back to back). Solana's BPF runtime caps inner-CPI account
    // allocations at 10 KB, so we can't use Anchor's `init` here - the
    // client must call system_program::create_account directly before this
    // instruction, sizing the account to ORDER_BOOK_ACCOUNT_SIZE, owned by
    // this program, and zero-initialized.
    //
    // `#[account(zeroed)]` verifies the account is owned by this program
    // and has its discriminator unset, which is exactly what a freshly
    // create_account-d account looks like. The handler then stamps the
    // discriminator + struct via `load_init()`.
    //
    // This is not a PDA. create_account requires the new account to sign
    // its own creation, and a PDA has no private key to sign with, so the
    // client must generate a real keypair for it. The program ties this
    // account to its market via `has_one = order_book` on `market`, not via
    // seeds.
    #[account(zeroed)]
    pub order_book: Account<OrderBook>,

    pub base_mint: InterfaceAccount<Mint>,

    pub quote_mint: InterfaceAccount<Mint>,

    #[account(
        init,
        payer = authority,
        token::mint = base_mint,
        token::authority = market,
        token::token_program = token_program
    )]
    pub base_vault: InterfaceAccount<TokenAccount>,

    #[account(
        init,
        payer = authority,
        token::mint = quote_mint,
        token::authority = market,
        token::token_program = token_program
    )]
    pub quote_vault: InterfaceAccount<TokenAccount>,

    // Taker fees accumulate here (quote mint). Separate from quote_vault so
    // maker-owed balances and market-earned fees can't be confused.
    #[account(
        init,
        payer = authority,
        token::mint = quote_mint,
        token::authority = market,
        token::token_program = token_program
    )]
    pub fee_vault: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub authority: Signer,

    pub token_program: Interface<'static, TokenInterface>,

    pub system_program: Program<System>,
}
