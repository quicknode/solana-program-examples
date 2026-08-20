use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::OrderBookError;
use crate::state::{load_order_book_init, Market, MarketInner};

// Basis points are hundredths of a percent; 10000 bps == 100%. Fees above 100%
// would be nonsensical, so we cap here.
const MAX_FEE_BASIS_POINTS: u16 = 10_000;

#[derive(Accounts)]
pub struct InitializeMarketAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(
        init,
        payer = authority,
        address = Market::seeds(base_mint.address(), quote_mint.address())
    )]
    pub market: Account<Market>,

    // The order book is a ~180 KB zero-copy account (two 1024-slot critbit
    // slabs back to back). Solana's BPF runtime caps inner-CPI account
    // allocations at 10 KB, so it can't be `init`-ed here: the client must call
    // system_program::create_account directly before this instruction, sizing
    // the account to ORDER_BOOK_ACCOUNT_SIZE, owned by this program, and
    // zero-initialized. The handler verifies ownership + the zero
    // discriminator, then stamps and initializes it in place.
    //
    // Not a PDA. create_account requires the new account to sign its own
    // creation, and a PDA has no key to sign with, so the client generates a
    // real keypair for it. The program ties this account to its market via the
    // market's stored `order_book` field, not via seeds.
    #[account(mut)]
    pub order_book: UncheckedAccount,

    pub base_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,

    #[account(
        init,
        payer = authority,
        token(mint = base_mint, authority = market, token_program = token_program),
    )]
    pub base_vault: Account<Token>,

    #[account(
        init,
        payer = authority,
        token(mint = quote_mint, authority = market, token_program = token_program),
    )]
    pub quote_vault: Account<Token>,

    // Taker fees accumulate here (quote mint). Separate from quote_vault so
    // maker-owed balances and market-earned fees can't be confused.
    #[account(
        init,
        payer = authority,
        token(mint = quote_mint, authority = market, token_program = token_program),
    )]
    pub fee_vault: Account<Token>,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub fn handle_initialize_market(
    accounts: &mut InitializeMarketAccountConstraints,
    fee_basis_points: u16,
    tick_size: u64,
    base_lot_size: u64,
    quote_lot_size: u64,
    min_order_size: u64,
    bumps: &InitializeMarketAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require!(tick_size > 0, OrderBookError::InvalidTickSize);
    require!(base_lot_size > 0, OrderBookError::InvalidBaseLotSize);
    require!(quote_lot_size > 0, OrderBookError::InvalidQuoteLotSize);
    require!(min_order_size > 0, OrderBookError::BelowMinOrderSize);
    require!(
        fee_basis_points <= MAX_FEE_BASIS_POINTS,
        OrderBookError::InvalidFeeBasisPoints
    );

    let market_address = *accounts.market.address();
    let order_book_address = *accounts.order_book.to_account_view().address();

    // Initialize the order book in place. The client pre-created it as a
    // program-owned, zeroed account; verify ownership before casting.
    {
        let view = accounts.order_book.to_account_view();
        require!(
            view.owned_by(&crate::ID),
            OrderBookError::InvalidOrderBookOwner
        );
        // SAFETY: `order_book` is writable and not aliased elsewhere in this
        // instruction. The cast mirrors the read-only raw-slice pattern used
        // in the pyth example, extended to a mutable slice for initialization.
        let data =
            unsafe { core::slice::from_raw_parts_mut(view.data_ptr() as *mut u8, view.data_len()) };
        let order_book = load_order_book_init(data)?;
        // The order book is not a PDA, so its stored `bump` is unused (0).
        order_book.initialize(market_address.to_bytes(), 0);
    }

    accounts.market.set_inner(MarketInner {
        authority: *accounts.authority.address(),
        base_mint: *accounts.base_mint.address(),
        quote_mint: *accounts.quote_mint.address(),
        base_vault: *accounts.base_vault.address(),
        quote_vault: *accounts.quote_vault.address(),
        fee_vault: *accounts.fee_vault.address(),
        order_book: order_book_address,
        fee_basis_points,
        tick_size,
        base_lot_size,
        quote_lot_size,
        min_order_size,
        is_active: PodBool::from(true),
        bump: bumps.market,
    });

    Ok(())
}
