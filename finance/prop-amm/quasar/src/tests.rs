extern crate std;

use {
    alloc::{vec, vec::Vec},
    quasar_svm::{
        token::{
            create_keyed_associated_token_account, create_keyed_mint_account,
            create_keyed_system_account, Mint,
        },
        Account, AccountMeta, Instruction, Pubkey, QuasarSvm,
    },
    std::fs,
};

// Both tokens have 6 decimals: base is NVDAx (tokenized NVIDIA stock), quote
// is USDC.
const ONE_TOKEN: u64 = 1_000_000;
// The oracle quotes prices with 8 decimals, so $165 is 165 * 10^8.
const ORACLE_SCALE: u32 = 8;
const SPREAD_BPS: u16 = 10;
const MAX_CONFIDENCE_BPS: u16 = 100;

const DIRECTION_BUY_BASE: u8 = 0;
const DIRECTION_SELL_BASE: u8 = 1;

// A fixed current slot well above the staleness bound, so tests can write
// feed accounts that are fresh (slot = SLOT) or stale (slot older than the
// 150-slot bound) without warping the SVM clock.
const SLOT: u64 = 1_000;

fn program_id() -> Pubkey {
    crate::ID.into()
}
fn token_program() -> Pubkey {
    quasar_svm::SPL_TOKEN_PROGRAM_ID
}
fn ata_program() -> Pubkey {
    quasar_svm::SPL_ASSOCIATED_TOKEN_PROGRAM_ID
}
fn system_program() -> Pubkey {
    quasar_svm::system_program::ID
}
fn clock_sysvar() -> Pubkey {
    "SysvarC1ock11111111111111111111111111111111"
        .parse()
        .unwrap()
}
fn rent_sysvar() -> Pubkey {
    "SysvarRent111111111111111111111111111111111"
        .parse()
        .unwrap()
}

fn dollars(whole: i128) -> i128 {
    whole * 10i128.pow(ORACLE_SCALE)
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &program_id()).0
}
fn ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), token_program().as_ref(), mint.as_ref()],
        &ata_program(),
    )
    .0
}

fn empty(address: &Pubkey) -> Account {
    Account {
        address: *address,
        lamports: 0,
        data: vec![],
        owner: system_program(),
        executable: false,
    }
}

fn mint_account(address: &Pubkey) -> Account {
    create_keyed_mint_account(
        address,
        &Mint {
            decimals: 6,
            is_initialized: true,
            ..Default::default()
        },
    )
}

/// A feed account in this program's layout: price (i128), scale (u32),
/// last_update_slot (u64), confidence (u64). The tests own this; production
/// reads a real feed.
fn feed_account(address: &Pubkey, price: i128, scale: u32, slot: u64, confidence: u64) -> Account {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&scale.to_le_bytes());
    data.extend_from_slice(&slot.to_le_bytes());
    data.extend_from_slice(&confidence.to_le_bytes());
    Account {
        address: *address,
        lamports: 1_000_000,
        data,
        owner: system_program(),
        executable: false,
    }
}

fn token_amount(svm: &QuasarSvm, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).expect("token account exists");
    // SPL token account layout: mint (32) + owner (32) + amount (u64) at offset 64.
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

struct Env {
    svm: QuasarSvm,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    feed: Pubkey,
    operator: Pubkey,
    market: Pubkey,
    market_authority: Pubkey,
    base_vault: Pubkey,
    quote_vault: Pubkey,
}

/// Build an SVM with the program, both mints, an oracle feed at $165, an
/// initialized market at a 10 bps spread, and 1,000 NVDAx + 200,000 USDC of
/// operator inventory deposited.
fn setup() -> Env {
    let mut env = try_setup(SPREAD_BPS).expect("market initialization should succeed");
    assert!(env.deposit_inventory_as_operator(1_000 * ONE_TOKEN, 200_000 * ONE_TOKEN));
    env
}

/// Like `setup`, but without the inventory deposit and with the spread exposed
/// so tests can probe the parameter validation.
fn try_setup(spread_bps: u16) -> Result<Env, ()> {
    let elf = fs::read("target/deploy/quasar_prop_amm.so").unwrap();
    let base_mint = Pubkey::new_unique();
    let quote_mint = Pubkey::new_unique();
    let feed = Pubkey::new_unique();
    let operator = Pubkey::new_unique();
    let market = pda(&[b"market", base_mint.as_ref(), quote_mint.as_ref()]);
    let market_authority = pda(&[b"authority", market.as_ref()]);
    let base_vault = pda(&[b"base_vault", market.as_ref()]);
    let quote_vault = pda(&[b"quote_vault", market.as_ref()]);

    let mut svm = QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program()
        .with_slot(SLOT)
        .with_account(mint_account(&base_mint))
        .with_account(mint_account(&quote_mint))
        .with_account(feed_account(&feed, dollars(165), ORACLE_SCALE, SLOT, 0))
        .with_account(create_keyed_system_account(&operator, 100_000_000_000));

    // The operator's own inventory accounts, funded.
    svm.set_account(create_keyed_associated_token_account(
        &operator,
        &base_mint,
        10_000 * ONE_TOKEN,
    ));
    svm.set_account(create_keyed_associated_token_account(
        &operator,
        &quote_mint,
        10_000_000 * ONE_TOKEN,
    ));

    // initialize_market
    let mut data = vec![0u8];
    data.extend_from_slice(&ORACLE_SCALE.to_le_bytes());
    data.extend_from_slice(&spread_bps.to_le_bytes());
    data.extend_from_slice(&MAX_CONFIDENCE_BPS.to_le_bytes());
    let metas = vec![
        AccountMeta::new(operator, true),
        AccountMeta::new(market, false),
        AccountMeta::new_readonly(base_mint, false),
        AccountMeta::new_readonly(quote_mint, false),
        AccountMeta::new_readonly(feed, false),
        AccountMeta::new_readonly(market_authority, false),
        AccountMeta::new(base_vault, false),
        AccountMeta::new(quote_vault, false),
        AccountMeta::new_readonly(token_program(), false),
        AccountMeta::new_readonly(system_program(), false),
        AccountMeta::new_readonly(rent_sysvar(), false),
    ];
    let provided = vec![
        svm.get_account(&operator).unwrap(),
        empty(&market),
        svm.get_account(&base_mint).unwrap(),
        svm.get_account(&quote_mint).unwrap(),
        empty(&base_vault),
        empty(&quote_vault),
    ];
    let result = svm.process_instruction(
        &Instruction {
            program_id: program_id(),
            accounts: metas,
            data,
        },
        &provided,
    );
    if !result.is_ok() {
        return Err(());
    }

    Ok(Env {
        svm,
        base_mint,
        quote_mint,
        feed,
        operator,
        market,
        market_authority,
        base_vault,
        quote_vault,
    })
}

impl Env {
    /// Create a wallet with funded base and quote token accounts.
    fn funded_wallet(&mut self, base: u64, quote: u64) -> Pubkey {
        let wallet = Pubkey::new_unique();
        self.svm
            .set_account(create_keyed_system_account(&wallet, 100_000_000_000));
        self.svm.set_account(create_keyed_associated_token_account(
            &wallet,
            &self.base_mint,
            base,
        ));
        self.svm.set_account(create_keyed_associated_token_account(
            &wallet,
            &self.quote_mint,
            quote,
        ));
        wallet
    }

    fn set_price(&mut self, price: i128) {
        self.svm
            .set_account(feed_account(&self.feed, price, ORACLE_SCALE, SLOT, 0));
    }

    fn set_price_with_confidence(&mut self, price: i128, confidence: u64) {
        self.svm.set_account(feed_account(
            &self.feed,
            price,
            ORACLE_SCALE,
            SLOT,
            confidence,
        ));
    }

    /// Write the feed with an update slot older than the 150-slot staleness
    /// bound (the SVM clock sits at `SLOT`).
    fn make_price_stale(&mut self) {
        self.svm.set_account(feed_account(
            &self.feed,
            dollars(165),
            ORACLE_SCALE,
            SLOT - 151,
            0,
        ));
    }

    fn move_inventory(&mut self, signer: &Pubkey, deposit: bool, base: u64, quote: u64) -> bool {
        let signer_base = ata(signer, &self.base_mint);
        let signer_quote = ata(signer, &self.quote_mint);
        let mut data = vec![if deposit { 1u8 } else { 2u8 }];
        data.extend_from_slice(&base.to_le_bytes());
        data.extend_from_slice(&quote.to_le_bytes());
        let mut metas = vec![AccountMeta::new(*signer, true)];
        metas.push(AccountMeta::new_readonly(self.market, false));
        if !deposit {
            metas.push(AccountMeta::new_readonly(self.market_authority, false));
        }
        metas.extend([
            AccountMeta::new_readonly(self.base_mint, false),
            AccountMeta::new_readonly(self.quote_mint, false),
            AccountMeta::new(self.base_vault, false),
            AccountMeta::new(self.quote_vault, false),
            AccountMeta::new(signer_base, false),
            AccountMeta::new(signer_quote, false),
            AccountMeta::new_readonly(token_program(), false),
        ]);
        self.run(metas, data, &[*signer, signer_base, signer_quote])
    }

    fn deposit_inventory_as_operator(&mut self, base: u64, quote: u64) -> bool {
        let operator = self.operator;
        self.move_inventory(&operator, true, base, quote)
    }

    fn withdraw_inventory_as_operator(&mut self, base: u64, quote: u64) -> bool {
        let operator = self.operator;
        self.move_inventory(&operator, false, base, quote)
    }

    fn set_quote(&mut self, signer: &Pubkey, spread_bps: u16, paused: u8) -> bool {
        let mut data = vec![3u8];
        data.extend_from_slice(&spread_bps.to_le_bytes());
        data.push(paused);
        let metas = vec![
            AccountMeta::new_readonly(*signer, true),
            AccountMeta::new(self.market, false),
            AccountMeta::new_readonly(self.base_mint, false),
            AccountMeta::new_readonly(self.quote_mint, false),
        ];
        self.run(metas, data, &[*signer])
    }

    fn swap(
        &mut self,
        trader: &Pubkey,
        direction: u8,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> bool {
        let trader_base = ata(trader, &self.base_mint);
        let trader_quote = ata(trader, &self.quote_mint);
        let mut data = vec![4u8, direction];
        data.extend_from_slice(&amount_in.to_le_bytes());
        data.extend_from_slice(&minimum_amount_out.to_le_bytes());
        let metas = vec![
            AccountMeta::new(*trader, true),
            AccountMeta::new_readonly(self.market, false),
            AccountMeta::new_readonly(self.market_authority, false),
            AccountMeta::new_readonly(self.feed, false),
            AccountMeta::new_readonly(self.base_mint, false),
            AccountMeta::new_readonly(self.quote_mint, false),
            AccountMeta::new(self.base_vault, false),
            AccountMeta::new(self.quote_vault, false),
            AccountMeta::new(trader_base, false),
            AccountMeta::new(trader_quote, false),
            AccountMeta::new_readonly(token_program(), false),
            AccountMeta::new_readonly(clock_sysvar(), false),
        ];
        self.run(metas, data, &[*trader, trader_base, trader_quote])
    }

    fn run(&mut self, metas: Vec<AccountMeta>, data: Vec<u8>, provide: &[Pubkey]) -> bool {
        let accounts: Vec<Account> = provide
            .iter()
            .map(|pk| self.svm.get_account(pk).unwrap_or_else(|| empty(pk)))
            .collect();
        let result = self.svm.process_instruction(
            &Instruction {
                program_id: program_id(),
                accounts: metas,
                data,
            },
            &accounts,
        );
        result.is_ok()
    }
}

#[test]
fn test_initialize_market() {
    let env = setup();
    // The market and both vaults were created, and the inventory landed.
    assert!(env.svm.get_account(&env.market).is_some());
    assert_eq!(token_amount(&env.svm, &env.base_vault), 1_000 * ONE_TOKEN);
    assert_eq!(
        token_amount(&env.svm, &env.quote_vault),
        200_000 * ONE_TOKEN
    );
}

/// Alice buys 10 NVDAx. At $165 with a 10 bps spread the ask is $165.165, so
/// 10 NVDAx costs exactly 1,651.65 USDC.
#[test]
fn test_swap_buys_base_at_the_ask() {
    let mut env = setup();
    let quote_in = 1_651_650_000;
    let alice = env.funded_wallet(0, quote_in);

    assert!(env.swap(&alice, DIRECTION_BUY_BASE, quote_in, 10 * ONE_TOKEN));

    assert_eq!(
        token_amount(&env.svm, &ata(&alice, &env.base_mint)),
        10 * ONE_TOKEN
    );
    assert_eq!(token_amount(&env.svm, &ata(&alice, &env.quote_mint)), 0);
    // Conservation: the vaults moved by exactly the two legs of the fill.
    assert_eq!(token_amount(&env.svm, &env.base_vault), 990 * ONE_TOKEN);
    assert_eq!(
        token_amount(&env.svm, &env.quote_vault),
        200_000 * ONE_TOKEN + quote_in
    );
}

/// Bob sells 10 NVDAx. At $165 with a 10 bps spread the bid is $164.835, so
/// he receives exactly 1,648.35 USDC.
#[test]
fn test_swap_sells_base_at_the_bid() {
    let mut env = setup();
    let bob = env.funded_wallet(10 * ONE_TOKEN, 0);

    assert!(env.swap(&bob, DIRECTION_SELL_BASE, 10 * ONE_TOKEN, 1_648_350_000));

    assert_eq!(token_amount(&env.svm, &ata(&bob, &env.base_mint)), 0);
    assert_eq!(
        token_amount(&env.svm, &ata(&bob, &env.quote_mint)),
        1_648_350_000
    );
}

/// A buy immediately followed by a sell of the same 10 NVDAx costs exactly
/// the round-trip spread: 3.30 USDC, all of which stays in the inventory.
#[test]
fn test_round_trip_costs_exactly_the_spread() {
    let mut env = setup();
    let quote_in = 1_651_650_000;
    let carol = env.funded_wallet(0, quote_in);

    assert!(env.swap(&carol, DIRECTION_BUY_BASE, quote_in, 0));
    assert!(env.swap(&carol, DIRECTION_SELL_BASE, 10 * ONE_TOKEN, 0));

    assert_eq!(token_amount(&env.svm, &ata(&carol, &env.base_mint)), 0);
    assert_eq!(
        token_amount(&env.svm, &ata(&carol, &env.quote_mint)),
        quote_in - 3_300_000
    );
    assert_eq!(
        token_amount(&env.svm, &env.quote_vault),
        200_000 * ONE_TOKEN + 3_300_000
    );
}

/// When the oracle reprices, the quote follows instantly. At $170 the ask is
/// $170.17, so 10 NVDAx costs exactly 1,701.70 USDC.
#[test]
fn test_quote_follows_the_oracle() {
    let mut env = setup();
    env.set_price(dollars(170));

    let quote_in = 1_701_700_000;
    let alice = env.funded_wallet(0, quote_in);
    assert!(env.swap(&alice, DIRECTION_BUY_BASE, quote_in, 10 * ONE_TOKEN));
    assert_eq!(
        token_amount(&env.svm, &ata(&alice, &env.base_mint)),
        10 * ONE_TOKEN
    );
}

/// The operator re-quotes to a 50 bps spread; the next fill prices at
/// $165.825, so 10 NVDAx costs exactly 1,658.25 USDC.
#[test]
fn test_set_quote_changes_the_spread() {
    let mut env = setup();
    let operator = env.operator;
    assert!(env.set_quote(&operator, 50, 0));

    let quote_in = 1_658_250_000;
    let alice = env.funded_wallet(0, quote_in);
    assert!(env.swap(&alice, DIRECTION_BUY_BASE, quote_in, 10 * ONE_TOKEN));
    assert_eq!(
        token_amount(&env.svm, &ata(&alice, &env.base_mint)),
        10 * ONE_TOKEN
    );
}

/// The operator can withdraw every token in both vaults at any time — its
/// capital, its exit. Afterwards swaps fail rather than misprice.
#[test]
fn test_operator_can_withdraw_everything_and_swaps_then_fail() {
    let mut env = setup();
    assert!(env.withdraw_inventory_as_operator(1_000 * ONE_TOKEN, 200_000 * ONE_TOKEN));

    assert_eq!(token_amount(&env.svm, &env.base_vault), 0);
    assert_eq!(token_amount(&env.svm, &env.quote_vault), 0);

    let alice = env.funded_wallet(0, 1_651_650_000);
    assert!(!env.swap(&alice, DIRECTION_BUY_BASE, 1_651_650_000, 0));
}

#[test]
fn test_withdraw_more_than_inventory_fails() {
    let mut env = setup();
    assert!(!env.withdraw_inventory_as_operator(1_001 * ONE_TOKEN, 0));
}

#[test]
fn test_deposit_rejects_non_operator() {
    let mut env = setup();
    let mallory = env.funded_wallet(ONE_TOKEN, ONE_TOKEN);
    assert!(!env.move_inventory(&mallory, true, ONE_TOKEN, 0));
}

#[test]
fn test_withdraw_rejects_non_operator() {
    let mut env = setup();
    let mallory = env.funded_wallet(0, 0);
    assert!(!env.move_inventory(&mallory, false, ONE_TOKEN, 0));
}

#[test]
fn test_set_quote_rejects_non_operator() {
    let mut env = setup();
    let mallory = env.funded_wallet(0, 0);
    assert!(!env.set_quote(&mallory, 500, 1));
}

/// A fill below the caller's minimum is rejected, not filled worse.
#[test]
fn test_swap_rejects_slippage() {
    let mut env = setup();
    let quote_in = 1_651_650_000;
    let alice = env.funded_wallet(0, quote_in);
    // The fill would be exactly 10 NVDAx; demand one minor unit more.
    assert!(!env.swap(&alice, DIRECTION_BUY_BASE, quote_in, 10 * ONE_TOKEN + 1));
}

/// An oracle price older than the staleness bound cannot be traded against:
/// a lagging quote is a free option for arbitrageurs.
#[test]
fn test_swap_rejects_stale_price() {
    let mut env = setup();
    env.make_price_stale();
    let alice = env.funded_wallet(0, 1_651_650_000);
    assert!(!env.swap(&alice, DIRECTION_BUY_BASE, 1_651_650_000, 0));
}

/// A price the oracle itself is unsure about is rejected: the confidence band
/// (about 1.2% here) exceeds the market's 1% limit.
#[test]
fn test_swap_rejects_wide_confidence() {
    let mut env = setup();
    env.set_price_with_confidence(dollars(165), 200_000_000);
    let alice = env.funded_wallet(0, 1_651_650_000);
    assert!(!env.swap(&alice, DIRECTION_BUY_BASE, 1_651_650_000, 0));
}

/// While the operator has pulled its quotes, nobody can swap; unpausing
/// restores the exact same quote.
#[test]
fn test_swap_rejects_when_paused() {
    let mut env = setup();
    let operator = env.operator;
    assert!(env.set_quote(&operator, SPREAD_BPS, 1));

    let alice = env.funded_wallet(0, 1_651_650_000);
    assert!(!env.swap(&alice, DIRECTION_BUY_BASE, 1_651_650_000, 0));

    assert!(env.set_quote(&operator, SPREAD_BPS, 0));
    assert!(env.swap(&alice, DIRECTION_BUY_BASE, 1_651_650_000, 10 * ONE_TOKEN));
}

#[test]
fn test_swap_rejects_zero_amount() {
    let mut env = setup();
    let alice = env.funded_wallet(0, ONE_TOKEN);
    assert!(!env.swap(&alice, DIRECTION_BUY_BASE, 0, 0));
}

/// A buy bigger than the base inventory is rejected whole — a prop AMM never
/// partially fills, and never prices what it cannot deliver.
#[test]
fn test_swap_rejects_insufficient_inventory() {
    let mut env = setup();
    // 1,100 NVDAx at $165.165 ≈ 181,681.50 USDC — affordable for the trader,
    // but the vault only holds 1,000 NVDAx.
    let quote_in = 181_681_500_000;
    let whale = env.funded_wallet(0, quote_in);
    assert!(!env.swap(&whale, DIRECTION_BUY_BASE, quote_in, 0));
}

#[test]
fn test_initialize_market_rejects_zero_spread() {
    assert!(try_setup(0).is_err());
}

#[test]
fn test_initialize_market_rejects_full_spread() {
    assert!(try_setup(10_000).is_err());
}

#[test]
fn test_set_quote_rejects_invalid_spread() {
    let mut env = setup();
    let operator = env.operator;
    assert!(!env.set_quote(&operator, 0, 0));
    assert!(!env.set_quote(&operator, 10_000, 0));
}
