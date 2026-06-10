//! LiteSVM tests for the order-book program.
//!
//! Covers the full lifecycle that the program supports: initialise a market,
//! create user accounts, place bids/asks (locking the appropriate vault),
//! reject invalid prices / tick-aligned prices / undersized quantities,
//! cancel orders (which credits unsettled balances), settle funds out of
//! the vaults, and - in the matching block near the bottom - cross incoming
//! orders against resting orders using price-time priority, charge the
//! configured taker fee to a fee vault, and drain the fee vault via
//! `withdraw_fees`.

use {
    anchor_lang::{
        solana_program::{
            instruction::{AccountMeta, Instruction},
            pubkey::Pubkey,
            system_instruction, system_program,
        },
        Discriminator, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
    solana_signer::Signer,
};

// Keep test-side seeds in sync with `programs/order_book/src/state/*`. Duplicated
// rather than imported so tests stay self-contained and exercise the same
// byte strings a client SDK would use.
const MARKET_SEED: &[u8] = b"market";
const ORDER_SEED: &[u8] = b"order";
const MARKET_USER_SEED: &[u8] = b"market_user";

// Size of the zero-copy OrderBook account, including Anchor's 8-byte
// discriminator. Mirrors `order_book::state::ORDER_BOOK_ACCOUNT_SIZE` - duplicated
// here so tests are self-contained and stay closer to what an SDK does.
// Two 1024-leaf critbit slabs at 88 bytes per node, plus header. If you
// change this, bump the constant in `state/order_book.rs` too - the
// `#[account(zero)]` check fails if the account size is wrong.
const ORDER_BOOK_ACCOUNT_SIZE: u64 = order_book::state::ORDER_BOOK_ACCOUNT_SIZE as u64;

// NVDAx has 8 decimals onchain; USDC has 6.
const BASE_DECIMALS: u8 = 8; // NVDAx (https://explorer.solana.com/address/Xsc9qvGR1efVDFGLrVsmkzv3qi45LTBjeUKSPmx9qEh)
const QUOTE_DECIMALS: u8 = 6; // USDC

// Two-lot model for NVDAx/USDC (d_base=8, d_quote=6):
//   base_lot_size  = 10^max(8-6, 0) = 100   → 1 lot = 100 raw NVDAx
//   quote_lot_size = 10^max(6-8, 0) = 1     → 1 quote-lot = 1 raw USDC
//   raw_base  = quantity × 100
//   raw_quote = price    × quantity × 1    (= human USDC/share × lots)
//   tick_size = 1  →  $1.00 minimum price increment
const FEE_BASIS_POINTS: u16 = 10;
const TICK_SIZE: u64 = 1;
const BASE_LOT_SIZE: u64 = 100;
const QUOTE_LOT_SIZE: u64 = 1;
const MIN_ORDER_SIZE: u64 = 1;

// Funding for each trader's token accounts. Large enough to cover every
// order placed in the tests with room to spare.
const TRADER_STARTING_BALANCE: u64 = 1_000_000_000;

// Shared order sizing - chosen so price * quantity stays well inside u64
// and the seller's ask sits at the same price as the buyer's bid (matching
// is not implemented, they just coexist in the book).
const BID_PRICE: u64 = 100;
const BID_QUANTITY: u64 = 10;
const ASK_PRICE: u64 = 100;
const ASK_QUANTITY: u64 = 5;

fn token_program_id() -> Pubkey {
    // The program accepts either the Classic Token Program or the Token
    // Extensions Program via `TokenInterface`; we use the Classic Token
    // Program for tests because solana-kite's helpers create classic mints.
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

fn market_pda(program_id: &Pubkey, base_mint: &Pubkey, quote_mint: &Pubkey) -> Pubkey {
    let (market, _) = Pubkey::find_program_address(
        &[MARKET_SEED, base_mint.as_ref(), quote_mint.as_ref()],
        program_id,
    );
    market
}

fn market_user_pda(program_id: &Pubkey, market: &Pubkey, owner: &Pubkey) -> Pubkey {
    let (market_user, _) = Pubkey::find_program_address(
        &[MARKET_USER_SEED, market.as_ref(), owner.as_ref()],
        program_id,
    );
    market_user
}

fn order_pda(program_id: &Pubkey, market: &Pubkey, order_id: u64) -> Pubkey {
    let (order, _) = Pubkey::find_program_address(
        &[ORDER_SEED, market.as_ref(), &order_id.to_le_bytes()],
        program_id,
    );
    order
}

// ---------------------------------------------------------------------------
// Scenario: a market with a buyer and a seller, both funded in both mints.
// ---------------------------------------------------------------------------

struct Scenario {
    svm: LiteSVM,
    program_id: Pubkey,
    // `payer` funds the mint authority + ATA creations during setup but is
    // not used directly by the tests afterwards.
    #[allow(dead_code)]
    payer: Keypair,
    authority: Keypair,
    buyer: Keypair,
    seller: Keypair,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    base_vault: Keypair,
    quote_vault: Keypair,
    // Fees accumulate here (quote mint). Created fresh per Scenario; the
    // market PDA is the signer, same as the other two vaults.
    fee_vault: Keypair,
    market: Pubkey,
    // The order book is a ~180 KB zero-copy account owned by the program.
    // It's NOT a PDA - the BPF runtime caps inner-CPI allocations at 10 KB,
    // so the client must allocate it directly via system_program::CreateAccount
    // and pass it in as a signer. See `build_initialize_market_tx` for the
    // full setup.
    order_book: Keypair,
    buyer_base_ata: Pubkey,
    buyer_quote_ata: Pubkey,
    seller_base_ata: Pubkey,
    seller_quote_ata: Pubkey,
    buyer_market_user: Pubkey,
    seller_market_user: Pubkey,
}

fn full_setup() -> Scenario {
    let program_id = order_book::id();
    let mut svm = LiteSVM::new();
    let program_bytes = include_bytes!("../../../target/deploy/order_book.so");
    svm.add_program(program_id, program_bytes).unwrap();

    // 100 SOL for the payer is overkill, but rent + a few init-ATA hops add
    // up and a generous balance keeps setup logic simple.
    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
    let authority = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let buyer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let seller = create_wallet(&mut svm, 10_000_000_000).unwrap();

    let base_mint = create_token_mint(&mut svm, &authority, BASE_DECIMALS, None).unwrap();
    let quote_mint = create_token_mint(&mut svm, &authority, QUOTE_DECIMALS, None).unwrap();

    // Create and fund every trader's ATAs up-front so individual tests do
    // not need to worry about mint/ATA side effects, only about order-book state.
    let buyer_base_ata =
        create_associated_token_account(&mut svm, &buyer.pubkey(), &base_mint, &payer).unwrap();
    let buyer_quote_ata =
        create_associated_token_account(&mut svm, &buyer.pubkey(), &quote_mint, &payer).unwrap();
    let seller_base_ata =
        create_associated_token_account(&mut svm, &seller.pubkey(), &base_mint, &payer).unwrap();
    let seller_quote_ata =
        create_associated_token_account(&mut svm, &seller.pubkey(), &quote_mint, &payer).unwrap();

    mint_tokens_to_token_account(
        &mut svm,
        &base_mint,
        &seller_base_ata,
        TRADER_STARTING_BALANCE,
        &authority,
    )
    .unwrap();
    mint_tokens_to_token_account(
        &mut svm,
        &quote_mint,
        &buyer_quote_ata,
        TRADER_STARTING_BALANCE,
        &authority,
    )
    .unwrap();

    let market = market_pda(&program_id, &base_mint, &quote_mint);
    let buyer_market_user = market_user_pda(&program_id, &market, &buyer.pubkey());
    let seller_market_user = market_user_pda(&program_id, &market, &seller.pubkey());

    // Vaults are plain token accounts created in-line by initialize_market
    // (not PDAs). Tests generate fresh keypairs to serve as their addresses.
    let base_vault = Keypair::new();
    let quote_vault = Keypair::new();
    let fee_vault = Keypair::new();
    let order_book = Keypair::new();

    Scenario {
        svm,
        program_id,
        payer,
        authority,
        buyer,
        seller,
        base_mint,
        quote_mint,
        base_vault,
        quote_vault,
        fee_vault,
        market,
        order_book,
        buyer_base_ata,
        buyer_quote_ata,
        seller_base_ata,
        seller_quote_ata,
        buyer_market_user,
        seller_market_user,
    }
}

// ---------------------------------------------------------------------------
// Instruction builders - one per program entry point.
// ---------------------------------------------------------------------------

/// Build the `system_program::CreateAccount` instruction the client must run
/// to allocate the ~180 KB OrderBook account before calling
/// `initialize_market`. The new account is owned by the order-book program and
/// zero-initialized; the program then runs `load_init` against it in the
/// next instruction.
///
/// Rent is whatever LiteSVM's bank quotes for that size at the current
/// rent rate; we use `minimum_balance` so the account is rent-exempt.
fn build_create_order_book_account_ix(
    sc: &Scenario,
    payer: &Pubkey,
) -> Instruction {
    // LiteSVM uses the default rent schedule; minimum_balance() on the
    // 180 KB account is around 1.25 SOL - well within the 100 SOL we fund
    // the test payer with in `full_setup`.
    let rent_lamports = sc
        .svm
        .minimum_balance_for_rent_exemption(ORDER_BOOK_ACCOUNT_SIZE as usize);
    system_instruction::create_account(
        payer,
        &sc.order_book.pubkey(),
        rent_lamports,
        ORDER_BOOK_ACCOUNT_SIZE,
        &sc.program_id,
    )
}

fn build_initialize_market_ix(
    sc: &Scenario,
    fee_basis_points: u16,
    tick_size: u64,
    base_lot_size: u64,
    quote_lot_size: u64,
    min_order_size: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        sc.program_id,
        &order_book::instruction::InitializeMarket {
            fee_basis_points,
            tick_size,
            base_lot_size,
            quote_lot_size,
            min_order_size,
        }
        .data(),
        order_book::accounts::InitializeMarket {
            market: sc.market,
            order_book: sc.order_book.pubkey(),
            base_mint: sc.base_mint,
            quote_mint: sc.quote_mint,
            base_vault: sc.base_vault.pubkey(),
            quote_vault: sc.quote_vault.pubkey(),
            fee_vault: sc.fee_vault.pubkey(),
            authority: sc.authority.pubkey(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn build_create_market_user_ix(sc: &Scenario, owner: &Pubkey) -> Instruction {
    let market_user = market_user_pda(&sc.program_id, &sc.market, owner);
    Instruction::new_with_bytes(
        sc.program_id,
        &order_book::instruction::CreateMarketUser {}.data(),
        order_book::accounts::CreateMarketUser {
            market_user,
            market: sc.market,
            owner: *owner,
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_place_order_ix(
    sc: &Scenario,
    owner: &Keypair,
    market_user: Pubkey,
    user_base_account: Pubkey,
    user_quote_account: Pubkey,
    side: order_book::state::OrderSide,
    order_id: u64,
    price: u64,
    quantity: u64,
) -> Instruction {
    let order = order_pda(&sc.program_id, &sc.market, order_id);
    Instruction::new_with_bytes(
        sc.program_id,
        &order_book::instruction::PlaceOrder {
            side,
            price,
            quantity,
        }
        .data(),
        order_book::accounts::PlaceOrder {
            market: sc.market,
            order_book: sc.order_book.pubkey(),
            order,
            market_user,
            base_vault: sc.base_vault.pubkey(),
            quote_vault: sc.quote_vault.pubkey(),
            fee_vault: sc.fee_vault.pubkey(),
            user_base_account,
            user_quote_account,
            base_mint: sc.base_mint,
            quote_mint: sc.quote_mint,
            owner: owner.pubkey(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

/// Build a `place_order` instruction with maker (order, market_user) PDA
/// pairs appended as remaining accounts. The order-book program expects them in the same
/// order the resting book will be walked - best-priced first (lowest ask
/// for a taker bid, highest bid for a taker ask), and within a price level
/// earliest-first. Every maker pair must be writable: the program mutates
/// the maker's Order (filled_quantity, status) and their MarketUser
/// (unsettled_* and open_orders).
#[allow(clippy::too_many_arguments)]
fn build_place_order_with_makers_ix(
    sc: &Scenario,
    owner: &Keypair,
    market_user: Pubkey,
    user_base_account: Pubkey,
    user_quote_account: Pubkey,
    side: order_book::state::OrderSide,
    order_id: u64,
    price: u64,
    quantity: u64,
    maker_pairs: &[(u64, Pubkey)],
) -> Instruction {
    let mut ix = build_place_order_ix(
        sc,
        owner,
        market_user,
        user_base_account,
        user_quote_account,
        side,
        order_id,
        price,
        quantity,
    );

    for (maker_order_id, maker_market_user) in maker_pairs {
        let maker_order = order_pda(&sc.program_id, &sc.market, *maker_order_id);
        ix.accounts
            .push(AccountMeta::new(maker_order, false));
        ix.accounts
            .push(AccountMeta::new(*maker_market_user, false));
    }

    ix
}

fn build_withdraw_fees_ix(
    sc: &Scenario,
    authority_quote_account: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        sc.program_id,
        &order_book::instruction::WithdrawFees {}.data(),
        order_book::accounts::WithdrawFees {
            market: sc.market,
            fee_vault: sc.fee_vault.pubkey(),
            authority_quote_account,
            quote_mint: sc.quote_mint,
            authority: sc.authority.pubkey(),
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    )
}

fn build_cancel_order_ix(
    sc: &Scenario,
    owner: &Pubkey,
    market_user: Pubkey,
    order_id: u64,
) -> Instruction {
    let order = order_pda(&sc.program_id, &sc.market, order_id);
    Instruction::new_with_bytes(
        sc.program_id,
        &order_book::instruction::CancelOrder {}.data(),
        order_book::accounts::CancelOrder {
            market: sc.market,
            order_book: sc.order_book.pubkey(),
            order,
            market_user,
            owner: *owner,
        }
        .to_account_metas(None),
    )
}

fn build_settle_funds_ix(
    sc: &Scenario,
    owner: &Pubkey,
    market_user: Pubkey,
    user_base_account: Pubkey,
    user_quote_account: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        sc.program_id,
        &order_book::instruction::SettleFunds {}.data(),
        order_book::accounts::SettleFunds {
            market: sc.market,
            market_user,
            base_vault: sc.base_vault.pubkey(),
            quote_vault: sc.quote_vault.pubkey(),
            user_base_account,
            user_quote_account,
            base_mint: sc.base_mint,
            quote_mint: sc.quote_mint,
            owner: *owner,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    )
}

// Convenience: run `initialize_market` with the shared test parameters and
// both user-account creations so tests that just want a ready-to-trade
// market do not have to repeat the boilerplate.
fn initialize_market_and_users(sc: &mut Scenario) {
    // Allocate the OrderBook account first - it has to exist (owned by the
    // program, zero-initialized) before initialize_market's `#[account(zero)]`
    // check passes.
    let create_ix = build_create_order_book_account_ix(sc, &sc.authority.pubkey());
    let init_ix = build_initialize_market_ix(sc, FEE_BASIS_POINTS, TICK_SIZE, BASE_LOT_SIZE, QUOTE_LOT_SIZE, MIN_ORDER_SIZE);
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, init_ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    )
    .unwrap();

    let buyer_ix = build_create_market_user_ix(sc, &sc.buyer.pubkey());
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![buyer_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    let seller_ix = build_create_market_user_ix(sc, &sc.seller.pubkey());
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![seller_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn initialize_market_sets_market_and_order_book() {
    let mut sc = full_setup();

    let create_ix = build_create_order_book_account_ix(&sc, &sc.authority.pubkey());
    let ix = build_initialize_market_ix(&sc, FEE_BASIS_POINTS, TICK_SIZE, BASE_LOT_SIZE, QUOTE_LOT_SIZE, MIN_ORDER_SIZE);
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    )
    .unwrap();

    // The market PDA is owned by the program and non-empty.
    let market_account = sc.svm.get_account(&sc.market).expect("market PDA missing");
    assert_eq!(market_account.owner, sc.program_id);
    assert!(!market_account.data.is_empty());

    let order_book_account = sc
        .svm
        .get_account(&sc.order_book.pubkey())
        .expect("order book account missing");
    assert_eq!(order_book_account.owner, sc.program_id);
    // The 8-byte discriminator should now be set (init handler ran).
    assert_eq!(
        &order_book_account.data[..8],
        order_book::state::OrderBook::DISCRIMINATOR
    );

    // Vaults were created with the market as authority; easiest check is
    // simply that they exist with a zero balance.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.base_vault.pubkey()).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.quote_vault.pubkey()).unwrap(),
        0
    );
}

#[test]
fn create_market_user_tracks_market_and_owner() {
    let mut sc = full_setup();

    let create_ix = build_create_order_book_account_ix(&sc, &sc.authority.pubkey());
    let init_ix = build_initialize_market_ix(&sc, FEE_BASIS_POINTS, TICK_SIZE, BASE_LOT_SIZE, QUOTE_LOT_SIZE, MIN_ORDER_SIZE);
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, init_ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    )
    .unwrap();

    let create_ix = build_create_market_user_ix(&sc, &sc.buyer.pubkey());
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    let market_user = sc
        .svm
        .get_account(&sc.buyer_market_user)
        .expect("user account PDA missing");
    assert_eq!(market_user.owner, sc.program_id);
}

#[test]
fn place_bid_locks_quote_in_vault() {
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    // The first order ever placed gets id = 1 (see initialize_market.rs).
    let bid_order_id = 1u64;
    let ix = build_place_order_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        bid_order_id,
        BID_PRICE,
        BID_QUANTITY,
    );
    send_transaction_from_instructions(&mut sc.svm, vec![ix], &[&sc.buyer], &sc.buyer.pubkey())
        .unwrap();

    // A bid locks price * quantity * quote_lot_size raw quote tokens.
    let locked_quote = BID_PRICE * BID_QUANTITY * QUOTE_LOT_SIZE;
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.quote_vault.pubkey()).unwrap(),
        locked_quote
    );
    // Buyer's quote ATA dropped by exactly that.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.buyer_quote_ata).unwrap(),
        TRADER_STARTING_BALANCE - locked_quote
    );
    // Base vault untouched - bids never move base tokens.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.base_vault.pubkey()).unwrap(),
        0
    );

    // Order PDA exists and is owned by the program.
    let order_account = sc
        .svm
        .get_account(&order_pda(&sc.program_id, &sc.market, bid_order_id))
        .expect("order PDA missing");
    assert_eq!(order_account.owner, sc.program_id);
}

#[test]
fn place_ask_locks_base_in_vault() {
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    let ask_order_id = 1u64;
    let ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        ask_order_id,
        ASK_PRICE,
        ASK_QUANTITY,
    );
    send_transaction_from_instructions(&mut sc.svm, vec![ix], &[&sc.seller], &sc.seller.pubkey())
        .unwrap();

    // An ask locks quantity * base_lot_size raw base tokens in the base vault.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.base_vault.pubkey()).unwrap(),
        ASK_QUANTITY * BASE_LOT_SIZE
    );
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.seller_base_ata).unwrap(),
        TRADER_STARTING_BALANCE - ASK_QUANTITY * BASE_LOT_SIZE
    );
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.quote_vault.pubkey()).unwrap(),
        0
    );
}

#[test]
fn place_order_rejects_zero_price() {
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    let order_id = 1u64;
    let ix = build_place_order_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        order_id,
        // Price 0 trips InvalidPrice before tick-size is even considered.
        0,
        BID_QUANTITY,
    );
    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    );
    assert!(result.is_err(), "order at price 0 must be rejected");
}

#[test]
fn place_order_rejects_unaligned_tick() {
    let mut sc = full_setup();

    // Override default TICK_SIZE for this test so we can place a mis-aligned
    // price and see the tick check fire.
    let unusual_tick_size: u64 = 50;
    let create_ix = build_create_order_book_account_ix(&sc, &sc.authority.pubkey());
    let init_ix =
        build_initialize_market_ix(&sc, FEE_BASIS_POINTS, unusual_tick_size, BASE_LOT_SIZE, QUOTE_LOT_SIZE, MIN_ORDER_SIZE);
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, init_ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    )
    .unwrap();

    let create_ix = build_create_market_user_ix(&sc, &sc.buyer.pubkey());
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // 75 is not a multiple of 50 - must be rejected by the tick check.
    let unaligned_price: u64 = 75;
    let ix = build_place_order_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        1,
        unaligned_price,
        BID_QUANTITY,
    );
    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    );
    assert!(
        result.is_err(),
        "unaligned price must be rejected by tick check"
    );
}

#[test]
fn place_order_rejects_below_min_order_size() {
    let mut sc = full_setup();

    // Force a higher min_order_size so we can place an order below it.
    let elevated_min_order_size: u64 = 10;
    let create_ix = build_create_order_book_account_ix(&sc, &sc.authority.pubkey());
    let init_ix =
        build_initialize_market_ix(&sc, FEE_BASIS_POINTS, TICK_SIZE, BASE_LOT_SIZE, QUOTE_LOT_SIZE, elevated_min_order_size);
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, init_ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    )
    .unwrap();

    let create_ix = build_create_market_user_ix(&sc, &sc.seller.pubkey());
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    let too_small_quantity: u64 = 1;
    let ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        1,
        ASK_PRICE,
        too_small_quantity,
    );
    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    );
    assert!(
        result.is_err(),
        "quantity below min_order_size must be rejected"
    );
}

#[test]
fn cancel_ask_credits_unsettled_base() {
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    // Seller places an ask, then cancels it. The full locked base should be
    // credited to unsettled_base (no settlement yet).
    let ask_order_id = 1u64;
    let place_ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        ask_order_id,
        ASK_PRICE,
        ASK_QUANTITY,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![place_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    let cancel_ix = build_cancel_order_ix(
        &sc,
        &sc.seller.pubkey(),
        sc.seller_market_user,
        ask_order_id,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![cancel_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    // Funds are still in the vault - cancel does not move tokens, it only
    // updates the unsettled balance. Settlement is a separate step.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.base_vault.pubkey()).unwrap(),
        ASK_QUANTITY * BASE_LOT_SIZE
    );
    // Seller's ATA hasn't received anything back yet.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.seller_base_ata).unwrap(),
        TRADER_STARTING_BALANCE - ASK_QUANTITY * BASE_LOT_SIZE
    );
}

#[test]
fn cancel_order_rejects_non_owner() {
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    // Buyer places a bid; seller tries to cancel it using their own user
    // account. The program's `order.owner == signer` check must reject.
    let bid_order_id = 1u64;
    let place_ix = build_place_order_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        bid_order_id,
        BID_PRICE,
        BID_QUANTITY,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![place_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    let attack_ix = build_cancel_order_ix(
        &sc,
        &sc.seller.pubkey(),
        sc.seller_market_user,
        bid_order_id,
    );
    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![attack_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    );
    assert!(
        result.is_err(),
        "non-owner must not be able to cancel an order"
    );
}

#[test]
fn settle_funds_moves_unsettled_base_to_user() {
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    // Seller places + cancels an ask → credits unsettled_base.
    let ask_order_id = 1u64;
    let place_ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        ask_order_id,
        ASK_PRICE,
        ASK_QUANTITY,
    );
    let cancel_ix = build_cancel_order_ix(
        &sc,
        &sc.seller.pubkey(),
        sc.seller_market_user,
        ask_order_id,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![place_ix, cancel_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    let settle_ix = build_settle_funds_ix(
        &sc,
        &sc.seller.pubkey(),
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![settle_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    // Vault drained, seller got their base tokens back in full.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.base_vault.pubkey()).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.seller_base_ata).unwrap(),
        TRADER_STARTING_BALANCE
    );
}

#[test]
fn cancel_and_settle_bid_refunds_full_quote() {
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    let bid_order_id = 1u64;
    let place_ix = build_place_order_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        bid_order_id,
        BID_PRICE,
        BID_QUANTITY,
    );
    let cancel_ix = build_cancel_order_ix(
        &sc,
        &sc.buyer.pubkey(),
        sc.buyer_market_user,
        bid_order_id,
    );
    let settle_ix = build_settle_funds_ix(
        &sc,
        &sc.buyer.pubkey(),
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![place_ix, cancel_ix, settle_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // Vault drained, buyer got the full price*quantity of quote back.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.quote_vault.pubkey()).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.buyer_quote_ata).unwrap(),
        TRADER_STARTING_BALANCE
    );
}

// Regression test for the fee-drain attack on settle_funds. Pre-fix,
// `SettleFunds` did not bind `quote_vault` to `market.quote_vault` via
// `has_one`, so a caller could pass `market.fee_vault` (same mint and
// same authority) where `quote_vault` was expected and drain accumulated
// taker fees while spending their own unsettled_quote credit. The
// has_one constraint now bound on the `market` field must surface this
// as `ConstraintHasOne` (anchor error 2001) before any transfer runs.
#[test]
fn settle_funds_rejects_fee_vault_substituted_for_quote_vault() {
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    // Earn the buyer some unsettled_quote: place a bid (locks quote in
    // the real quote_vault) and immediately cancel it (credits the
    // buyer's unsettled_quote). settle_funds would normally drain the
    // quote_vault to the buyer's ATA.
    let bid_order_id = 1u64;
    let place_ix = build_place_order_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        bid_order_id,
        BID_PRICE,
        BID_QUANTITY,
    );
    let cancel_ix = build_cancel_order_ix(
        &sc,
        &sc.buyer.pubkey(),
        sc.buyer_market_user,
        bid_order_id,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![place_ix, cancel_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // Build a settle_funds ix but swap fee_vault in for quote_vault.
    // Everything else (base_vault, mints, user accounts, owner) stays
    // correct, so the only thing that should reject this is the new
    // has_one constraint on the market PDA.
    let attack_ix = Instruction::new_with_bytes(
        sc.program_id,
        &order_book::instruction::SettleFunds {}.data(),
        order_book::accounts::SettleFunds {
            market: sc.market,
            market_user: sc.buyer_market_user,
            base_vault: sc.base_vault.pubkey(),
            // Attack: route the quote-side transfer at the fee_vault.
            quote_vault: sc.fee_vault.pubkey(),
            user_base_account: sc.buyer_base_ata,
            user_quote_account: sc.buyer_quote_ata,
            base_mint: sc.base_mint,
            quote_mint: sc.quote_mint,
            owner: sc.buyer.pubkey(),
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );

    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![attack_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    );
    assert!(
        result.is_err(),
        "settle_funds must reject fee_vault substituted for quote_vault"
    );
}

#[test]
fn initialize_market_rejects_zero_tick_size() {
    let mut sc = full_setup();

    let zero_tick_size: u64 = 0;
    let create_ix = build_create_order_book_account_ix(&sc, &sc.authority.pubkey());
    let ix = build_initialize_market_ix(&sc, FEE_BASIS_POINTS, zero_tick_size, BASE_LOT_SIZE, QUOTE_LOT_SIZE, MIN_ORDER_SIZE);
    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    );
    assert!(result.is_err(), "tick_size == 0 must be rejected");
}

#[test]
fn initialize_market_rejects_zero_base_lot_size() {
    let mut sc = full_setup();

    let create_ix = build_create_order_book_account_ix(&sc, &sc.authority.pubkey());
    let ix = build_initialize_market_ix(&sc, FEE_BASIS_POINTS, TICK_SIZE, 0, QUOTE_LOT_SIZE, MIN_ORDER_SIZE);
    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    );
    assert!(result.is_err(), "base_lot_size == 0 must be rejected");
}

#[test]
fn initialize_market_rejects_zero_quote_lot_size() {
    let mut sc = full_setup();

    let create_ix = build_create_order_book_account_ix(&sc, &sc.authority.pubkey());
    let ix = build_initialize_market_ix(&sc, FEE_BASIS_POINTS, TICK_SIZE, BASE_LOT_SIZE, 0, MIN_ORDER_SIZE);
    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    );
    assert!(result.is_err(), "quote_lot_size == 0 must be rejected");
}

#[test]
fn initialize_market_rejects_oversized_fee() {
    let mut sc = full_setup();

    // 10_000 bps == 100% is the cap; anything higher must fail.
    let over_cap_fee_basis_points: u16 = 10_001;
    let create_ix = build_create_order_book_account_ix(&sc, &sc.authority.pubkey());
    let ix = build_initialize_market_ix(
        &sc,
        over_cap_fee_basis_points,
        TICK_SIZE,
        BASE_LOT_SIZE,
        QUOTE_LOT_SIZE,
        MIN_ORDER_SIZE,
    );
    let result = send_transaction_from_instructions(
        &mut sc.svm,
        vec![create_ix, ix],
        &[
            &sc.authority,
            &sc.order_book,
            &sc.base_vault,
            &sc.quote_vault,
            &sc.fee_vault,
        ],
        &sc.authority.pubkey(),
    );
    assert!(
        result.is_err(),
        "fee_basis_points above 10_000 must be rejected"
    );
}

// ---------------------------------------------------------------------------
// Matching-engine tests
//
// These exercise the price-time priority crossing logic added to place_order.
// Constants are named per-test (rather than shared at the top of the file)
// so each test reads self-contained and the maths is easy to follow.
// ---------------------------------------------------------------------------

// MarketUser field offsets after the 8-byte Anchor discriminator. Layout
// (see programs/order_book/src/state/market_user.rs):
//   market: Pubkey         (32)
//   owner:  Pubkey         (32)
//   unsettled_base: u64    (8)
//   unsettled_quote: u64   (8)
//   ...
// Borsh-decoding manually (rather than pulling MarketUser via try_from)
// keeps the tests readable and side-steps rent-checked deserialise paths.
const USER_ACCOUNT_UNSETTLED_BASE_OFFSET: usize = 8 + 32 + 32;
const USER_ACCOUNT_UNSETTLED_QUOTE_OFFSET: usize = USER_ACCOUNT_UNSETTLED_BASE_OFFSET + 8;

// Order layout after 8-byte discriminator (see state/order.rs):
//   market: Pubkey                      (32)
//   owner:  Pubkey                      (32)
//   order_id: u64                       (8)
//   side: u8 (Borsh-encoded enum tag)   (1)
//   price: u64                          (8)
//   original_quantity: u64              (8)
//   filled_quantity: u64                (8)
const ORDER_FILLED_QUANTITY_OFFSET: usize = 8 + 32 + 32 + 8 + 1 + 8 + 8;
const ORDER_STATUS_OFFSET: usize = ORDER_FILLED_QUANTITY_OFFSET + 8;
const ORDER_STATUS_OPEN: u8 = 0;
const ORDER_STATUS_PARTIALLY_FILLED: u8 = 1;
const ORDER_STATUS_FILLED: u8 = 2;

fn read_user_unsettled(svm: &LiteSVM, market_user: &Pubkey) -> (u64, u64) {
    let data = svm
        .get_account(market_user)
        .expect("user account missing")
        .data
        .clone();
    let base = u64::from_le_bytes(
        data[USER_ACCOUNT_UNSETTLED_BASE_OFFSET..USER_ACCOUNT_UNSETTLED_BASE_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let quote = u64::from_le_bytes(
        data[USER_ACCOUNT_UNSETTLED_QUOTE_OFFSET..USER_ACCOUNT_UNSETTLED_QUOTE_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    (base, quote)
}

fn read_order_fill_and_status(svm: &LiteSVM, order: &Pubkey) -> (u64, u8) {
    let data = svm
        .get_account(order)
        .expect("order account missing")
        .data
        .clone();
    let filled = u64::from_le_bytes(
        data[ORDER_FILLED_QUANTITY_OFFSET..ORDER_FILLED_QUANTITY_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let status = data[ORDER_STATUS_OFFSET];
    (filled, status)
}

#[test]
fn taker_bid_fully_crosses_best_ask() {
    // Seller rests an ask, buyer's bid fully eats it. Check base flows to
    // buyer's unsettled_base, quote net-of-fee flows to seller's
    // unsettled_quote, and fee_vault receives the expected bps.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    const MAKER_ASK_ID: u64 = 1;
    // 1000 * 100 = 100_000 quote flows, and 100_000 * 10 bps / 10_000 = 100
    // fee - big enough to be non-zero after integer division, tiny enough
    // that trader starting balances easily cover it.
    const PRICE: u64 = 1000;
    const QUANTITY: u64 = 100;
    const EXPECTED_GROSS_QUOTE: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE;
    const EXPECTED_FEE: u64 = EXPECTED_GROSS_QUOTE * FEE_BASIS_POINTS as u64 / 10_000;
    const EXPECTED_NET_TO_MAKER: u64 = EXPECTED_GROSS_QUOTE - EXPECTED_FEE;

    // Seller posts the resting ask.
    let maker_ask_ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        MAKER_ASK_ID,
        PRICE,
        QUANTITY,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![maker_ask_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    // Buyer's taker bid at the same price, same qty - fully crosses.
    const TAKER_BID_ID: u64 = 2;
    let taker_bid_ix = build_place_order_with_makers_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        TAKER_BID_ID,
        PRICE,
        QUANTITY,
        &[(MAKER_ASK_ID, sc.seller_market_user)],
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![taker_bid_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // Fee vault received exactly fee_bps of the gross.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.fee_vault.pubkey()).unwrap(),
        EXPECTED_FEE
    );

    let (buyer_base, buyer_quote) = read_user_unsettled(&sc.svm, &sc.buyer_market_user);
    assert_eq!(buyer_base, QUANTITY * BASE_LOT_SIZE);
    // No price improvement here - buyer's limit == maker's price - so no
    // quote rebate lands in the taker's unsettled_quote.
    assert_eq!(buyer_quote, 0);

    let (_seller_base, seller_quote) = read_user_unsettled(&sc.svm, &sc.seller_market_user);
    assert_eq!(seller_quote, EXPECTED_NET_TO_MAKER);

    // The resting maker order should have been removed from the book and
    // marked Filled.
    let maker_order = order_pda(&sc.program_id, &sc.market, MAKER_ASK_ID);
    let (filled, status) = read_order_fill_and_status(&sc.svm, &maker_order);
    assert_eq!(filled, QUANTITY);
    assert_eq!(status, ORDER_STATUS_FILLED);
}

#[test]
fn taker_ask_fully_crosses_best_bid() {
    // Mirror of the bid test. Buyer rests a bid, seller's ask fully eats it.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    const MAKER_BID_ID: u64 = 1;
    const PRICE: u64 = 1000;
    const QUANTITY: u64 = 100;
    const EXPECTED_GROSS_QUOTE: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE;
    const EXPECTED_FEE: u64 = EXPECTED_GROSS_QUOTE * FEE_BASIS_POINTS as u64 / 10_000;
    const EXPECTED_NET_TO_TAKER: u64 = EXPECTED_GROSS_QUOTE - EXPECTED_FEE;

    let maker_bid_ix = build_place_order_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        MAKER_BID_ID,
        PRICE,
        QUANTITY,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![maker_bid_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    const TAKER_ASK_ID: u64 = 2;
    let taker_ask_ix = build_place_order_with_makers_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        TAKER_ASK_ID,
        PRICE,
        QUANTITY,
        &[(MAKER_BID_ID, sc.buyer_market_user)],
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![taker_ask_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.fee_vault.pubkey()).unwrap(),
        EXPECTED_FEE
    );
    // Maker (buyer) received the base tokens they paid for.
    let (buyer_base, _buyer_quote) = read_user_unsettled(&sc.svm, &sc.buyer_market_user);
    assert_eq!(buyer_base, QUANTITY * BASE_LOT_SIZE);

    // Taker (seller) received the net-of-fee quote.
    let (_seller_base, seller_quote) = read_user_unsettled(&sc.svm, &sc.seller_market_user);
    assert_eq!(seller_quote, EXPECTED_NET_TO_TAKER);
}

#[test]
fn taker_partially_fills_resting_order_rest_stays_on_book() {
    // Seller rests ask qty=100. Buyer bids qty=40 at the same price.
    // The ask stays on the book with qty=60 remaining; the taker fully
    // matches and rests nothing.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    const MAKER_ASK_ID: u64 = 1;
    const MAKER_ASK_QUANTITY: u64 = 100;
    const TAKER_BID_QUANTITY: u64 = 40;
    const PRICE: u64 = 1000;

    let ask_ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        MAKER_ASK_ID,
        PRICE,
        MAKER_ASK_QUANTITY,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![ask_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    const TAKER_BID_ID: u64 = 2;
    let bid_ix = build_place_order_with_makers_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        TAKER_BID_ID,
        PRICE,
        TAKER_BID_QUANTITY,
        &[(MAKER_ASK_ID, sc.seller_market_user)],
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![bid_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // Maker order: still PartiallyFilled, filled_quantity == TAKER_BID_QUANTITY.
    let maker_order = order_pda(&sc.program_id, &sc.market, MAKER_ASK_ID);
    let (filled, status) = read_order_fill_and_status(&sc.svm, &maker_order);
    assert_eq!(filled, TAKER_BID_QUANTITY);
    assert_eq!(status, ORDER_STATUS_PARTIALLY_FILLED);

    // Base vault still holds the un-filled portion (seller's lock, minus
    // what was delivered to the taker's unsettled_base - which never left
    // the vault, just got re-tagged as owed to the buyer).
    //
    // Total base in vault stays == MAKER_ASK_QUANTITY * BASE_LOT_SIZE, because
    // fills are bucket-accounting inside the single vault.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.base_vault.pubkey()).unwrap(),
        MAKER_ASK_QUANTITY * BASE_LOT_SIZE
    );

    // Taker received TAKER_BID_QUANTITY lots = TAKER_BID_QUANTITY * BASE_LOT_SIZE raw base tokens.
    let (buyer_base, _) = read_user_unsettled(&sc.svm, &sc.buyer_market_user);
    assert_eq!(buyer_base, TAKER_BID_QUANTITY * BASE_LOT_SIZE);
}

#[test]
fn taker_partially_filled_remainder_rests_on_book() {
    // Seller rests ask qty=40. Buyer bids qty=100 at the same price.
    // Buyer eats the whole ask and the remaining 60 rests on the book as a
    // new bid.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    const MAKER_ASK_ID: u64 = 1;
    const MAKER_ASK_QUANTITY: u64 = 40;
    const TAKER_BID_QUANTITY: u64 = 100;
    const PRICE: u64 = 1000;

    let ask_ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        MAKER_ASK_ID,
        PRICE,
        MAKER_ASK_QUANTITY,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![ask_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    const TAKER_BID_ID: u64 = 2;
    let bid_ix = build_place_order_with_makers_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        TAKER_BID_ID,
        PRICE,
        TAKER_BID_QUANTITY,
        &[(MAKER_ASK_ID, sc.seller_market_user)],
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![bid_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // Maker ask is fully filled.
    let maker_order = order_pda(&sc.program_id, &sc.market, MAKER_ASK_ID);
    let (filled, status) = read_order_fill_and_status(&sc.svm, &maker_order);
    assert_eq!(filled, MAKER_ASK_QUANTITY);
    assert_eq!(status, ORDER_STATUS_FILLED);

    // Taker's own order is PartiallyFilled with `filled_quantity` equal
    // to what the maker supplied.
    let taker_order = order_pda(&sc.program_id, &sc.market, TAKER_BID_ID);
    let (taker_filled, taker_status) = read_order_fill_and_status(&sc.svm, &taker_order);
    assert_eq!(taker_filled, MAKER_ASK_QUANTITY);
    assert_eq!(taker_status, ORDER_STATUS_PARTIALLY_FILLED);

    // The taker's own Order PDA holds the true remaining-on-book quantity
    // (original_quantity - filled_quantity). On-book quantity isn't stored
    // on OrderEntry directly - see state/order_book.rs - so this is the
    // source of truth both here and at runtime.
    assert_eq!(
        TAKER_BID_QUANTITY - taker_filled,
        TAKER_BID_QUANTITY - MAKER_ASK_QUANTITY
    );
}

#[test]
fn taker_crosses_multiple_resting_orders_best_price_first() {
    // Two resting asks at different prices: 900 and 1000. A taker bid big
    // enough to chew through both must hit 900 first (best price for the
    // taker), then 1000.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    const BEST_ASK_ID: u64 = 1;
    const BEST_ASK_PRICE: u64 = 900;
    const BEST_ASK_QUANTITY: u64 = 30;

    const SECOND_ASK_ID: u64 = 2;
    const SECOND_ASK_PRICE: u64 = 1000;
    const SECOND_ASK_QUANTITY: u64 = 50;

    // Taker bids at the worse of the two resting prices so both cross.
    const TAKER_BID_ID: u64 = 3;
    const TAKER_BID_PRICE: u64 = 1000;
    const TAKER_BID_QUANTITY: u64 = BEST_ASK_QUANTITY + SECOND_ASK_QUANTITY;

    // Need to post both asks and both rest - seller places two in sequence.
    let ask_one_ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        BEST_ASK_ID,
        BEST_ASK_PRICE,
        BEST_ASK_QUANTITY,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![ask_one_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();
    let ask_two_ix = build_place_order_ix(
        &sc,
        &sc.seller,
        sc.seller_market_user,
        sc.seller_base_ata,
        sc.seller_quote_ata,
        order_book::state::OrderSide::Ask,
        SECOND_ASK_ID,
        SECOND_ASK_PRICE,
        SECOND_ASK_QUANTITY,
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![ask_two_ix],
        &[&sc.seller],
        &sc.seller.pubkey(),
    )
    .unwrap();

    // Taker walks in book order: best ask (900) then second (1000).
    let taker_ix = build_place_order_with_makers_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        TAKER_BID_ID,
        TAKER_BID_PRICE,
        TAKER_BID_QUANTITY,
        &[
            (BEST_ASK_ID, sc.seller_market_user),
            (SECOND_ASK_ID, sc.seller_market_user),
        ],
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![taker_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // Both resting asks are fully filled.
    let order_one = order_pda(&sc.program_id, &sc.market, BEST_ASK_ID);
    let order_two = order_pda(&sc.program_id, &sc.market, SECOND_ASK_ID);
    assert_eq!(read_order_fill_and_status(&sc.svm, &order_one).1, ORDER_STATUS_FILLED);
    assert_eq!(read_order_fill_and_status(&sc.svm, &order_two).1, ORDER_STATUS_FILLED);

    // Taker got TAKER_BID_QUANTITY lots = TAKER_BID_QUANTITY * BASE_LOT_SIZE raw base tokens.
    let (buyer_base, buyer_quote_rebate) = read_user_unsettled(&sc.svm, &sc.buyer_market_user);
    assert_eq!(buyer_base, TAKER_BID_QUANTITY * BASE_LOT_SIZE);

    // Price-improvement rebate: taker locked at 1000/unit but 30 units
    // filled at 900. Rebate = (1000 - 900) * 30 * quote_lot_size.
    const PRICE_IMPROVEMENT_REBATE: u64 = (TAKER_BID_PRICE - BEST_ASK_PRICE) * BEST_ASK_QUANTITY * QUOTE_LOT_SIZE;
    assert_eq!(buyer_quote_rebate, PRICE_IMPROVEMENT_REBATE);

    // Seller's net unsettled_quote = sum of (fill_price * fill_qty * quote_lot_size - fee).
    let gross_one: u64 = BEST_ASK_PRICE * BEST_ASK_QUANTITY * QUOTE_LOT_SIZE;
    let gross_two: u64 = SECOND_ASK_PRICE * SECOND_ASK_QUANTITY * QUOTE_LOT_SIZE;
    let fee_one: u64 = gross_one * FEE_BASIS_POINTS as u64 / 10_000;
    let fee_two: u64 = gross_two * FEE_BASIS_POINTS as u64 / 10_000;
    let expected_seller_quote = (gross_one - fee_one) + (gross_two - fee_two);
    let (_, seller_quote) = read_user_unsettled(&sc.svm, &sc.seller_market_user);
    assert_eq!(seller_quote, expected_seller_quote);
}

#[test]
fn resting_orders_at_same_price_fill_by_time_priority() {
    // Two resting asks at price 1000: first from seller, then from a second
    // seller. Taker only buys enough to cross the first one. The second
    // must stay on the book untouched.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    // Bootstrap a third wallet (second seller) with base tokens.
    let second_seller = create_wallet(&mut sc.svm, 10_000_000_000).unwrap();
    let second_seller_base_ata = create_associated_token_account(
        &mut sc.svm,
        &second_seller.pubkey(),
        &sc.base_mint,
        &sc.payer,
    )
    .unwrap();
    let second_seller_quote_ata = create_associated_token_account(
        &mut sc.svm,
        &second_seller.pubkey(),
        &sc.quote_mint,
        &sc.payer,
    )
    .unwrap();
    mint_tokens_to_token_account(
        &mut sc.svm,
        &sc.base_mint,
        &second_seller_base_ata,
        TRADER_STARTING_BALANCE,
        &sc.authority,
    )
    .unwrap();
    let second_seller_market_user = market_user_pda(&sc.program_id, &sc.market, &second_seller.pubkey());
    let __ix1 = build_create_market_user_ix(&sc, &second_seller.pubkey());
    send_transaction_from_instructions(&mut sc.svm, vec![__ix1], &[&second_seller],
        &second_seller.pubkey()).unwrap();

    const FIRST_ASK_ID: u64 = 1;
    const SECOND_ASK_ID: u64 = 2;
    const ASK_PRICE_SHARED: u64 = 1000;
    const ASK_QUANTITY_EACH: u64 = 20;

    // Seller 1 first in.
    let __ix2 = build_place_order_ix(
            &sc,
            &sc.seller,
            sc.seller_market_user,
            sc.seller_base_ata,
            sc.seller_quote_ata,
            order_book::state::OrderSide::Ask,
            FIRST_ASK_ID,
            ASK_PRICE_SHARED,
            ASK_QUANTITY_EACH,
        );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix2], &[&sc.seller],
        &sc.seller.pubkey()).unwrap();
    // Seller 2 second in at the same price.
    let __ix3 = build_place_order_ix(
            &sc,
            &second_seller,
            second_seller_market_user,
            second_seller_base_ata,
            second_seller_quote_ata,
            order_book::state::OrderSide::Ask,
            SECOND_ASK_ID,
            ASK_PRICE_SHARED,
            ASK_QUANTITY_EACH,
        );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix3], &[&second_seller],
        &second_seller.pubkey()).unwrap();

    // Taker bid buys only enough to cross seller 1's ask.
    const TAKER_BID_ID: u64 = 3;
    let taker_ix = build_place_order_with_makers_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        TAKER_BID_ID,
        ASK_PRICE_SHARED,
        ASK_QUANTITY_EACH,
        &[(FIRST_ASK_ID, sc.seller_market_user)],
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![taker_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // Time priority: seller 1 filled, seller 2 still open.
    let order_one = order_pda(&sc.program_id, &sc.market, FIRST_ASK_ID);
    let order_two = order_pda(&sc.program_id, &sc.market, SECOND_ASK_ID);
    assert_eq!(read_order_fill_and_status(&sc.svm, &order_one).1, ORDER_STATUS_FILLED);
    assert_eq!(read_order_fill_and_status(&sc.svm, &order_two).1, ORDER_STATUS_OPEN);
}

#[test]
fn taker_bid_gets_price_improvement_from_resting_ask() {
    // Taker limit 1000, resting ask at 900. Taker pays 900 (maker's price),
    // gets 100-per-unit price improvement rebated to their unsettled_quote.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    const MAKER_ASK_ID: u64 = 1;
    const MAKER_ASK_PRICE: u64 = 900;
    const TAKER_BID_PRICE: u64 = 1000;
    const QUANTITY: u64 = 50;

    // Maker ask.
    let __ix4 = build_place_order_ix(
            &sc,
            &sc.seller,
            sc.seller_market_user,
            sc.seller_base_ata,
            sc.seller_quote_ata,
            order_book::state::OrderSide::Ask,
            MAKER_ASK_ID,
            MAKER_ASK_PRICE,
            QUANTITY,
        );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix4], &[&sc.seller],
        &sc.seller.pubkey()).unwrap();

    // Taker bid - limit 1000.
    const TAKER_BID_ID: u64 = 2;
    let taker_ix = build_place_order_with_makers_ix(
        &sc,
        &sc.buyer,
        sc.buyer_market_user,
        sc.buyer_base_ata,
        sc.buyer_quote_ata,
        order_book::state::OrderSide::Bid,
        TAKER_BID_ID,
        TAKER_BID_PRICE,
        QUANTITY,
        &[(MAKER_ASK_ID, sc.seller_market_user)],
    );
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![taker_ix],
        &[&sc.buyer],
        &sc.buyer.pubkey(),
    )
    .unwrap();

    // Maker got 900-per-unit (minus fee), not 1000.
    let gross_to_maker: u64 = MAKER_ASK_PRICE * QUANTITY * QUOTE_LOT_SIZE;
    let fee: u64 = gross_to_maker * FEE_BASIS_POINTS as u64 / 10_000;
    let expected_net_to_maker: u64 = gross_to_maker - fee;
    let (_, seller_quote) = read_user_unsettled(&sc.svm, &sc.seller_market_user);
    assert_eq!(seller_quote, expected_net_to_maker);

    // Taker locked (TAKER_BID_PRICE * QUANTITY * QUOTE_LOT_SIZE) up front;
    // only (MAKER_ASK_PRICE * QUANTITY * QUOTE_LOT_SIZE) was spent.
    let expected_rebate: u64 = (TAKER_BID_PRICE - MAKER_ASK_PRICE) * QUANTITY * QUOTE_LOT_SIZE;
    let (buyer_base, buyer_quote) = read_user_unsettled(&sc.svm, &sc.buyer_market_user);
    assert_eq!(buyer_base, QUANTITY * BASE_LOT_SIZE);
    assert_eq!(buyer_quote, expected_rebate);
}

#[test]
fn fee_vault_receives_exactly_bps_of_taker_gross() {
    // Simpler standalone check of the fee maths: fee_vault must equal
    // (taker gross quote) * fee_bps / 10_000 after a single fill.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    const MAKER_ASK_ID: u64 = 1;
    const PRICE: u64 = 500;
    const QUANTITY: u64 = 200;
    const GROSS: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE;
    const EXPECTED_FEE: u64 = GROSS * FEE_BASIS_POINTS as u64 / 10_000;

    let __ix5 = build_place_order_ix(
            &sc,
            &sc.seller,
            sc.seller_market_user,
            sc.seller_base_ata,
            sc.seller_quote_ata,
            order_book::state::OrderSide::Ask,
            MAKER_ASK_ID,
            PRICE,
            QUANTITY,
        );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix5], &[&sc.seller],
        &sc.seller.pubkey()).unwrap();

    const TAKER_BID_ID: u64 = 2;
    let __ix6 = build_place_order_with_makers_ix(
            &sc,
            &sc.buyer,
            sc.buyer_market_user,
            sc.buyer_base_ata,
            sc.buyer_quote_ata,
            order_book::state::OrderSide::Bid,
            TAKER_BID_ID,
            PRICE,
            QUANTITY,
            &[(MAKER_ASK_ID, sc.seller_market_user)],
    );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix6], &[&sc.buyer], &sc.buyer.pubkey()).unwrap();


    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.fee_vault.pubkey()).unwrap(),
        EXPECTED_FEE
    );
}

#[test]
fn authority_can_withdraw_fees_after_match() {
    // Run a fill, confirm fee vault has something, withdraw to authority.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    // Authority needs a quote ATA to receive the withdrawn fees.
    let authority_quote_ata = create_associated_token_account(
        &mut sc.svm,
        &sc.authority.pubkey(),
        &sc.quote_mint,
        &sc.payer,
    )
    .unwrap();

    const MAKER_ASK_ID: u64 = 1;
    const PRICE: u64 = 2000;
    const QUANTITY: u64 = 50;
    const GROSS: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE;
    const EXPECTED_FEE: u64 = GROSS * FEE_BASIS_POINTS as u64 / 10_000;

    let __ix7 = build_place_order_ix(
            &sc,
            &sc.seller,
            sc.seller_market_user,
            sc.seller_base_ata,
            sc.seller_quote_ata,
            order_book::state::OrderSide::Ask,
            MAKER_ASK_ID,
            PRICE,
            QUANTITY,
        );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix7], &[&sc.seller],
        &sc.seller.pubkey()).unwrap();

    const TAKER_BID_ID: u64 = 2;
    let __ix8 = build_place_order_with_makers_ix(
            &sc,
            &sc.buyer,
            sc.buyer_market_user,
            sc.buyer_base_ata,
            sc.buyer_quote_ata,
            order_book::state::OrderSide::Bid,
            TAKER_BID_ID,
            PRICE,
            QUANTITY,
            &[(MAKER_ASK_ID, sc.seller_market_user)],
    );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix8], &[&sc.buyer], &sc.buyer.pubkey()).unwrap();


    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.fee_vault.pubkey()).unwrap(),
        EXPECTED_FEE
    );

    let withdraw_ix = build_withdraw_fees_ix(&sc, authority_quote_ata);
    send_transaction_from_instructions(
        &mut sc.svm,
        vec![withdraw_ix],
        &[&sc.authority],
        &sc.authority.pubkey(),
    )
    .unwrap();

    // Fee vault drained, authority received the fees.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.fee_vault.pubkey()).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&sc.svm, &authority_quote_ata).unwrap(),
        EXPECTED_FEE
    );
}

#[test]
fn settle_funds_after_match_pays_out_both_unsettled_balances() {
    // End-to-end: match, then call settle_funds for both sides. Both
    // traders must receive the tokens the match credited to their
    // unsettled_* balances.
    let mut sc = full_setup();
    initialize_market_and_users(&mut sc);

    const MAKER_ASK_ID: u64 = 1;
    const PRICE: u64 = 1000;
    const QUANTITY: u64 = 100;
    const GROSS: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE;
    const EXPECTED_FEE: u64 = GROSS * FEE_BASIS_POINTS as u64 / 10_000;
    const EXPECTED_NET_QUOTE_TO_SELLER: u64 = GROSS - EXPECTED_FEE;

    // Maker posts and taker crosses.
    let __ix9 = build_place_order_ix(
            &sc,
            &sc.seller,
            sc.seller_market_user,
            sc.seller_base_ata,
            sc.seller_quote_ata,
            order_book::state::OrderSide::Ask,
            MAKER_ASK_ID,
            PRICE,
            QUANTITY,
        );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix9], &[&sc.seller],
        &sc.seller.pubkey()).unwrap();
    const TAKER_BID_ID: u64 = 2;
    let __ix10 = build_place_order_with_makers_ix(
            &sc,
            &sc.buyer,
            sc.buyer_market_user,
            sc.buyer_base_ata,
            sc.buyer_quote_ata,
            order_book::state::OrderSide::Bid,
            TAKER_BID_ID,
            PRICE,
            QUANTITY,
            &[(MAKER_ASK_ID, sc.seller_market_user)],
    );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix10], &[&sc.buyer], &sc.buyer.pubkey()).unwrap();


    // Settle both sides.
    let __ix11 = build_settle_funds_ix(
            &sc,
            &sc.buyer.pubkey(),
            sc.buyer_market_user,
            sc.buyer_base_ata,
            sc.buyer_quote_ata,
        );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix11], &[&sc.buyer],
        &sc.buyer.pubkey()).unwrap();
    let __ix12 = build_settle_funds_ix(
            &sc,
            &sc.seller.pubkey(),
            sc.seller_market_user,
            sc.seller_base_ata,
            sc.seller_quote_ata,
        );
    send_transaction_from_instructions(&mut sc.svm, vec![__ix12], &[&sc.seller],
        &sc.seller.pubkey()).unwrap();

    // Buyer should now hold `QUANTITY` extra base tokens and have paid the
    // gross quote (starting balance minus gross). No price improvement
    // here, so nothing else to refund.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.buyer_base_ata).unwrap(),
        QUANTITY
    );
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.buyer_quote_ata).unwrap(),
        TRADER_STARTING_BALANCE - GROSS
    );

    // Seller should now hold (starting - QUANTITY) base and
    // EXPECTED_NET_QUOTE_TO_SELLER quote.
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.seller_base_ata).unwrap(),
        TRADER_STARTING_BALANCE - QUANTITY
    );
    assert_eq!(
        get_token_account_balance(&sc.svm, &sc.seller_quote_ata).unwrap(),
        EXPECTED_NET_QUOTE_TO_SELLER
    );
}

