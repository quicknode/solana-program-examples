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

const ONE_USDC: u64 = 1_000_000;
const ORACLE_SCALE: u32 = 8;

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
    collateral_mint: Pubkey,
    feed: Pubkey,
    admin: Pubkey,
    pool: Pubkey,
    pool_authority: Pubkey,
    lp_mint: Pubkey,
    custody_vault: Pubkey,
}

const SLOT: u64 = 10;

/// Build an SVM with the program, token program, a collateral mint, an oracle
/// feed at $100, and an initialized pool. Insurance fee and profit warm-up off.
fn setup() -> Env {
    try_setup(500, 10).expect("pool initialization should succeed")
}

/// Like `setup`, but with the two cross-checked pool parameters exposed and an
/// `initialize_pool` rejection surfaced instead of panicking, so tests can
/// probe the parameter validation.
fn try_setup(maintenance_margin_bps: u16, close_fee_bps: u16) -> Result<Env, ()> {
    try_setup_full(maintenance_margin_bps, close_fee_bps, 10, 0, 0)
}

/// The full pool-parameter builder. Exposes the open fee, the insurance-fund fee
/// cut, and the profit warm-up so the haircut, maturation, and insurance tests
/// can configure them.
fn try_setup_full(
    maintenance_margin_bps: u16,
    close_fee_bps: u16,
    open_fee_bps: u16,
    insurance_fee_bps: u16,
    profit_warmup_slots: u64,
) -> Result<Env, ()> {
    let elf = fs::read("target/deploy/quasar_perpetual_futures.so").unwrap();
    let collateral_mint = Pubkey::new_unique();
    let feed = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let pool = pda(&[b"pool", collateral_mint.as_ref(), feed.as_ref()]);
    let pool_authority = pda(&[b"authority", pool.as_ref()]);
    let lp_mint = pda(&[b"lp_mint", pool.as_ref()]);
    let custody_vault = pda(&[b"vault", pool.as_ref()]);

    let mut svm = QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program()
        .with_slot(SLOT)
        .with_account(mint_account(&collateral_mint))
        .with_account(feed_account(&feed, dollars(100), ORACLE_SCALE, SLOT, 0))
        .with_account(create_keyed_system_account(&admin, 100_000_000_000));

    // initialize_pool
    let mut data = vec![0u8];
    data.extend_from_slice(&ORACLE_SCALE.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // funding_rate_per_slot = 0
    data.extend_from_slice(&open_fee_bps.to_le_bytes());
    data.extend_from_slice(&close_fee_bps.to_le_bytes());
    data.extend_from_slice(&10u16.to_le_bytes()); // max_leverage
    data.extend_from_slice(&maintenance_margin_bps.to_le_bytes());
    data.extend_from_slice(&100u16.to_le_bytes()); // liquidation_fee_bps
    data.extend_from_slice(&100u16.to_le_bytes()); // max_confidence_bps
    data.extend_from_slice(&insurance_fee_bps.to_le_bytes());
    data.extend_from_slice(&profit_warmup_slots.to_le_bytes());
    let metas = vec![
        AccountMeta::new(admin, true),
        AccountMeta::new(pool, false),
        AccountMeta::new_readonly(collateral_mint, false),
        AccountMeta::new_readonly(feed, false),
        AccountMeta::new_readonly(pool_authority, false),
        AccountMeta::new(lp_mint, false),
        AccountMeta::new(custody_vault, false),
        AccountMeta::new_readonly(token_program(), false),
        AccountMeta::new_readonly(system_program(), false),
        AccountMeta::new_readonly(clock_sysvar(), false),
        AccountMeta::new_readonly(rent_sysvar(), false),
    ];
    let provided = vec![
        svm.get_account(&admin).unwrap(),
        empty(&pool),
        svm.get_account(&collateral_mint).unwrap(),
        empty(&lp_mint),
        empty(&custody_vault),
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
        collateral_mint,
        feed,
        admin,
        pool,
        pool_authority,
        lp_mint,
        custody_vault,
    })
}

impl Env {
    /// Create a wallet with a funded collateral token account, returning the
    /// wallet and its collateral account.
    fn funded_wallet(&mut self, collateral: u64) -> (Pubkey, Pubkey) {
        let wallet = Pubkey::new_unique();
        let collateral_account = ata(&wallet, &self.collateral_mint);
        self.svm
            .set_account(create_keyed_system_account(&wallet, 100_000_000_000));
        self.svm.set_account(create_keyed_associated_token_account(
            &wallet,
            &self.collateral_mint,
            collateral,
        ));
        (wallet, collateral_account)
    }

    fn lp_account(&mut self, wallet: &Pubkey) -> Pubkey {
        let account = ata(wallet, &self.lp_mint);
        self.svm.set_account(create_keyed_associated_token_account(
            wallet,
            &self.lp_mint,
            0,
        ));
        account
    }

    fn add_liquidity(&mut self, provider: &Pubkey, amount: u64) -> bool {
        let provider_collateral = ata(provider, &self.collateral_mint);
        let provider_lp = self.lp_account(provider);
        let mut data = vec![1u8];
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        let metas = vec![
            AccountMeta::new(*provider, true),
            AccountMeta::new(self.pool, false),
            AccountMeta::new_readonly(self.pool_authority, false),
            AccountMeta::new_readonly(self.feed, false),
            AccountMeta::new_readonly(self.collateral_mint, false),
            AccountMeta::new(self.lp_mint, false),
            AccountMeta::new(self.custody_vault, false),
            AccountMeta::new(provider_collateral, false),
            AccountMeta::new(provider_lp, false),
            AccountMeta::new_readonly(token_program(), false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(clock_sysvar(), false),
        ];
        self.run(metas, data, &[*provider, provider_collateral, provider_lp])
    }

    fn remove_liquidity(&mut self, provider: &Pubkey, shares: u64) -> bool {
        let provider_collateral = ata(provider, &self.collateral_mint);
        let provider_lp = ata(provider, &self.lp_mint);
        let mut data = vec![2u8];
        data.extend_from_slice(&shares.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        let metas = vec![
            AccountMeta::new(*provider, true),
            AccountMeta::new(self.pool, false),
            AccountMeta::new_readonly(self.pool_authority, false),
            AccountMeta::new_readonly(self.feed, false),
            AccountMeta::new_readonly(self.collateral_mint, false),
            AccountMeta::new(self.lp_mint, false),
            AccountMeta::new(self.custody_vault, false),
            AccountMeta::new(provider_collateral, false),
            AccountMeta::new(provider_lp, false),
            AccountMeta::new_readonly(token_program(), false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(clock_sysvar(), false),
        ];
        self.run(metas, data, &[*provider, provider_collateral, provider_lp])
    }

    fn open_position(&mut self, owner: &Pubkey, side: u8, collateral: u64, size: u64) -> bool {
        let trader_collateral = ata(owner, &self.collateral_mint);
        let position = pda(&[b"position", self.pool.as_ref(), owner.as_ref()]);
        let mut data = vec![3u8, side];
        data.extend_from_slice(&collateral.to_le_bytes());
        data.extend_from_slice(&size.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        let metas = vec![
            AccountMeta::new(*owner, true),
            AccountMeta::new(self.pool, false),
            AccountMeta::new(position, false),
            AccountMeta::new_readonly(self.feed, false),
            AccountMeta::new_readonly(self.collateral_mint, false),
            AccountMeta::new(self.custody_vault, false),
            AccountMeta::new(trader_collateral, false),
            AccountMeta::new_readonly(token_program(), false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(clock_sysvar(), false),
            AccountMeta::new_readonly(rent_sysvar(), false),
        ];
        self.run(metas, data, &[*owner, position, trader_collateral])
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

    /// Advance the clock and refresh the feed at the new slot, so the price stays
    /// fresh past the warm-up window.
    fn warp_and_set_price(&mut self, slot: u64, price: i128) {
        self.svm.sysvars.warp_to_slot(slot);
        self.svm
            .set_account(feed_account(&self.feed, price, ORACLE_SCALE, slot, 0));
    }

    fn close_position(&mut self, owner: &Pubkey) -> bool {
        let trader_collateral = ata(owner, &self.collateral_mint);
        let position = pda(&[b"position", self.pool.as_ref(), owner.as_ref()]);
        let mut data = vec![4u8];
        data.extend_from_slice(&0u64.to_le_bytes());
        let metas = vec![
            AccountMeta::new(*owner, true),
            AccountMeta::new(self.pool, false),
            AccountMeta::new(position, false),
            AccountMeta::new_readonly(self.pool_authority, false),
            AccountMeta::new_readonly(self.feed, false),
            AccountMeta::new_readonly(self.collateral_mint, false),
            AccountMeta::new(self.custody_vault, false),
            AccountMeta::new(trader_collateral, false),
            AccountMeta::new_readonly(token_program(), false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(clock_sysvar(), false),
        ];
        self.run(metas, data, &[*owner, position, trader_collateral])
    }

    fn liquidate(&mut self, liquidator: &Pubkey, owner: &Pubkey) -> bool {
        let trader_collateral = ata(owner, &self.collateral_mint);
        let liquidator_collateral = ata(liquidator, &self.collateral_mint);
        self.svm.set_account(create_keyed_associated_token_account(
            liquidator,
            &self.collateral_mint,
            0,
        ));
        let position = pda(&[b"position", self.pool.as_ref(), owner.as_ref()]);
        let data = vec![5u8];
        let metas = vec![
            AccountMeta::new(*liquidator, true),
            AccountMeta::new(*owner, false),
            AccountMeta::new(self.pool, false),
            AccountMeta::new(position, false),
            AccountMeta::new_readonly(self.pool_authority, false),
            AccountMeta::new_readonly(self.feed, false),
            AccountMeta::new_readonly(self.collateral_mint, false),
            AccountMeta::new(self.custody_vault, false),
            AccountMeta::new(trader_collateral, false),
            AccountMeta::new(liquidator_collateral, false),
            AccountMeta::new_readonly(token_program(), false),
            AccountMeta::new_readonly(system_program(), false),
            AccountMeta::new_readonly(clock_sysvar(), false),
        ];
        self.run(
            metas,
            data,
            &[
                *liquidator,
                *owner,
                position,
                trader_collateral,
                liquidator_collateral,
            ],
        )
    }

    fn collect_fees(&mut self) -> bool {
        let admin = self.admin;
        let admin_collateral = ata(&admin, &self.collateral_mint);
        self.svm.set_account(create_keyed_associated_token_account(
            &admin,
            &self.collateral_mint,
            0,
        ));
        let data = vec![6u8];
        let metas = vec![
            AccountMeta::new(admin, true),
            AccountMeta::new(self.pool, false),
            AccountMeta::new_readonly(self.pool_authority, false),
            AccountMeta::new_readonly(self.feed, false),
            AccountMeta::new_readonly(self.collateral_mint, false),
            AccountMeta::new(self.custody_vault, false),
            AccountMeta::new(admin_collateral, false),
            AccountMeta::new_readonly(token_program(), false),
            AccountMeta::new_readonly(system_program(), false),
        ];
        self.run(metas, data, &[admin, admin_collateral])
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
fn test_initialize_pool() {
    let env = setup();
    // The pool, vault, and liquidity-provider mint were created.
    assert!(env.svm.get_account(&env.pool).is_some());
    assert!(env.svm.get_account(&env.custody_vault).is_some());
    assert!(env.svm.get_account(&env.lp_mint).is_some());
}

#[test]
fn test_add_liquidity() {
    let mut env = setup();
    let (provider, _) = env.funded_wallet(10_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 10_000 * ONE_USDC));

    // The vault holds the deposit and the provider received shares.
    assert_eq!(
        token_amount(&env.svm, &env.custody_vault),
        10_000 * ONE_USDC
    );
    let provider_lp = ata(&provider, &env.lp_mint);
    assert_eq!(
        token_amount(&env.svm, &provider_lp),
        10_000 * ONE_USDC - 1_000
    );
}

#[test]
fn test_remove_liquidity_round_trip() {
    let mut env = setup();
    let (provider, provider_collateral) = env.funded_wallet(10_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 10_000 * ONE_USDC));

    let provider_lp = ata(&provider, &env.lp_mint);
    let shares = token_amount(&env.svm, &provider_lp);
    let mut data = vec![2u8];
    data.extend_from_slice(&shares.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    let metas = vec![
        AccountMeta::new(provider, true),
        AccountMeta::new(env.pool, false),
        AccountMeta::new_readonly(env.pool_authority, false),
        AccountMeta::new_readonly(env.feed, false),
        AccountMeta::new_readonly(env.collateral_mint, false),
        AccountMeta::new(env.lp_mint, false),
        AccountMeta::new(env.custody_vault, false),
        AccountMeta::new(provider_collateral, false),
        AccountMeta::new(provider_lp, false),
        AccountMeta::new_readonly(token_program(), false),
        AccountMeta::new_readonly(system_program(), false),
        AccountMeta::new_readonly(clock_sysvar(), false),
    ];
    assert!(env.run(metas, data, &[provider, provider_collateral, provider_lp]));

    // Sole provider reclaims the full deposit.
    assert_eq!(
        token_amount(&env.svm, &provider_collateral),
        10_000 * ONE_USDC
    );
}

#[test]
fn test_open_long_position() {
    let mut env = setup();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let (trader, _) = env.funded_wallet(1_000 * ONE_USDC);
    assert!(env.open_position(&trader, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC));

    let position = pda(&[b"position", env.pool.as_ref(), trader.as_ref()]);
    assert!(env.svm.get_account(&position).is_some());
}

#[test]
fn test_close_long_in_profit() {
    let mut env = setup();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let (trader, trader_collateral) = env.funded_wallet(1_000 * ONE_USDC);
    let size = 5_000 * ONE_USDC;
    assert!(env.open_position(&trader, 0, 1_000 * ONE_USDC, size));

    // Price rises 20%: a $5,000 long earns $1,000.
    env.set_price(dollars(120));
    assert!(env.close_position(&trader));

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = 1_000 * ONE_USDC - open_fee;
    let profit = size / 5;
    let expected = net_collateral + profit - close_fee;
    assert_eq!(token_amount(&env.svm, &trader_collateral), expected);
}

#[test]
fn test_open_rejects_excess_leverage() {
    let mut env = setup();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let (trader, _) = env.funded_wallet(1_000 * ONE_USDC);
    // 11x exceeds the 10x maximum.
    assert!(!env.open_position(&trader, 0, 1_000 * ONE_USDC, 11_000 * ONE_USDC));
}

#[test]
fn test_liquidate_underwater_long() {
    let mut env = setup();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let (trader, _) = env.funded_wallet(1_100 * ONE_USDC);
    let size = 10_000 * ONE_USDC;
    assert!(env.open_position(&trader, 0, 1_100 * ONE_USDC, size));

    // Price falls 9%: a $10,000 long loses $900, dropping below maintenance.
    env.set_price(dollars(91));
    let liquidator = Pubkey::new_unique();
    env.svm
        .set_account(create_keyed_system_account(&liquidator, 100_000_000_000));
    assert!(env.liquidate(&liquidator, &trader));

    let liquidator_collateral = ata(&liquidator, &env.collateral_mint);
    assert!(token_amount(&env.svm, &liquidator_collateral) > 0);
    let position = pda(&[b"position", env.pool.as_ref(), trader.as_ref()]);
    assert!(env
        .svm
        .get_account(&position)
        .map(|a| a.data.is_empty())
        .unwrap_or(true));
}

#[test]
fn test_collect_fees() {
    let mut env = setup();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));
    let (trader, _) = env.funded_wallet(1_000 * ONE_USDC);
    let size = 5_000 * ONE_USDC;
    assert!(env.open_position(&trader, 0, 1_000 * ONE_USDC, size));

    assert!(env.collect_fees());
    let admin_collateral = ata(&env.admin, &env.collateral_mint);
    // The open fee (0.1% of notional) was swept to the admin.
    assert_eq!(token_amount(&env.svm, &admin_collateral), size / 1_000);
}

#[test]
fn test_wide_oracle_confidence_rejected() {
    let mut env = setup();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));
    let (trader, _) = env.funded_wallet(1_000 * ONE_USDC);

    // The pool tolerates a 1% confidence band (max_confidence_bps = 100). Widen
    // the feed's band to 2% of the price and the open must be rejected.
    env.set_price_with_confidence(dollars(100), dollars(2) as u64);
    assert!(!env.open_position(&trader, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC));
}

#[test]
fn test_open_allowed_without_full_backing() {
    // No open-interest cap: a 10,000 position opens against only 6,000 of
    // liquidity; solvency is kept at exit by the haircut, not gated at entry.
    let mut env = setup();
    let (provider, _) = env.funded_wallet(6_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 6_000 * ONE_USDC));
    let (trader, _) = env.funded_wallet(1_100 * ONE_USDC);
    assert!(env.open_position(&trader, 0, 1_100 * ONE_USDC, 10_000 * ONE_USDC));
    let position = pda(&[b"position", env.pool.as_ref(), trader.as_ref()]);
    assert!(env.svm.get_account(&position).is_some());
}

#[test]
fn test_profit_runs_uncapped_when_backed() {
    let mut env = setup();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let collateral = 2_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = env.funded_wallet(collateral);
    assert!(env.open_position(&trader, 0, collateral, size));

    // Price triples: profit is 2x the notional. The deep pool fully backs it, so
    // the haircut is 1 and the trader keeps every cent — profit is uncapped.
    env.set_price(dollars(300));
    assert!(env.close_position(&trader));

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let profit = 2 * size;
    let expected = net_collateral + profit - close_fee;
    assert_eq!(token_amount(&env.svm, &trader_collateral), expected);
}

#[test]
fn test_haircut_scales_profit_when_pool_stressed() {
    // A thin pool and a doubling price: traders are owed more than the pool
    // holds, so the haircut scales the winner to the backing. h = 6,000 / 10,000.
    let mut env = setup();
    let (provider, _) = env.funded_wallet(6_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 6_000 * ONE_USDC));

    let collateral = 1_100 * ONE_USDC;
    let size = 10_000 * ONE_USDC;
    let (trader, trader_collateral) = env.funded_wallet(collateral);
    assert!(env.open_position(&trader, 0, collateral, size));

    env.set_price(dollars(200));
    assert!(env.close_position(&trader));

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let haircut_profit = 6_000 * ONE_USDC; // 0.6 of the 10,000 full profit
    let expected = net_collateral + haircut_profit - close_fee;
    assert_eq!(token_amount(&env.svm, &trader_collateral), expected);
}

#[test]
fn test_remove_liquidity_capped_at_liquidity() {
    // A provider cannot withdraw more than the tracked liquidity, even when open
    // trader losses mark their share higher — that gain is not cash yet.
    let mut env = setup();
    let (provider, _) = env.funded_wallet(10_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 10_000 * ONE_USDC));
    let (trader, _) = env.funded_wallet(1_000 * ONE_USDC);
    assert!(env.open_position(&trader, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC));

    // Price falls 20%: the long is down 1,000, AUM marks to 11,000 while
    // liquidity is 10,000. Redeeming every share is refused; half succeeds.
    env.set_price(dollars(80));
    let provider_lp = ata(&provider, &env.lp_mint);
    let shares = token_amount(&env.svm, &provider_lp);
    assert!(!env.remove_liquidity(&provider, shares));
    assert!(env.remove_liquidity(&provider, shares / 2));
}

#[test]
fn test_profit_blocked_before_maturation() {
    // A 100-slot warm-up: profit cannot be realized in the block it appears.
    let mut env = try_setup_full(500, 10, 10, 0, 100).unwrap();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let (trader, _) = env.funded_wallet(1_000 * ONE_USDC);
    assert!(env.open_position(&trader, 0, 1_000 * ONE_USDC, 5_000 * ONE_USDC));

    // Price jumps 20%; closing for profit before the warm-up elapses is refused.
    env.set_price(dollars(120));
    assert!(!env.close_position(&trader));
}

#[test]
fn test_profit_realized_after_maturation() {
    let mut env = try_setup_full(500, 10, 10, 0, 100).unwrap();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = env.funded_wallet(1_000 * ONE_USDC);
    assert!(env.open_position(&trader, 0, 1_000 * ONE_USDC, size));

    // Wait past the warm-up (opened at slot 10), refresh the price, then close:
    // the profit has matured and is paid in full.
    env.warp_and_set_price(SLOT + 200, dollars(120));
    assert!(env.close_position(&trader));

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = 1_000 * ONE_USDC - open_fee;
    let profit = size / 5;
    let expected = net_collateral + profit - close_fee;
    assert_eq!(token_amount(&env.svm, &trader_collateral), expected);
}

#[test]
fn test_loss_not_gated_by_maturation() {
    // The warm-up gates profit only; a loss can be closed at once.
    let mut env = try_setup_full(500, 10, 10, 0, 100).unwrap();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = env.funded_wallet(1_000 * ONE_USDC);
    assert!(env.open_position(&trader, 0, 1_000 * ONE_USDC, size));

    env.set_price(dollars(90));
    assert!(env.close_position(&trader));

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = 1_000 * ONE_USDC - open_fee;
    let loss = size / 10;
    let expected = net_collateral - loss - close_fee;
    assert_eq!(token_amount(&env.svm, &trader_collateral), expected);
}

#[test]
fn test_insurance_fund_funded_by_fees() {
    // Half of every fee is routed to the insurance fund, so `collect_fees`
    // sweeps only the protocol's half of the open fee.
    let mut env = try_setup_full(500, 10, 10, 5_000, 0).unwrap();
    let (provider, _) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let size = 5_000 * ONE_USDC;
    let (trader, _) = env.funded_wallet(1_000 * ONE_USDC);
    assert!(env.open_position(&trader, 0, 1_000 * ONE_USDC, size));

    assert!(env.collect_fees());
    let admin_collateral = ata(&env.admin, &env.collateral_mint);
    let open_fee = size / 1_000;
    assert_eq!(token_amount(&env.svm, &admin_collateral), open_fee / 2);
}

#[test]
fn test_insurance_absorbs_bankruptcy_deficit() {
    // A position gaps through zero equity, owing 40 beyond its collateral. The
    // insurance fund (here, the 50 open fee) covers that deficit, so the sole
    // provider reclaims the trader's collateral *and* the insurance top-up
    // rather than eating the loss.
    let mut env = try_setup_full(500, 10, 500, 10_000, 0).unwrap();
    let (provider, provider_collateral) = env.funded_wallet(100_000 * ONE_USDC);
    assert!(env.add_liquidity(&provider, 100_000 * ONE_USDC));

    let collateral = 160 * ONE_USDC;
    let size = 1_000 * ONE_USDC;
    let (trader, _) = env.funded_wallet(collateral);
    assert!(env.open_position(&trader, 0, collateral, size));

    // Price falls 15%: a 1,000 long loses 150 against 110 of net collateral.
    env.set_price(dollars(85));
    let liquidator = Pubkey::new_unique();
    env.svm
        .set_account(create_keyed_system_account(&liquidator, 100_000_000_000));
    assert!(env.liquidate(&liquidator, &trader));

    // The deficit (150 loss − 110 net collateral = 40) was paid from insurance.
    // The provider redeems all shares and ends with deposit + collateral + the
    // insurance top-up: 100,000 + 110 + 40 = 100,150.
    let provider_lp = ata(&provider, &env.lp_mint);
    let shares = token_amount(&env.svm, &provider_lp);
    assert!(env.remove_liquidity(&provider, shares));
    assert_eq!(
        token_amount(&env.svm, &provider_collateral),
        100_150 * ONE_USDC
    );
}

#[test]
fn test_initialize_pool_rejects_close_fee_at_or_above_maintenance_margin() {
    // A pool whose close fee reached the maintenance margin could strand a
    // position that is too healthy to liquidate but too poor to pay the fee to
    // close, so initialize_pool refuses the configuration.
    assert!(try_setup(500, 600).is_err());
}
