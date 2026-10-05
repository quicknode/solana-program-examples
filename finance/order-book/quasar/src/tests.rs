//! quasar-test integration tests. They drive the real program instructions
//! end-to-end: initialize a market, create users, place and cross orders,
//! settle, and withdraw fees, asserting on-chain state and token balances at
//! each step. The pause block checks that `pause_market` stops new orders and
//! nothing else, and that `resume_market` reopens the market. The closing
//! block at the end checks that `close_order` and `close_market_user` give a
//! finished order's and an idle user's rent back to the owner, and refuse
//! while anything still rests or is owed.

use {
    crate::{
        cpi::{
            CancelOrderInstruction, CloseMarketUserInstruction, CloseOrderInstruction,
            InitializeMarketInstruction, InitializeMarketUserInstruction, PauseMarketInstruction,
            PlaceOrderInstruction, ResumeMarketInstruction, SettleFundsInstruction,
            WithdrawFeesInstruction,
        },
        errors::OrderBookError,
        state::{
            BaseVaultPda, FeeVaultPda, Market, MarketUser, Order, OrderStatus, QuoteVaultPda,
            ORDER_BOOK_ACCOUNT_SIZE,
        },
    },
    quasar_test::prelude::*,
};

// --- Market parameters used across the tests: NVDAx (8 decimals onchain,
// https://explorer.solana.com/address/Xsc9qvGR1efVDFGLrVsmkzv3qi45LTBjeUKSPmx9qEh)
// over USDC (6 decimals). base_lot_size = 10^(8-6) = 100, quote_lot_size = 1,
// so `price` reads as quote units per base lot. ---
const FEE_BASIS_POINTS: u16 = 100; // 1%
const TICK_SIZE: u64 = 1;
const BASE_LOT_SIZE: u64 = 100;
const QUOTE_LOT_SIZE: u64 = 1;
const MIN_ORDER_SIZE: u64 = 1;
const BASE_DECIMALS: u8 = 8;
const QUOTE_DECIMALS: u8 = 6;

// Deterministic addresses keep tests independent of discovery order.
const AUTHORITY: Pubkey = Pubkey::new_from_array([1; 32]);
const BASE_MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const QUOTE_MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const ORDER_BOOK: Pubkey = Pubkey::new_from_array([4; 32]);
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
        fee_basis_points: FEE_BASIS_POINTS,
        tick_size: TICK_SIZE,
        base_lot_size: BASE_LOT_SIZE,
        quote_lot_size: QUOTE_LOT_SIZE,
        min_order_size: MIN_ORDER_SIZE,
    })
    .succeeds();

    test.derive_pda(Market::seeds(&BASE_MINT, &QUOTE_MINT))
}

/// The market's three vaults. They are PDAs of the market, so a client derives
/// them rather than choosing them.
struct Vaults {
    base: Pubkey,
    quote: Pubkey,
    fee: Pubkey,
}

fn vaults(test: &mut Test, market: Pubkey) -> Vaults {
    Vaults {
        base: test.derive_pda(BaseVaultPda::seeds(&market)),
        quote: test.derive_pda(QuoteVaultPda::seeds(&market)),
        fee: test.derive_pda(FeeVaultPda::seeds(&market)),
    }
}

fn initialize_market_user(test: &mut Test, market: Pubkey, owner: Pubkey) -> Pubkey {
    test.add(Wallet::new().at(owner));
    test.send(InitializeMarketUserInstruction { owner, market })
        .succeeds();
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
    let vaults = vaults(test, market);
    test.send(PlaceOrderInstruction {
        market,
        order_book: ORDER_BOOK,
        base_vault: vaults.base,
        quote_vault: vaults.quote,
        fee_vault: vaults.fee,
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
    let vaults = vaults(test, market);
    test.send(SettleFundsInstruction {
        owner,
        market,
        base_vault: vaults.base,
        quote_vault: vaults.quote,
        user_base_account,
        user_quote_account,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
    })
}

#[quasar_test]
fn initialize_market_stamps_market_and_order_book(test: &mut Test) {
    let market = init_market(test);

    // Market state records the pair, vaults, and parameters. The vaults it
    // records are the market's PDAs, not addresses the client chose.
    let vaults = vaults(test, market);
    let state = test.read::<Market>(market);
    assert_eq!(state.authority, AUTHORITY, "authority");
    assert_eq!(state.base_mint, BASE_MINT, "base_mint");
    assert_eq!(state.quote_mint, QUOTE_MINT, "quote_mint");
    assert_eq!(state.base_vault, vaults.base, "base_vault");
    assert_eq!(state.quote_vault, vaults.quote, "quote_vault");
    assert_eq!(state.fee_vault, vaults.fee, "fee_vault");
    assert_eq!(state.order_book, ORDER_BOOK, "order_book");
    assert_eq!(u16::from(state.fee_basis_points), FEE_BASIS_POINTS);

    // Order-book discriminator + next_order_id == 1. The byte layout IS the
    // point here (hand-rolled zero-copy slab): disc(8) then market(32),
    // bids_root(8), asks_root(8), next_order_id(8)...
    let order_book = test.account(ORDER_BOOK).unwrap();
    assert_eq!(
        &order_book.data[0..8],
        b"ORDRBOOK",
        "order-book discriminator"
    );
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
/// quote, taker receives 500 raw base.
#[quasar_test]
fn place_match_settle_withdraw_moves_tokens_and_fees(test: &mut Test) {
    let market = init_market(test);
    let maker_market_user = initialize_market_user(test, market, MAKER);
    let taker_market_user = initialize_market_user(test, market, TAKER);

    // Maker sells 5 base lots (locks 5 * 100 = 500 raw base); taker buys 5
    // lots at 100 (locks 100 * 5 * 1 = 500 raw quote).
    const PRICE: u64 = 100;
    const QUANTITY: u64 = 5;
    const MAKER_BASE_LOCK: u64 = QUANTITY * BASE_LOT_SIZE; // 500
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
    place_order(
        test,
        market,
        MAKER,
        MAKER_BASE,
        MAKER_QUOTE,
        1,
        PRICE,
        QUANTITY,
        1,
        &[],
    )
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
    let vaults = vaults(test, market);
    test.send(WithdrawFeesInstruction {
        market,
        fee_vault: vaults.fee,
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
    assert_eq!(
        test.read::<MarketUser>(maker_market_user).open_orders_len,
        0
    );
    let _ = taker_market_user;

    // Settlement moved tokens: maker received net quote, taker received base.
    assert_eq!(test.tokens(MAKER_QUOTE), MAKER_NET_QUOTE);
    assert_eq!(test.tokens(TAKER_BASE), MAKER_BASE_LOCK);

    // Fee swept to the authority.
    assert_eq!(test.tokens(AUTHORITY_QUOTE), FEE_QUOTE);
    assert_eq!(test.tokens(vaults.fee), 0);

    // Vaults drained after settlement (maker sold all base, taker paid gross).
    assert_eq!(test.tokens(vaults.base), 0);
    assert_eq!(test.tokens(vaults.quote), 0);
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
    place_order(
        test,
        market,
        MAKER,
        MAKER_BASE,
        MAKER_QUOTE,
        1,
        PRICE,
        QUANTITY,
        1,
        &[],
    )
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

    let vaults = vaults(test, market);
    test.send(WithdrawFeesInstruction {
        market,
        fee_vault: vaults.fee,
        authority_quote_account: ATTACKER_QUOTE,
        quote_mint: QUOTE_MINT,
        authority: ATTACKER,
    })
    .fails_with(OrderBookError::NotMarketAuthority);
}

// --- Eviction: a full side makes room for a better order ---
//
// A side's 1024 tree nodes hold 512 resting orders, because every order after
// the first adds a leaf and an inner node. These tests fill the bid side with
// 512 one-lot bids, one per price from EVICTION_WORST_BID_PRICE upward, so the
// worst bid is the first one placed: order ID 1, at the lowest price.

const BID: u8 = 0;
const ORDERS_PER_SIDE: u64 = 512;
// One under the 20-order cap, so every filler can still place an order of
// their own (the self-eviction test needs that).
const ORDERS_PER_FILLER: u64 = 19;
const EVICTION_WORST_BID_PRICE: u64 = 100;
const EVICTION_ORDER_QUANTITY: u64 = 1;
const EVICTION_BETTER_BID_PRICE: u64 = 10_000;
const WORST_BID_ORDER_ID: u64 = 1;
const EVICTION_TRADER_QUOTE: u64 = 1_000_000_000;

struct Trader {
    owner: Pubkey,
    market_user: Pubkey,
    base: Pubkey,
    quote: Pubkey,
}

/// A distinct address per trader and role, clear of the fixed addresses above.
fn trader_address(index: u8, role: u8) -> Pubkey {
    let mut bytes = [0u8; 32];
    bytes[0] = 200;
    bytes[1] = index;
    bytes[2] = role;
    Pubkey::new_from_array(bytes)
}

/// A funded trader with a MarketUser.
fn create_trader(test: &mut Test, market: Pubkey, index: u8) -> Trader {
    let owner = trader_address(index, 1);
    let base = trader_address(index, 2);
    let quote = trader_address(index, 3);
    let market_user = initialize_market_user(test, market, owner);
    test.add(TokenAccount::new(BASE_MINT, owner).at(base));
    test.add(
        TokenAccount::new(QUOTE_MINT, owner)
            .at(quote)
            .amount(EVICTION_TRADER_QUOTE),
    );
    Trader {
        owner,
        market_user,
        base,
        quote,
    }
}

/// Fill the bid side to capacity. Returns the fillers in order; the first one
/// owns the worst bid, `WORST_BID_ORDER_ID`.
fn fill_bid_side(test: &mut Test, market: Pubkey) -> Vec<Trader> {
    let mut fillers: Vec<Trader> = Vec::new();
    for order_id in 1..=ORDERS_PER_SIDE {
        if (order_id - 1) % ORDERS_PER_FILLER == 0 {
            let index = fillers.len() as u8;
            fillers.push(create_trader(test, market, index));
        }
        let filler = fillers.last().unwrap();
        let (owner, base, quote) = (filler.owner, filler.base, filler.quote);
        place_order(
            test,
            market,
            owner,
            base,
            quote,
            BID,
            EVICTION_WORST_BID_PRICE + (order_id - 1),
            EVICTION_ORDER_QUANTITY,
            order_id,
            &[],
        )
        .succeeds();
    }
    fillers
}

/// Place a one-lot bid, passing `evicted` as the accounts after the (empty)
/// maker pairs.
fn place_bid(
    test: &mut Test,
    market: Pubkey,
    trader: &Trader,
    order_id: u64,
    price: u64,
    evicted: &[Pubkey],
) -> Outcome {
    let remaining_accounts = evicted
        .iter()
        .map(|address| AccountMeta::new(*address, false))
        .collect();
    let vaults = vaults(test, market);
    test.send(PlaceOrderInstruction {
        market,
        order_book: ORDER_BOOK,
        base_vault: vaults.base,
        quote_vault: vaults.quote,
        fee_vault: vaults.fee,
        user_base_account: trader.base,
        user_quote_account: trader.quote,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
        owner: trader.owner,
        side: BID,
        price,
        quantity: EVICTION_ORDER_QUANTITY,
        order_id,
        remaining_accounts,
    })
}

#[quasar_test]
fn full_side_refuses_an_order_no_better_than_its_worst(test: &mut Test) {
    let market = init_market(test);
    let fillers = fill_bid_side(test, market);
    let worst_order = test.derive_pda(Order::seeds(&market, WORST_BID_ORDER_ID));

    // Worse than the worst bid, and equal to it: equal is not better, because
    // the resting bid got there first.
    for price in [EVICTION_WORST_BID_PRICE - 1, EVICTION_WORST_BID_PRICE] {
        place_bid(
            test,
            market,
            &fillers[1],
            ORDERS_PER_SIDE + 1,
            price,
            &[worst_order, fillers[0].market_user],
        )
        .fails_with(OrderBookError::OrderBookFull);
    }
    assert_eq!(
        test.read::<Order>(worst_order).status,
        OrderStatus::Open as u8
    );
}

#[quasar_test]
fn better_order_evicts_the_worst_and_rests(test: &mut Test) {
    let market = init_market(test);
    let fillers = fill_bid_side(test, market);
    let newcomer = create_trader(test, market, 100);
    let new_order_id = ORDERS_PER_SIDE + 1;
    let worst_order = test.derive_pda(Order::seeds(&market, WORST_BID_ORDER_ID));

    place_bid(
        test,
        market,
        &newcomer,
        new_order_id,
        EVICTION_BETTER_BID_PRICE,
        &[worst_order, fillers[0].market_user],
    )
    .succeeds();

    assert_eq!(
        test.read::<Order>(worst_order).status,
        OrderStatus::Cancelled as u8
    );
    // The evicted bid's whole lock is owed back to its owner, exactly as a
    // cancel would owe it.
    let evicted_user = test.read::<MarketUser>(fillers[0].market_user);
    assert_eq!(
        u64::from(evicted_user.unsettled_quote),
        EVICTION_WORST_BID_PRICE * EVICTION_ORDER_QUANTITY * QUOTE_LOT_SIZE
    );
    assert_eq!(evicted_user.open_orders_len as u64, ORDERS_PER_FILLER - 1);

    let new_order = test.derive_pda(Order::seeds(&market, new_order_id));
    assert_eq!(
        test.read::<Order>(new_order).status,
        OrderStatus::Open as u8
    );
    assert_eq!(
        test.read::<MarketUser>(newcomer.market_user)
            .open_orders_len,
        1
    );

    // The side is still full, and its worst bid is now order 2, one tick up.
    let second_worst = test.derive_pda(Order::seeds(&market, WORST_BID_ORDER_ID + 1));
    place_bid(
        test,
        market,
        &newcomer,
        new_order_id + 1,
        EVICTION_WORST_BID_PRICE + 1,
        &[second_worst, fillers[0].market_user],
    )
    .fails_with(OrderBookError::OrderBookFull);
}

#[quasar_test]
fn evicted_maker_settles_their_refund(test: &mut Test) {
    let market = init_market(test);
    let fillers = fill_bid_side(test, market);
    let newcomer = create_trader(test, market, 100);
    let evicted = &fillers[0];
    let worst_order = test.derive_pda(Order::seeds(&market, WORST_BID_ORDER_ID));
    let quote_before = test.tokens(evicted.quote);

    place_bid(
        test,
        market,
        &newcomer,
        ORDERS_PER_SIDE + 1,
        EVICTION_BETTER_BID_PRICE,
        &[worst_order, evicted.market_user],
    )
    .succeeds();
    settle_funds(test, market, evicted.owner, evicted.base, evicted.quote).succeeds();

    assert_eq!(
        test.tokens(evicted.quote) - quote_before,
        EVICTION_WORST_BID_PRICE * EVICTION_ORDER_QUANTITY * QUOTE_LOT_SIZE
    );
    let evicted_user = test.read::<MarketUser>(evicted.market_user);
    assert_eq!(u64::from(evicted_user.unsettled_base), 0);
    assert_eq!(u64::from(evicted_user.unsettled_quote), 0);
}

#[quasar_test]
fn eviction_rejects_missing_or_wrong_evicted_accounts(test: &mut Test) {
    let market = init_market(test);
    let fillers = fill_bid_side(test, market);
    let newcomer = create_trader(test, market, 100);
    let new_order_id = ORDERS_PER_SIDE + 1;
    let worst_order = test.derive_pda(Order::seeds(&market, WORST_BID_ORDER_ID));
    let second_worst = test.derive_pda(Order::seeds(&market, WORST_BID_ORDER_ID + 1));

    place_bid(
        test,
        market,
        &newcomer,
        new_order_id,
        EVICTION_BETTER_BID_PRICE,
        &[],
    )
    .fails_with(OrderBookError::MissingEvictedAccounts);

    // Order 2 is resting, but it is not the worst bid.
    place_bid(
        test,
        market,
        &newcomer,
        new_order_id,
        EVICTION_BETTER_BID_PRICE,
        &[second_worst, fillers[0].market_user],
    )
    .fails_with(OrderBookError::EvictedAccountMismatch);

    // The right order, with someone else's MarketUser to credit.
    place_bid(
        test,
        market,
        &newcomer,
        new_order_id,
        EVICTION_BETTER_BID_PRICE,
        &[worst_order, fillers[1].market_user],
    )
    .fails_with(OrderBookError::EvictedAccountMismatch);

    assert_eq!(
        test.read::<Order>(worst_order).status,
        OrderStatus::Open as u8
    );
}

#[quasar_test]
fn trader_can_evict_their_own_worst_order(test: &mut Test) {
    let market = init_market(test);
    let fillers = fill_bid_side(test, market);
    let owner = &fillers[0];
    let worst_order = test.derive_pda(Order::seeds(&market, WORST_BID_ORDER_ID));

    // Only the evicted order is passed: the owner's MarketUser is already the
    // instruction's `market_user`.
    place_bid(
        test,
        market,
        owner,
        ORDERS_PER_SIDE + 1,
        EVICTION_BETTER_BID_PRICE,
        &[worst_order],
    )
    .succeeds();

    assert_eq!(
        test.read::<Order>(worst_order).status,
        OrderStatus::Cancelled as u8
    );
    let user = test.read::<MarketUser>(owner.market_user);
    assert_eq!(
        u64::from(user.unsettled_quote),
        EVICTION_WORST_BID_PRICE * EVICTION_ORDER_QUANTITY * QUOTE_LOT_SIZE
    );
    // One order out, one order in.
    assert_eq!(user.open_orders_len as u64, ORDERS_PER_FILLER);
}

// --- Pause and resume: a pause stops new orders and nothing else ---

// The pause tests share one resting ask and one crossing bid. Chosen apart
// from the suite's other sizes so a balance that matches is this test's own.
const PAUSE_ASK_ID: u64 = 1;
const PAUSE_BID_ID: u64 = 2;
const PAUSE_PRICE: u64 = 1_300;
const PAUSE_QUANTITY: u64 = 7;
const PAUSE_GROSS: u64 = PAUSE_PRICE * PAUSE_QUANTITY * QUOTE_LOT_SIZE; // 9100
const PAUSE_FEE: u64 = 91; // ceil(9100 * 100 / 10000)
const PAUSE_LOCKED_BASE: u64 = PAUSE_QUANTITY * BASE_LOT_SIZE; // 700
const ASK: u8 = 1;

/// A market with the maker (the seller) and the taker (the buyer) registered
/// and funded for the pause tests' one ask and one bid.
fn init_pause_market(test: &mut Test) -> Pubkey {
    let market = init_market(test);
    initialize_market_user(test, market, MAKER);
    initialize_market_user(test, market, TAKER);
    test.add(
        TokenAccount::new(BASE_MINT, MAKER)
            .at(MAKER_BASE)
            .amount(PAUSE_LOCKED_BASE),
    );
    test.add(TokenAccount::new(QUOTE_MINT, MAKER).at(MAKER_QUOTE));
    test.add(TokenAccount::new(BASE_MINT, TAKER).at(TAKER_BASE));
    test.add(
        TokenAccount::new(QUOTE_MINT, TAKER)
            .at(TAKER_QUOTE)
            .amount(PAUSE_GROSS),
    );
    market
}

/// The maker places the pause tests' ask.
fn place_pause_ask(test: &mut Test, market: Pubkey) -> Outcome {
    place_order(
        test,
        market,
        MAKER,
        MAKER_BASE,
        MAKER_QUOTE,
        ASK,
        PAUSE_PRICE,
        PAUSE_QUANTITY,
        PAUSE_ASK_ID,
        &[],
    )
}

#[quasar_test]
fn pause_market_refuses_new_orders_with_market_paused(test: &mut Test) {
    let market = init_pause_market(test);

    test.send(PauseMarketInstruction {
        market,
        authority: AUTHORITY,
    })
    .succeeds();
    assert!(!test.read::<Market>(market).is_active.is_true());

    place_pause_ask(test, market).fails_with(OrderBookError::MarketPaused);
    // The refused ask locked nothing.
    let vaults = vaults(test, market);
    assert_eq!(test.tokens(vaults.base), 0);
    assert_eq!(test.tokens(MAKER_BASE), PAUSE_LOCKED_BASE);
}

#[quasar_test]
fn paused_market_still_cancels_and_settles_a_resting_order(test: &mut Test) {
    let market = init_pause_market(test);
    let vaults = vaults(test, market);

    place_pause_ask(test, market).succeeds();
    assert_eq!(test.tokens(vaults.base), PAUSE_LOCKED_BASE);

    test.send(PauseMarketInstruction {
        market,
        authority: AUTHORITY,
    })
    .succeeds();

    // The ask was placed before the pause; its owner can still cancel it
    // and take the locked base back out while the market is paused.
    let cancel: Instruction = CancelOrderInstruction {
        market,
        order_book: ORDER_BOOK,
        order_order_id_seed: PAUSE_ASK_ID,
        owner: MAKER,
    }
    .into();
    let settle: Instruction = SettleFundsInstruction {
        owner: MAKER,
        market,
        base_vault: vaults.base,
        quote_vault: vaults.quote,
        user_base_account: MAKER_BASE,
        user_quote_account: MAKER_QUOTE,
        base_mint: BASE_MINT,
        quote_mint: QUOTE_MINT,
    }
    .into();
    test.send_all([cancel, settle]).succeeds();

    let ask = test.derive_pda(Order::seeds(&market, PAUSE_ASK_ID));
    assert_eq!(test.read::<Order>(ask).status, OrderStatus::Cancelled as u8);
    assert_eq!(test.tokens(vaults.base), 0);
    assert_eq!(test.tokens(MAKER_BASE), PAUSE_LOCKED_BASE);
}

#[quasar_test]
fn paused_market_still_pays_out_fills_and_withdraws_fees(test: &mut Test) {
    let market = init_pause_market(test);
    let maker_market_user = test.derive_pda(MarketUser::seeds(&market, &MAKER));
    let vaults = vaults(test, market);
    test.add(TokenAccount::new(QUOTE_MINT, AUTHORITY).at(AUTHORITY_QUOTE));

    // A fill before the pause leaves the maker owed quote in their
    // unsettled balance and the fee vault holding the taker fee.
    place_pause_ask(test, market).succeeds();
    let ask = test.derive_pda(Order::seeds(&market, PAUSE_ASK_ID));
    place_order(
        test,
        market,
        TAKER,
        TAKER_BASE,
        TAKER_QUOTE,
        BID,
        PAUSE_PRICE,
        PAUSE_QUANTITY,
        PAUSE_BID_ID,
        &[(ask, maker_market_user)],
    )
    .succeeds();
    assert_eq!(test.tokens(vaults.fee), PAUSE_FEE);

    test.send(PauseMarketInstruction {
        market,
        authority: AUTHORITY,
    })
    .succeeds();

    // The maker's settlement goes through while paused.
    settle_funds(test, market, MAKER, MAKER_BASE, MAKER_QUOTE).succeeds();
    assert_eq!(test.tokens(MAKER_QUOTE), PAUSE_GROSS - PAUSE_FEE);

    // So does the authority's fee withdrawal.
    test.send(WithdrawFeesInstruction {
        market,
        fee_vault: vaults.fee,
        authority_quote_account: AUTHORITY_QUOTE,
        quote_mint: QUOTE_MINT,
        authority: AUTHORITY,
    })
    .succeeds();
    assert_eq!(test.tokens(vaults.fee), 0);
    assert_eq!(test.tokens(AUTHORITY_QUOTE), PAUSE_FEE);
}

#[quasar_test]
fn resume_market_accepts_orders_again(test: &mut Test) {
    let market = init_pause_market(test);

    test.send(PauseMarketInstruction {
        market,
        authority: AUTHORITY,
    })
    .succeeds();
    place_pause_ask(test, market).fails_with(OrderBookError::MarketPaused);

    test.send(ResumeMarketInstruction {
        market,
        authority: AUTHORITY,
    })
    .succeeds();
    assert!(test.read::<Market>(market).is_active.is_true());

    // The same ask, refused a moment ago, now rests and locks its base.
    place_pause_ask(test, market).succeeds();
    let vaults = vaults(test, market);
    assert_eq!(test.tokens(vaults.base), PAUSE_LOCKED_BASE);
}

#[quasar_test]
fn only_the_market_authority_can_pause_or_resume(test: &mut Test) {
    let market = init_pause_market(test);

    // A trader signing `pause_market` is refused, and the market stays open:
    // the ask goes through afterwards.
    test.send(PauseMarketInstruction {
        market,
        authority: TAKER,
    })
    .fails_with(OrderBookError::NotMarketAuthority);
    place_pause_ask(test, market).succeeds();

    // Once the authority has paused, a trader signing `resume_market` is
    // refused too, and the market stays paused: the taker's bid is refused.
    test.send(PauseMarketInstruction {
        market,
        authority: AUTHORITY,
    })
    .succeeds();
    test.send(ResumeMarketInstruction {
        market,
        authority: TAKER,
    })
    .fails_with(OrderBookError::NotMarketAuthority);
    place_order(
        test,
        market,
        TAKER,
        TAKER_BASE,
        TAKER_QUOTE,
        BID,
        PAUSE_PRICE,
        PAUSE_QUANTITY,
        PAUSE_BID_ID,
        &[],
    )
    .fails_with(OrderBookError::MarketPaused);
}

// --- Closing accounts: a finished order and an idle MarketUser give their
// rent back to the owner who paid it. Every test here uses the maker's ask of
// CLOSE_QUANTITY lots at CLOSE_PRICE (order CLOSE_ASK_ID), and the ones that
// need a fill cross it with the taker's bid (order CLOSE_BID_ID). quasar-test
// charges no transaction fee, so an owner's balance rises by exactly the
// closed account's rent. ---

const CLOSE_ASK_ID: u64 = 1;
const CLOSE_BID_ID: u64 = 2;
const CLOSE_PRICE: u64 = 100;
const CLOSE_QUANTITY: u64 = 5;
const CLOSE_LOCKED_BASE: u64 = CLOSE_QUANTITY * BASE_LOT_SIZE; // 500
const CLOSE_GROSS: u64 = CLOSE_PRICE * CLOSE_QUANTITY * QUOTE_LOT_SIZE; // 500
const CLOSE_FEE: u64 = 5; // ceil(500 * 100 / 10000)

/// A market with the maker and taker registered and funded for the closing
/// tests' ask and the bid that crosses it.
fn init_close_market(test: &mut Test) -> Pubkey {
    let market = init_market(test);
    initialize_market_user(test, market, MAKER);
    initialize_market_user(test, market, TAKER);
    test.add(
        TokenAccount::new(BASE_MINT, MAKER)
            .at(MAKER_BASE)
            .amount(CLOSE_LOCKED_BASE),
    );
    test.add(TokenAccount::new(QUOTE_MINT, MAKER).at(MAKER_QUOTE));
    test.add(TokenAccount::new(BASE_MINT, TAKER).at(TAKER_BASE));
    test.add(
        TokenAccount::new(QUOTE_MINT, TAKER)
            .at(TAKER_QUOTE)
            .amount(CLOSE_GROSS),
    );
    market
}

/// The maker rests the closing tests' ask.
fn place_close_ask(test: &mut Test, market: Pubkey) {
    place_order(
        test,
        market,
        MAKER,
        MAKER_BASE,
        MAKER_QUOTE,
        ASK,
        CLOSE_PRICE,
        CLOSE_QUANTITY,
        CLOSE_ASK_ID,
        &[],
    )
    .succeeds();
}

/// The taker crosses the maker's ask with a bid of `quantity` lots at the
/// same price: the whole ask when `quantity` is CLOSE_QUANTITY, part of it
/// otherwise.
fn cross_close_ask(test: &mut Test, market: Pubkey, quantity: u64) {
    let maker_order = test.derive_pda(Order::seeds(&market, CLOSE_ASK_ID));
    let maker_market_user = test.derive_pda(MarketUser::seeds(&market, &MAKER));
    place_order(
        test,
        market,
        TAKER,
        TAKER_BASE,
        TAKER_QUOTE,
        BID,
        CLOSE_PRICE,
        quantity,
        CLOSE_BID_ID,
        &[(maker_order, maker_market_user)],
    )
    .succeeds();
}

/// The maker cancels their ask, which credits its locked base to their
/// unsettled balance.
fn cancel_close_ask(test: &mut Test, market: Pubkey) {
    test.send(CancelOrderInstruction {
        market,
        order_book: ORDER_BOOK,
        order_order_id_seed: CLOSE_ASK_ID,
        owner: MAKER,
    })
    .succeeds();
}

fn close_order(market: Pubkey, owner: Pubkey, order_id: u64) -> CloseOrderInstruction {
    CloseOrderInstruction {
        market,
        order_order_id_seed: order_id,
        owner,
    }
}

/// `close_market_user` signed by `owner` for the MarketUser of
/// `account_owner`: the signer's own, unless a test is checking that a
/// non-owner is refused.
fn close_market_user(
    test: &Test,
    market: Pubkey,
    owner: Pubkey,
    account_owner: Pubkey,
) -> CloseMarketUserInstruction {
    CloseMarketUserInstruction {
        market,
        market_user: test.derive_pda(MarketUser::seeds(&market, &account_owner)),
        owner,
    }
}

/// Closes `order_id` as `owner`, asserting the account is gone afterwards and
/// that the owner's balance rose by exactly its rent.
fn close_order_and_assert_rent_returned(
    test: &mut Test,
    market: Pubkey,
    owner: Pubkey,
    order_id: u64,
) {
    let order = test.derive_pda(Order::seeds(&market, order_id));
    let rent = test.lamports(order);
    let owner_before = test.lamports(owner);
    test.send(close_order(market, owner, order_id))
        .succeeds()
        .is_closed(order)
        .has_lamports(owner, owner_before + rent);
}

#[quasar_test]
fn close_order_returns_a_cancelled_orders_rent(test: &mut Test) {
    let market = init_close_market(test);
    place_close_ask(test, market);
    cancel_close_ask(test, market);

    close_order_and_assert_rent_returned(test, market, MAKER, CLOSE_ASK_ID);

    // Closing the order touches neither the refund it had already credited
    // nor the vault that still holds the tokens until settlement.
    let maker_market_user = test.derive_pda(MarketUser::seeds(&market, &MAKER));
    assert_eq!(
        u64::from(test.read::<MarketUser>(maker_market_user).unsettled_base),
        CLOSE_LOCKED_BASE
    );
    let base_vault = vaults(test, market).base;
    assert_eq!(test.tokens(base_vault), CLOSE_LOCKED_BASE);
}

#[quasar_test]
fn close_order_returns_a_filled_orders_rent(test: &mut Test) {
    let market = init_close_market(test);
    place_close_ask(test, market);
    cross_close_ask(test, market, CLOSE_QUANTITY);

    // Both the maker's ask and the taker's bid filled in full; each owner
    // closes their own.
    close_order_and_assert_rent_returned(test, market, MAKER, CLOSE_ASK_ID);
    close_order_and_assert_rent_returned(test, market, TAKER, CLOSE_BID_ID);

    // The fills' credits are untouched: they live on the MarketUser
    // accounts, not on the orders.
    let maker_market_user = test.derive_pda(MarketUser::seeds(&market, &MAKER));
    let taker_market_user = test.derive_pda(MarketUser::seeds(&market, &TAKER));
    assert_eq!(
        u64::from(test.read::<MarketUser>(maker_market_user).unsettled_quote),
        CLOSE_GROSS - CLOSE_FEE
    );
    assert_eq!(
        u64::from(test.read::<MarketUser>(taker_market_user).unsettled_base),
        CLOSE_LOCKED_BASE
    );
}

#[quasar_test]
fn close_order_refuses_a_resting_order(test: &mut Test) {
    let market = init_close_market(test);
    place_close_ask(test, market);

    test.send(close_order(market, MAKER, CLOSE_ASK_ID))
        .fails_with(OrderBookError::OrderNotClosable);
    let order = test.derive_pda(Order::seeds(&market, CLOSE_ASK_ID));
    assert_eq!(test.read::<Order>(order).status, OrderStatus::Open as u8);
}

#[quasar_test]
fn close_order_refuses_a_partially_filled_order(test: &mut Test) {
    let market = init_close_market(test);
    place_close_ask(test, market);
    cross_close_ask(test, market, CLOSE_QUANTITY - 1);

    let order = test.derive_pda(Order::seeds(&market, CLOSE_ASK_ID));
    assert_eq!(
        test.read::<Order>(order).status,
        OrderStatus::PartiallyFilled as u8
    );

    test.send(close_order(market, MAKER, CLOSE_ASK_ID))
        .fails_with(OrderBookError::OrderNotClosable);
    assert_eq!(
        test.read::<Order>(order).status,
        OrderStatus::PartiallyFilled as u8
    );
}

#[quasar_test]
fn close_order_refuses_a_non_owner(test: &mut Test) {
    let market = init_close_market(test);
    place_close_ask(test, market);
    cancel_close_ask(test, market);

    // The order is closable, but only by the maker: the taker signing for
    // it would be paid the maker's rent.
    test.send(close_order(market, TAKER, CLOSE_ASK_ID))
        .fails_with(OrderBookError::Unauthorized);
    let order = test.derive_pda(Order::seeds(&market, CLOSE_ASK_ID));
    assert_eq!(
        test.read::<Order>(order).status,
        OrderStatus::Cancelled as u8
    );
}

#[quasar_test]
fn close_market_user_returns_rent_when_nothing_is_open_or_owed(test: &mut Test) {
    let market = init_close_market(test);

    // Place, cancel and settle, so the account has been through a full
    // cycle and is back to nothing open and nothing owed.
    place_close_ask(test, market);
    cancel_close_ask(test, market);
    settle_funds(test, market, MAKER, MAKER_BASE, MAKER_QUOTE).succeeds();
    let maker_market_user = test.derive_pda(MarketUser::seeds(&market, &MAKER));
    let state = test.read::<MarketUser>(maker_market_user);
    assert_eq!(state.open_orders_len, 0);
    assert_eq!(u64::from(state.unsettled_base), 0);
    assert_eq!(u64::from(state.unsettled_quote), 0);

    let rent = test.lamports(maker_market_user);
    let maker_before = test.lamports(MAKER);
    let close_ix = close_market_user(test, market, MAKER, MAKER);
    test.send(close_ix)
        .succeeds()
        .is_closed(maker_market_user)
        .has_lamports(MAKER, maker_before + rent);

    // The maker can come back to the market: the PDA is free to create
    // again.
    test.send(InitializeMarketUserInstruction {
        owner: MAKER,
        market,
    })
    .succeeds();
    assert_eq!(test.read::<MarketUser>(maker_market_user).owner, MAKER);
}

#[quasar_test]
fn close_market_user_refuses_an_open_order(test: &mut Test) {
    let market = init_close_market(test);
    place_close_ask(test, market);

    let close_ix = close_market_user(test, market, MAKER, MAKER);
    test.send(close_ix)
        .fails_with(OrderBookError::MarketUserNotClosable);
    let maker_market_user = test.derive_pda(MarketUser::seeds(&market, &MAKER));
    assert_eq!(
        test.read::<MarketUser>(maker_market_user).open_orders_len,
        1
    );
}

#[quasar_test]
fn close_market_user_refuses_an_unsettled_balance(test: &mut Test) {
    let market = init_close_market(test);

    // Cancelled but not settled: nothing is open, but the refund is still
    // owed through this account and would be lost with it.
    place_close_ask(test, market);
    cancel_close_ask(test, market);
    let maker_market_user = test.derive_pda(MarketUser::seeds(&market, &MAKER));
    assert_eq!(
        test.read::<MarketUser>(maker_market_user).open_orders_len,
        0
    );

    let close_ix = close_market_user(test, market, MAKER, MAKER);
    test.send(close_ix)
        .fails_with(OrderBookError::MarketUserNotClosable);
    assert_eq!(
        u64::from(test.read::<MarketUser>(maker_market_user).unsettled_base),
        CLOSE_LOCKED_BASE
    );
}

#[quasar_test]
fn close_market_user_refuses_a_non_owner(test: &mut Test) {
    let market = init_close_market(test);

    // The maker's account is closable, but the taker signing for it would
    // be paid the maker's rent.
    let close_ix = close_market_user(test, market, TAKER, MAKER);
    test.send(close_ix).fails_with(OrderBookError::Unauthorized);
    let maker_market_user = test.derive_pda(MarketUser::seeds(&market, &MAKER));
    assert_eq!(test.read::<MarketUser>(maker_market_user).owner, MAKER);
}
