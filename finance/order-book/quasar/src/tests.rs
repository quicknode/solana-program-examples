//! QuasarSVM integration tests. They drive the real program instructions
//! end-to-end: initialize a market, create users, place and cross orders,
//! settle, and withdraw fees, asserting on-chain state and token balances at
//! each step.
//!
//! Multi-step flows use `process_instruction_chain`, which runs several
//! instructions atomically over a shared, evolving account set - so a resting
//! order placed by one instruction is visible to the crossing order in the
//! next, without hand-building the zero-copy slab.

extern crate std;

use {
    alloc::vec,
    alloc::vec::Vec,
    quasar_svm::{Account, AccountMeta, Instruction, Pubkey, QuasarSvm},
    solana_program_pack::Pack,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::println,
};

use crate::state::{MARKET_SEED, MARKET_USER_SEED, ORDER_BOOK_ACCOUNT_SIZE, ORDER_SEED};

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

const STARTING_LAMPORTS: u64 = 1_000_000_000;

fn program_id() -> Pubkey {
    Pubkey::new_from_array(crate::ID.to_bytes())
}

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_order_book.so").unwrap();
    QuasarSvm::new()
        .with_program(&program_id(), &elf)
        .with_token_program()
}

fn rent_id() -> Pubkey {
    quasar_svm::solana_sdk_ids::sysvar::rent::ID
}
fn token_program_id() -> Pubkey {
    quasar_svm::SPL_TOKEN_PROGRAM_ID
}
fn system_program_id() -> Pubkey {
    quasar_svm::system_program::ID
}

fn signer_account(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, STARTING_LAMPORTS)
}

fn empty_account(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: system_program_id(),
        executable: false,
    }
}

fn mint_account(address: Pubkey, authority: Pubkey, decimals: u8) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &Mint {
            mint_authority: Some(authority).into(),
            supply: 1_000_000_000_000,
            decimals,
            is_initialized: true,
            freeze_authority: None.into(),
        },
    )
}

fn token_account(address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
    quasar_svm::token::create_keyed_token_account(
        &address,
        &TokenAccount {
            mint,
            owner,
            amount,
            state: AccountState::Initialized,
            ..TokenAccount::default()
        },
    )
}

/// A program-owned, zeroed order-book account of the exact size the program
/// expects. Stands in for the client's `create_account` call.
fn order_book_account(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 5_000_000_000,
        data: vec![0u8; ORDER_BOOK_ACCOUNT_SIZE],
        owner: program_id(),
        executable: false,
    }
}

fn token_amount(account: &Account) -> u64 {
    TokenAccount::unpack(&account.data).unwrap().amount
}

fn derive_market(base_mint: &Pubkey, quote_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[MARKET_SEED, base_mint.as_ref(), quote_mint.as_ref()],
        &program_id(),
    )
    .0
}

fn derive_market_user(market: &Pubkey, owner: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[MARKET_USER_SEED, market.as_ref(), owner.as_ref()],
        &program_id(),
    )
    .0
}

fn derive_order(market: &Pubkey, order_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[ORDER_SEED, market.as_ref(), &order_id.to_le_bytes()],
        &program_id(),
    )
    .0
}

// --- Instruction data builders (discriminator byte + little-endian args) ---

fn initialize_market_data() -> Vec<u8> {
    let mut data = vec![0u8];
    data.extend_from_slice(&FEE_BASIS_POINTS.to_le_bytes());
    data.extend_from_slice(&TICK_SIZE.to_le_bytes());
    data.extend_from_slice(&BASE_LOT_SIZE.to_le_bytes());
    data.extend_from_slice(&QUOTE_LOT_SIZE.to_le_bytes());
    data.extend_from_slice(&MIN_ORDER_SIZE.to_le_bytes());
    data
}

fn create_market_user_data() -> Vec<u8> {
    vec![1u8]
}

fn place_order_data(side: u8, price: u64, quantity: u64, order_id: u64) -> Vec<u8> {
    let mut data = vec![2u8, side];
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&quantity.to_le_bytes());
    data.extend_from_slice(&order_id.to_le_bytes());
    data
}

fn cancel_order_data() -> Vec<u8> {
    vec![3u8]
}

fn settle_funds_data() -> Vec<u8> {
    vec![4u8]
}

fn withdraw_fees_data() -> Vec<u8> {
    vec![5u8]
}

/// A market fixture: the mints, PDA, vaults, and order-book account for one
/// (base, quote) pair.
struct MarketFixture {
    authority: Pubkey,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    market: Pubkey,
    order_book: Pubkey,
    base_vault: Pubkey,
    quote_vault: Pubkey,
    fee_vault: Pubkey,
}

fn market_fixture() -> MarketFixture {
    let authority = Pubkey::new_unique();
    let base_mint = Pubkey::new_unique();
    let quote_mint = Pubkey::new_unique();
    MarketFixture {
        authority,
        base_mint,
        quote_mint,
        market: derive_market(&base_mint, &quote_mint),
        order_book: Pubkey::new_unique(),
        base_vault: Pubkey::new_unique(),
        quote_vault: Pubkey::new_unique(),
        fee_vault: Pubkey::new_unique(),
    }
}

fn initialize_market_ix(fx: &MarketFixture) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(fx.authority, true),
            AccountMeta::new(fx.market, false),
            AccountMeta::new(fx.order_book, false),
            AccountMeta::new_readonly(fx.base_mint, false),
            AccountMeta::new_readonly(fx.quote_mint, false),
            AccountMeta::new(fx.base_vault, true),
            AccountMeta::new(fx.quote_vault, true),
            AccountMeta::new(fx.fee_vault, true),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: initialize_market_data(),
    }
}

fn create_market_user_ix(fx: &MarketFixture, owner: Pubkey, market_user: Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(owner, true),
            AccountMeta::new_readonly(fx.market, false),
            AccountMeta::new(market_user, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: create_market_user_data(),
    }
}

/// One trader's per-market accounts.
struct Trader {
    owner: Pubkey,
    market_user: Pubkey,
    base_account: Pubkey,
    quote_account: Pubkey,
}

fn trader(fx: &MarketFixture) -> Trader {
    let owner = Pubkey::new_unique();
    Trader {
        owner,
        market_user: derive_market_user(&fx.market, &owner),
        base_account: Pubkey::new_unique(),
        quote_account: Pubkey::new_unique(),
    }
}

#[allow(clippy::too_many_arguments)]
fn place_order_ix(
    fx: &MarketFixture,
    trader: &Trader,
    order: Pubkey,
    side: u8,
    price: u64,
    quantity: u64,
    order_id: u64,
    makers: &[(Pubkey, Pubkey)],
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(fx.market, false),
        AccountMeta::new(fx.order_book, false),
        AccountMeta::new(order, false),
        AccountMeta::new(trader.market_user, false),
        AccountMeta::new(fx.base_vault, false),
        AccountMeta::new(fx.quote_vault, false),
        AccountMeta::new(fx.fee_vault, false),
        AccountMeta::new(trader.base_account, false),
        AccountMeta::new(trader.quote_account, false),
        AccountMeta::new_readonly(fx.base_mint, false),
        AccountMeta::new_readonly(fx.quote_mint, false),
        AccountMeta::new(trader.owner, true),
        AccountMeta::new_readonly(rent_id(), false),
        AccountMeta::new_readonly(token_program_id(), false),
        AccountMeta::new_readonly(system_program_id(), false),
    ];
    for (maker_order, maker_market_user) in makers {
        accounts.push(AccountMeta::new(*maker_order, false));
        accounts.push(AccountMeta::new(*maker_market_user, false));
    }
    Instruction {
        program_id: program_id(),
        accounts,
        data: place_order_data(side, price, quantity, order_id),
    }
}

fn cancel_order_ix(fx: &MarketFixture, trader: &Trader, order: Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(fx.market, false),
            AccountMeta::new(fx.order_book, false),
            AccountMeta::new(order, false),
            AccountMeta::new(trader.market_user, false),
            AccountMeta::new_readonly(trader.owner, true),
        ],
        data: cancel_order_data(),
    }
}

fn settle_funds_ix(fx: &MarketFixture, trader: &Trader) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(trader.owner, true),
            AccountMeta::new_readonly(fx.market, false),
            AccountMeta::new(trader.market_user, false),
            AccountMeta::new(fx.base_vault, false),
            AccountMeta::new(fx.quote_vault, false),
            AccountMeta::new(trader.base_account, false),
            AccountMeta::new(trader.quote_account, false),
            AccountMeta::new_readonly(fx.base_mint, false),
            AccountMeta::new_readonly(fx.quote_mint, false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data: settle_funds_data(),
    }
}

fn withdraw_fees_ix(fx: &MarketFixture, authority: Pubkey, authority_quote: Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(fx.market, false),
            AccountMeta::new(fx.fee_vault, false),
            AccountMeta::new(authority_quote, false),
            AccountMeta::new_readonly(fx.quote_mint, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data: withdraw_fees_data(),
    }
}

// --- Order-account field offsets (dense discriminator + fields, little-endian).
// disc(1) market(32) owner(32) order_id(8) side(1) price(8) original(8)
// filled(8) status(1) timestamp(8) bump(1). ---
const ORDER_STATUS_OFFSET: usize = 1 + 32 + 32 + 8 + 1 + 8 + 8 + 8;
const ORDER_FILLED_OFFSET: usize = 1 + 32 + 32 + 8 + 1 + 8 + 8;

fn order_status(account: &Account) -> u8 {
    account.data[ORDER_STATUS_OFFSET]
}
fn order_filled(account: &Account) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&account.data[ORDER_FILLED_OFFSET..ORDER_FILLED_OFFSET + 8]);
    u64::from_le_bytes(bytes)
}

// --- MarketUser field offsets. disc(1) market(32) owner(32) unsettled_base(8)
// unsettled_quote(8) open_orders_len(1) bump(1) open_orders(160). ---
const MU_UNSETTLED_BASE_OFFSET: usize = 1 + 32 + 32;
const MU_UNSETTLED_QUOTE_OFFSET: usize = 1 + 32 + 32 + 8;
const MU_OPEN_ORDERS_LEN_OFFSET: usize = 1 + 32 + 32 + 8 + 8;

fn mu_unsettled_base(account: &Account) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&account.data[MU_UNSETTLED_BASE_OFFSET..MU_UNSETTLED_BASE_OFFSET + 8]);
    u64::from_le_bytes(bytes)
}
fn mu_unsettled_quote(account: &Account) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&account.data[MU_UNSETTLED_QUOTE_OFFSET..MU_UNSETTLED_QUOTE_OFFSET + 8]);
    u64::from_le_bytes(bytes)
}
fn mu_open_orders_len(account: &Account) -> u8 {
    account.data[MU_OPEN_ORDERS_LEN_OFFSET]
}

// Order status codes (mirror state::OrderStatus).
const STATUS_FILLED: u8 = 2;
const STATUS_CANCELLED: u8 = 3;

#[test]
fn test_initialize_market() {
    let mut svm = setup();
    let fx = market_fixture();

    let accounts = vec![
        signer_account(fx.authority),
        empty_account(fx.market),
        order_book_account(fx.order_book),
        mint_account(fx.base_mint, fx.authority, BASE_DECIMALS),
        mint_account(fx.quote_mint, fx.authority, QUOTE_DECIMALS),
        empty_account(fx.base_vault),
        empty_account(fx.quote_vault),
        empty_account(fx.fee_vault),
    ];

    let result = svm.process_instruction(&initialize_market_ix(&fx), &accounts);
    assert!(result.is_ok(), "initialize_market failed: {:?}", result.raw_result);

    // Market discriminator (dense = 1) is stamped.
    let market = result.account(&fx.market).unwrap();
    assert_eq!(market.data[0], 1, "market discriminator");

    // Order-book discriminator + next_order_id == 1. Layout: disc(8) then
    // market(32), bids_root(8), asks_root(8), next_order_id(8)...
    let order_book = result.account(&fx.order_book).unwrap();
    assert_eq!(&order_book.data[0..8], b"ORDRBOOK", "order-book discriminator");
    let next_order_id_offset = 8 + 32 + 8 + 8;
    let mut id_bytes = [0u8; 8];
    id_bytes.copy_from_slice(&order_book.data[next_order_id_offset..next_order_id_offset + 8]);
    assert_eq!(u64::from_le_bytes(id_bytes), 1, "next_order_id starts at 1");

    println!("  INITIALIZE_MARKET CU: {}", result.compute_units_consumed);
}

#[test]
fn test_create_market_user() {
    let mut svm = setup();
    let fx = market_fixture();
    let maker = trader(&fx);

    let accounts = vec![
        signer_account(fx.authority),
        empty_account(fx.market),
        order_book_account(fx.order_book),
        mint_account(fx.base_mint, fx.authority, BASE_DECIMALS),
        mint_account(fx.quote_mint, fx.authority, QUOTE_DECIMALS),
        empty_account(fx.base_vault),
        empty_account(fx.quote_vault),
        empty_account(fx.fee_vault),
        signer_account(maker.owner),
        empty_account(maker.market_user),
    ];

    let instructions = vec![
        initialize_market_ix(&fx),
        create_market_user_ix(&fx, maker.owner, maker.market_user),
    ];
    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(result.is_ok(), "chain failed: {:?}", result.raw_result);

    let market_user = result.account(&maker.market_user).unwrap();
    assert_eq!(market_user.data[0], 2, "market_user discriminator");
    assert_eq!(mu_unsettled_base(market_user), 0);
    assert_eq!(mu_unsettled_quote(market_user), 0);
    assert_eq!(mu_open_orders_len(market_user), 0);

    println!("  CREATE_MARKET_USER CU: {}", result.compute_units_consumed);
}

/// Full lifecycle: a maker rests an ask, a taker bid crosses it fully, both
/// settle, and the authority withdraws the fee. Prices in the NVDAx/USDC lot
/// model: ask 5 lots @ 100 -> gross 500 quote, 1% fee = 5, maker nets 495
/// quote, taker receives 5000 raw base.
#[test]
fn test_place_match_settle_withdraw() {
    let mut svm = setup();
    let fx = market_fixture();
    let maker = trader(&fx);
    let taker = trader(&fx);
    let maker_order = derive_order(&fx.market, 1);
    let taker_order = derive_order(&fx.market, 2);
    let authority_quote = Pubkey::new_unique();

    // Maker sells 5 base lots (locks 5 * 1000 = 5000 raw base); taker buys 5
    // lots at 100 (locks 100 * 5 * 1 = 500 raw quote).
    const PRICE: u64 = 100;
    const QUANTITY: u64 = 5;
    const MAKER_BASE_LOCK: u64 = QUANTITY * BASE_LOT_SIZE; // 5000
    const TAKER_QUOTE_LOCK: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE; // 500
    const GROSS_QUOTE: u64 = PRICE * QUANTITY * QUOTE_LOT_SIZE; // 500
    const FEE_QUOTE: u64 = 5; // ceil(500 * 100 / 10000)
    const MAKER_NET_QUOTE: u64 = GROSS_QUOTE - FEE_QUOTE; // 495

    let accounts = vec![
        signer_account(fx.authority),
        empty_account(fx.market),
        order_book_account(fx.order_book),
        mint_account(fx.base_mint, fx.authority, BASE_DECIMALS),
        mint_account(fx.quote_mint, fx.authority, QUOTE_DECIMALS),
        empty_account(fx.base_vault),
        empty_account(fx.quote_vault),
        empty_account(fx.fee_vault),
        // maker
        signer_account(maker.owner),
        empty_account(maker.market_user),
        empty_account(maker_order),
        token_account(maker.base_account, fx.base_mint, maker.owner, MAKER_BASE_LOCK),
        token_account(maker.quote_account, fx.quote_mint, maker.owner, 0),
        // taker
        signer_account(taker.owner),
        empty_account(taker.market_user),
        empty_account(taker_order),
        token_account(taker.base_account, fx.base_mint, taker.owner, 0),
        token_account(taker.quote_account, fx.quote_mint, taker.owner, TAKER_QUOTE_LOCK),
        // fee withdrawal destination
        token_account(authority_quote, fx.quote_mint, fx.authority, 0),
    ];

    let instructions = vec![
        initialize_market_ix(&fx),
        create_market_user_ix(&fx, maker.owner, maker.market_user),
        create_market_user_ix(&fx, taker.owner, taker.market_user),
        // Maker ask (id 1) rests on the book.
        place_order_ix(&fx, &maker, maker_order, 1, PRICE, QUANTITY, 1, &[]),
        // Taker bid (id 2) crosses the maker ask; maker accounts supplied as
        // remaining accounts.
        place_order_ix(
            &fx,
            &taker,
            taker_order,
            0,
            PRICE,
            QUANTITY,
            2,
            &[(maker_order, maker.market_user)],
        ),
        // Both settle, then the authority sweeps the fee vault.
        settle_funds_ix(&fx, &maker),
        settle_funds_ix(&fx, &taker),
        withdraw_fees_ix(&fx, fx.authority, authority_quote),
    ];

    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(result.is_ok(), "lifecycle chain failed: {:?}", result.raw_result);

    // Both orders fully filled.
    assert_eq!(order_status(result.account(&maker_order).unwrap()), STATUS_FILLED);
    assert_eq!(order_filled(result.account(&maker_order).unwrap()), QUANTITY);
    assert_eq!(order_status(result.account(&taker_order).unwrap()), STATUS_FILLED);
    assert_eq!(order_filled(result.account(&taker_order).unwrap()), QUANTITY);

    // Maker's open-orders list emptied when its resting order fully filled.
    assert_eq!(mu_open_orders_len(result.account(&maker.market_user).unwrap()), 0);

    // Settlement moved tokens: maker received net quote, taker received base.
    assert_eq!(
        token_amount(result.account(&maker.quote_account).unwrap()),
        MAKER_NET_QUOTE
    );
    assert_eq!(
        token_amount(result.account(&taker.base_account).unwrap()),
        MAKER_BASE_LOCK
    );

    // Fee swept to the authority.
    assert_eq!(token_amount(result.account(&authority_quote).unwrap()), FEE_QUOTE);
    assert_eq!(token_amount(result.account(&fx.fee_vault).unwrap()), 0);

    // Vaults drained after settlement (maker sold all base, taker paid gross).
    assert_eq!(token_amount(result.account(&fx.base_vault).unwrap()), 0);
    assert_eq!(token_amount(result.account(&fx.quote_vault).unwrap()), 0);

    println!("  LIFECYCLE CU: {}", result.compute_units_consumed);
}

/// Cancelling a resting order credits the locked base back to the owner's
/// unsettled balance and marks the order cancelled.
#[test]
fn test_cancel_order() {
    let mut svm = setup();
    let fx = market_fixture();
    let maker = trader(&fx);
    let maker_order = derive_order(&fx.market, 1);

    const PRICE: u64 = 100;
    const QUANTITY: u64 = 5;
    const MAKER_BASE_LOCK: u64 = QUANTITY * BASE_LOT_SIZE;

    let accounts = vec![
        signer_account(fx.authority),
        empty_account(fx.market),
        order_book_account(fx.order_book),
        mint_account(fx.base_mint, fx.authority, BASE_DECIMALS),
        mint_account(fx.quote_mint, fx.authority, QUOTE_DECIMALS),
        empty_account(fx.base_vault),
        empty_account(fx.quote_vault),
        empty_account(fx.fee_vault),
        signer_account(maker.owner),
        empty_account(maker.market_user),
        empty_account(maker_order),
        token_account(maker.base_account, fx.base_mint, maker.owner, MAKER_BASE_LOCK),
        token_account(maker.quote_account, fx.quote_mint, maker.owner, 0),
    ];

    let instructions = vec![
        initialize_market_ix(&fx),
        create_market_user_ix(&fx, maker.owner, maker.market_user),
        place_order_ix(&fx, &maker, maker_order, 1, PRICE, QUANTITY, 1, &[]),
        cancel_order_ix(&fx, &maker, maker_order),
    ];

    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(result.is_ok(), "cancel chain failed: {:?}", result.raw_result);

    assert_eq!(order_status(result.account(&maker_order).unwrap()), STATUS_CANCELLED);

    // The locked base is credited back to the owner's unsettled balance and
    // the open-order slot is freed.
    let market_user = result.account(&maker.market_user).unwrap();
    assert_eq!(mu_unsettled_base(market_user), MAKER_BASE_LOCK);
    assert_eq!(mu_open_orders_len(market_user), 0);

    println!("  CANCEL_ORDER CU: {}", result.compute_units_consumed);
}

/// A non-authority signer cannot withdraw the fee vault.
#[test]
fn test_withdraw_fees_rejects_non_authority() {
    let mut svm = setup();
    let fx = market_fixture();
    let attacker = Pubkey::new_unique();
    let attacker_quote = Pubkey::new_unique();

    let init_accounts = vec![
        signer_account(fx.authority),
        empty_account(fx.market),
        order_book_account(fx.order_book),
        mint_account(fx.base_mint, fx.authority, BASE_DECIMALS),
        mint_account(fx.quote_mint, fx.authority, QUOTE_DECIMALS),
        empty_account(fx.base_vault),
        empty_account(fx.quote_vault),
        empty_account(fx.fee_vault),
        signer_account(attacker),
        token_account(attacker_quote, fx.quote_mint, attacker, 0),
    ];

    let instructions = vec![
        initialize_market_ix(&fx),
        withdraw_fees_ix(&fx, attacker, attacker_quote),
    ];
    let result = svm.process_instruction_chain(&instructions, &init_accounts);
    assert!(
        !result.is_ok(),
        "withdraw_fees must reject a signer who is not the market authority"
    );
}
