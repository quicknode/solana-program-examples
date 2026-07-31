//! quasar-test integration tests. They drive the real program instructions
//! end-to-end: initialize a market, create users, place and cross orders,
//! settle, and withdraw fees, asserting on-chain state and token balances at
//! each step.

use {
    crate::{
        cpi::{
            CancelOrderInstruction, InitializeMarketUserInstruction, InitializeMarketInstruction,
            PlaceOrderInstruction, SettleFundsInstruction, WithdrawFeesInstruction,
        },
        errors::OrderBookError,
        state::{Market, MarketUser, Order, OrderStatus, ORDER_BOOK_ACCOUNT_SIZE},
    },
    quasar_test::prelude::*,
};

// --- Market parameters used across the tests (NVDAx-style base / USDC-style
// quote): base 9 decimals, quote 6 decimals. base_lot_size = 10^(9-6) = 1000,
// quote_lot_size = 1, so `price` reads as quote units per base lot. ---
const FEE_BASIS_POINTS: u16 = 100; // 1%
const TICK_SIZE: u64 = 1;
const BASE_LOT_SIZE: u64 = 1000;
const QUOTE_LOT_SIZE: u64 = 1;
const MIN_ORDER_SIZE: u64 = 1;
const BASE_DECIMALS: u8 = 9;
const QUOTE_DECIMALS: u8 = 6;

// Deterministic addresses keep tests independent of discovery order.
const AUTHORITY: Pubkey = Pubkey::new_from_array([1; 32]);
const BASE_MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const QUOTE_MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const ORDER_BOOK: Pubkey = Pubkey::new_from_array([4; 32]);
const BASE_VAULT: Pubkey = Pubkey::new_from_array([5; 32]);
const QUOTE_VAULT: Pubkey = Pubkey::new_from_array([6; 32]);
const FEE_VAULT: Pubkey = Pubkey::new_from_array([7; 32]);
const MAKER: Pubkey = Pubkey::new_from_array([8; 32]);
const TAKER: Pubkey = Pubkey::new_from_array([9; 32]);
const MAKER_BASE: Pubkey = Pubkey::new_from_array([10; 32]);
const MAKER_QUOTE: Pubkey = Pubkey::new_from_array([11; 32]);
const TAKER_BASE: Pubkey = Pubkey::new_from_array([12; 32]);
const TAKER_QUOTE: Pubkey = Pubkey::new_from_array([13; 32]);
const AUTHORITY_QUOTE: Pubkey = Pubkey::new_from_array([14; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([15; 32]);
const ATTACKER_QUOTE: Pubkey = Pubkey::new_from_array([16; 32]);

/// Register the authority, both mints, the pre-created order-book account, and
/// initialize the market. Returns the Market PDA.
fn init_market(test: &mut Test) -> Pubkey {
    test.add(Wallet::new().at(AUTHORITY));
    test.add(
        Mint::new(AUTHORITY)
            .at(BASE_MINT)
            .supply(1_000_000_000_000)
            .decimals(BASE_DECIMALS),
    );
    test.add(
        Mint::new(AUTHORITY)
            .at(QUOTE_MINT)
            .supply(1_000_000_000_000)
            .decimals(QUOTE_DECIMALS),
    );
    // The ~180 KB order book cannot be created via inner CPI (10 KB cap), so
    // the client pre-creates it program-owned and zeroed; this fixture stands
    // in for that `create_account` call.
    let program_id = test.program_id();
    test.add(Account::new(
        ORDER_BOOK,
        program_id,
        5_000_000_000,
        vec![0u8; ORDER_BOOK_ACCOUNT_SIZE],
    ));

    test.send(InitializeMarketInstruction {
        authority: AUTHORITY,
        order_book: ORDER_BOOK,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        base_vault: BASE_VAULT,
        quote_vault: QUOTE_VAULT,
        fee_vault: FEE_VAULT,
        fee_basis_points: FEE_BASIS_POINTS,
        tick_size: TICK_SIZE,
        base_lot_size: BASE_LOT_SIZE,
        quote_lot_size: QUOTE_LOT_SIZE,
        min_order_size: MIN_ORDER_SIZE,
    })
    .succeeds();

    test.derive_pda(Market::seeds(&BASE_MINT, &QUOTE_MINT))
}

fn initialize_market_user(test: &mut Test, market: Pubkey, owner: Pubkey) -> Pubkey {
    test.add(Wallet::new().at(owner));
    test.send(InitializeMarketUserInstruction { owner, market }).succeeds();
    test.derive_pda(MarketUser::seeds(&market, &owner))
}

#[allow(clippy::too_many_arguments)]
fn place_order(
    test: &mut Test,
    market: Pubkey,
    owner: Pubkey,
    user_base_account: Pubkey,
    user_quote_account: Pubkey,
    side: u8,
    price: u64,
    quantity: u64,
    order_id: u64,
    makers: &[(Pubkey, Pubkey)],
) -> Outcome {
    // Resting maker orders to cross arrive as remaining accounts, in pairs of
    // (maker_order, maker_market_user), in price-time priority.
    let mut remaining_accounts = Vec::new();
    for (maker_order, maker_market_user) in makers {
        remaining_accounts.push(AccountMeta::new(*maker_order, false));
        remaining_accounts.push(AccountMeta::new(*maker_market_user, false));
    }
    test.send(PlaceOrderInstruction {
        market,
        order_book: ORDER_BOOK,
        base_vault: BASE_VAULT,
        quote_vault: QUOTE_VAULT,
        fee_vault: FEE_VAULT,
        user_base_account,
        user_quote_account,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        owner,
        side,
        price,
        quantity,
        order_id,
        remaining_accounts,
    })
}

fn settle_funds(
    test: &mut Test,
    market: Pubkey,
    owner: Pubkey,
    user_base_account: Pubkey,
    user_quote_account: Pubkey,
) -> Outcome {
    test.send(SettleFundsInstruction {
        owner,
        market,
        base_vault: BASE_VAULT,
        quote_vault: QUOTE_VAULT,
        user_base_account,
        user_quote_account,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
    })
}

#[quasar_test]
fn initialize_market_stamps_market_and_order_book(test: &mut Test) {
    let market = init_market(test);

    // Market state records the pair, vaults, and parameters.
    let state = test.read::<Market>(market);
    assert_eq!(state.authority, AUTHORITY, "authority");
    assert_eq!(state.base_mint, BASE_MINT, "base_mint");
    assert_eq!(state.quote_mint, QUOTE_MINT, "quote_mint");
    assert_eq!(state.base_vault, BASE_VAULT, "base_vault");
    assert_eq!(state.quote_vault, QUOTE_VAULT, "quote_vault");
    assert_eq!(state.fee_vault, FEE_VAULT, "fee_vault");
    assert_eq!(state.order_book, ORDER_BOOK, "order_book");
    assert_eq!(u16::from(state.fee_basis_points), FEE_BASIS_POINTS);

    // Order-book discriminator + next_order_id == 1. The byte layout IS the
    // point here (hand-rolled zero-copy slab): disc(8) then market(32),
    // bids_root(8), asks_root(8), next_order_id(8)...
    let order_book = test.account(ORDER_BOOK).unwrap();
    assert_eq!(&order_book.data[0..8], b"ORDRBOOK", "order-book discriminator");
    let next_order_id_offset = 8 + 32 + 8 + 8;
    let mut id_bytes = [0u8; 8];
    id_bytes.copy_from_slice(&order_book.data[next_order_id_offset..next_order_id_offset + 8]);
    assert_eq!(u64::from_le_bytes(id_bytes), 1, "next_order_id starts at 1");
}

#[quasar_test]
fn initialize_market_user_starts_with_empty_balances(test: &mut Test) {
    let market = init_market(test);
    let market_user = initialize_market_user(test, market, MAKER);

    let state = test.read::<MarketUser>(market_user);
    assert_eq!(state.market, market, "market");
    assert_eq!(state.owner, MAKER, "owner");
    assert_eq!(u64::from(state.unsettled_base), 0);
    assert_eq!(u64::from(state.unsettled_quote), 0);
    assert_eq!(state.open_orders_len, 0);
}

/// Full lifecycle: a maker rests an ask, a taker bid crosses it fully, both
/// settle, and the authority withdraws the fee. Prices in the NVDAx/USDC lot
/// model: ask 5 lots @ 100 -> gross 500 quote, 1% fee = 5, maker nets 495
/// quote, taker receives 5000 raw base.
#[quasar_test]
fn place_match_settle_withdraw_moves_tokens_and_fees(test: &mut Test) {
    let market = init_market(test);
    let maker_market_user = initialize_market_user(test, market, MAKER);
    let taker_market_user = initialize_market_user(test, market, TAKER);

    // Maker sells 5 base lots (locks 5 * 1000 = 5000 raw base); taker buys 5
    // lots at 100 (locks 100 * 5 * 1 = 500 raw quote).
    const PRICE: u64 = 100;
    const QUANTITY: u64 = 5;
    const MAKER_BASE_LOCK: u64 = QUANTITY * BASE_LOT_SIZE; // 5000
    const TAKER_QUOTE_LOCK: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE; // 500
    const GROSS_QUOTE: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE; // 500
    const FEE_QUOTE: u64 = 5; // ceil(500 * 100 / 10000)
    const MAKER_NET_QUOTE: u64 = GROSS_QUOTE - FEE_QUOTE; // 495

    test.add(
        TokenAccount::new(BASE_MINT, MAKER)
            .at(MAKER_BASE)
            .amount(MAKER_BASE_LOCK),
    );
    test.add(TokenAccount::new(QUOTE_MINT, MAKER).at(MAKER_QUOTE));
    test.add(TokenAccount::new(BASE_MINT, TAKER).at(TAKER_BASE));
    test.add(
        TokenAccount::new(QUOTE_MINT, TAKER)
            .at(TAKER_QUOTE)
            .amount(TAKER_QUOTE_LOCK),
    );
    // Fee withdrawal destination.
    test.add(TokenAccount::new(QUOTE_MINT, AUTHORITY).at(AUTHORITY_QUOTE));

    let maker_order = test.derive_pda(Order::seeds(&market, 1));
    let taker_order = test.derive_pda(Order::seeds(&market, 2));

    // Maker ask (id 1) rests on the book.
    place_order(test, market, MAKER, MAKER_BASE, MAKER_QUOTE, 1, PRICE, QUANTITY, 1, &[])
        .succeeds();
    // Taker bid (id 2) crosses the maker ask; maker accounts supplied as
    // remaining accounts.
    place_order(
        test,
        market,
        TAKER,
        TAKER_BASE,
        TAKER_QUOTE,
        0,
        PRICE,
        QUANTITY,
        2,
        &[(maker_order, maker_market_user)],
    )
    .succeeds();
    // Both settle, then the authority sweeps the fee vault.
    settle_funds(test, market, MAKER, MAKER_BASE, MAKER_QUOTE).succeeds();
    settle_funds(test, market, TAKER, TAKER_BASE, TAKER_QUOTE).succeeds();
    test.send(WithdrawFeesInstruction {
        market,
        fee_vault: FEE_VAULT,
        authority_quote_account: AUTHORITY_QUOTE,
        quote_mint: QUOTE_MINT,
        authority: AUTHORITY,
    })
    .succeeds();

    // Both orders fully filled.
    let maker_state = test.read::<Order>(maker_order);
    assert_eq!(maker_state.status, OrderStatus::Filled as u8);
    assert_eq!(u64::from(maker_state.filled_quantity), QUANTITY);
    let taker_state = test.read::<Order>(taker_order);
    assert_eq!(taker_state.status, OrderStatus::Filled as u8);
    assert_eq!(u64::from(taker_state.filled_quantity), QUANTITY);

    // Maker's open-orders list emptied when its resting order fully filled.
    assert_eq!(test.read::<MarketUser>(maker_market_user).open_orders_len, 0);
    let _ = taker_market_user;

    // Settlement moved tokens: maker received net quote, taker received base.
    assert_eq!(test.tokens(MAKER_QUOTE), MAKER_NET_QUOTE);
    assert_eq!(test.tokens(TAKER_BASE), MAKER_BASE_LOCK);

    // Fee swept to the authority.
    assert_eq!(test.tokens(AUTHORITY_QUOTE), FEE_QUOTE);
    assert_eq!(test.tokens(FEE_VAULT), 0);

    // Vaults drained after settlement (maker sold all base, taker paid gross).
    assert_eq!(test.tokens(BASE_VAULT), 0);
    assert_eq!(test.tokens(QUOTE_VAULT), 0);
}

/// Cancelling a resting order credits the locked base back to the owner's
/// unsettled balance and marks the order cancelled.
#[quasar_test]
fn cancel_order_credits_the_locked_base_back(test: &mut Test) {
    let market = init_market(test);
    let maker_market_user = initialize_market_user(test, market, MAKER);

    const PRICE: u64 = 100;
    const QUANTITY: u64 = 5;
    const MAKER_BASE_LOCK: u64 = QUANTITY * BASE_LOT_SIZE;

    test.add(
        TokenAccount::new(BASE_MINT, MAKER)
            .at(MAKER_BASE)
            .amount(MAKER_BASE_LOCK),
    );
    test.add(TokenAccount::new(QUOTE_MINT, MAKER).at(MAKER_QUOTE));

    let maker_order = test.derive_pda(Order::seeds(&market, 1));
    place_order(test, market, MAKER, MAKER_BASE, MAKER_QUOTE, 1, PRICE, QUANTITY, 1, &[])
        .succeeds();

    test.send(CancelOrderInstruction {
        market,
        order_book: ORDER_BOOK,
        order_order_id_seed: 1,
        owner: MAKER,
    })
    .succeeds();

    assert_eq!(
        test.read::<Order>(maker_order).status,
        OrderStatus::Cancelled as u8
    );

    // The locked base is credited back to the owner's unsettled balance and
    // the open-order slot is freed.
    let market_user = test.read::<MarketUser>(maker_market_user);
    assert_eq!(u64::from(market_user.unsettled_base), MAKER_BASE_LOCK);
    assert_eq!(market_user.open_orders_len, 0);
}

/// A non-authority signer cannot withdraw the fee vault.
#[quasar_test]
fn withdraw_fees_rejects_a_non_authority_signer(test: &mut Test) {
    let market = init_market(test);
    test.add(Wallet::new().at(ATTACKER));
    test.add(TokenAccount::new(QUOTE_MINT, ATTACKER).at(ATTACKER_QUOTE));

    test.send(WithdrawFeesInstruction {
        market,
        fee_vault: FEE_VAULT,
        authority_quote_account: ATTACKER_QUOTE,
        quote_mint: QUOTE_MINT,
        authority: ATTACKER,
    })
    .fails_with(OrderBookError::NotMarketAuthority);
}
