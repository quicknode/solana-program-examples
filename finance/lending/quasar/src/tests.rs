extern crate std;

use {
    alloc::{vec, vec::Vec},
    quasar_svm::{Account, Instruction, Pubkey, QuasarSvm},
    solana_instruction::AccountMeta,
    spl_token_interface::state::{Account as SplToken, AccountState, Mint as SplMint},
    std::fs,
};

// Prices are passed as `mantissa * 10^-18` (Switchboard-shaped).
const EXP: i32 = -18;
fn dollars(whole: u64) -> i128 {
    (whole as i128) * 1_000_000_000_000_000_000
}
fn cents(amount: u64) -> i128 {
    (amount as i128) * 10_000_000_000_000_000
}

const DECIMALS: u8 = 6;
const UNIT: u64 = 1_000_000; // 1 token at 6 decimals

// Deterministic addresses.
const OWNER: Pubkey = Pubkey::new_from_array([1; 32]);
const SUPPLIER: Pubkey = Pubkey::new_from_array([2; 32]);
const BORROWER: Pubkey = Pubkey::new_from_array([3; 32]);
const LIQUIDATOR: Pubkey = Pubkey::new_from_array([4; 32]);
const COLL_MINT: Pubkey = Pubkey::new_from_array([5; 32]);
const BORROW_MINT: Pubkey = Pubkey::new_from_array([6; 32]);
const QUOTE_MINT: Pubkey = Pubkey::new_from_array([7; 32]);
// Token accounts.
const SUPPLIER_BORROW: Pubkey = Pubkey::new_from_array([10; 32]);
const SUPPLIER_BORROW_SHARE: Pubkey = Pubkey::new_from_array([11; 32]);
const BORROWER_COLL: Pubkey = Pubkey::new_from_array([12; 32]);
const BORROWER_COLL_SHARE: Pubkey = Pubkey::new_from_array([13; 32]);
const BORROWER_BORROW: Pubkey = Pubkey::new_from_array([14; 32]);
const LIQUIDATOR_BORROW: Pubkey = Pubkey::new_from_array([15; 32]);
const LIQUIDATOR_COLL_SHARE: Pubkey = Pubkey::new_from_array([16; 32]);
const OWNER_BORROW: Pubkey = Pubkey::new_from_array([17; 32]);
// Per-owner market index this market is seeded from (owner's market 0).
const MARKET_ID: u64 = 0;

fn token_program() -> Pubkey {
    quasar_svm::SPL_TOKEN_PROGRAM_ID
}
fn system_program() -> Pubkey {
    quasar_svm::system_program::ID
}

fn pda(seeds: &[&[u8]]) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, &crate::ID)
}

fn meta(address: Pubkey, writable: bool, signer: bool) -> AccountMeta {
    if writable {
        let mut m = AccountMeta::new(address.into(), signer);
        m.is_signer = signer;
        m
    } else {
        AccountMeta::new_readonly(address.into(), signer)
    }
}

fn system(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, 10_000_000_000)
}
fn empty(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: system_program(),
        executable: false,
    }
}
fn mint(address: Pubkey, authority: Pubkey) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &SplMint {
            mint_authority: Some(authority).into(),
            supply: 1_000_000_000_000,
            decimals: DECIMALS,
            is_initialized: true,
            freeze_authority: None.into(),
        },
    )
}
fn token(address: Pubkey, the_mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
    quasar_svm::token::create_keyed_token_account(
        &address,
        &SplToken {
            mint: the_mint,
            owner,
            amount,
            state: AccountState::Initialized,
            ..SplToken::default()
        },
    )
}

/// Read an SPL token account's amount from committed bytes (offset 64..72).
fn balance(result: &quasar_svm::ExecutionResult, address: Pubkey) -> u64 {
    let account = result.account(&address).expect("account present");
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

struct World {
    svm: QuasarSvm,
    market: Pubkey,
    coll_reserve: Pubkey,
    coll_vault: Pubkey,
    coll_share_mint: Pubkey,
    coll_price: Pubkey,
    borrow_reserve: Pubkey,
    borrow_vault: Pubkey,
    borrow_share_mint: Pubkey,
    borrow_price: Pubkey,
    obligation: Pubkey,
    obligation_vault: Pubkey,
}

impl World {
    fn new() -> Self {
        let elf = fs::read("target/deploy/quasar_lending.so").unwrap();
        let mut svm = QuasarSvm::new()
            .with_program(&crate::ID, &elf)
            .with_token_program();

        let (market, _) = pda(&[b"lending_market", &MARKET_ID.to_le_bytes()]);
        let (coll_reserve, _) = pda(&[b"reserve", market.as_ref(), COLL_MINT.as_ref()]);
        let (borrow_reserve, _) = pda(&[b"reserve", market.as_ref(), BORROW_MINT.as_ref()]);
        let (coll_vault, _) = pda(&[b"liquidity_vault", coll_reserve.as_ref()]);
        let (borrow_vault, _) = pda(&[b"liquidity_vault", borrow_reserve.as_ref()]);
        let (coll_share_mint, _) = pda(&[b"share_mint", coll_reserve.as_ref()]);
        let (borrow_share_mint, _) = pda(&[b"share_mint", borrow_reserve.as_ref()]);
        // Feed PDAs are seeded by their writing authority (the market owner here).
        let (coll_price, _) = pda(&[b"price_feed", market.as_ref(), COLL_MINT.as_ref()]);
        let (borrow_price, _) = pda(&[b"price_feed", market.as_ref(), BORROW_MINT.as_ref()]);
        let (obligation, _) = pda(&[b"obligation", market.as_ref(), BORROWER.as_ref()]);
        let (obligation_vault, _) =
            pda(&[b"obligation_vault", coll_reserve.as_ref(), obligation.as_ref()]);

        for account in [
            system(OWNER),
            system(SUPPLIER),
            system(BORROWER),
            system(LIQUIDATOR),
            mint(COLL_MINT, OWNER),
            mint(BORROW_MINT, OWNER),
            mint(QUOTE_MINT, OWNER),
            // PDAs created by the program.
            empty(market),
            empty(coll_reserve),
            empty(borrow_reserve),
            empty(coll_vault),
            empty(borrow_vault),
            empty(coll_share_mint),
            empty(borrow_share_mint),
            empty(coll_price),
            empty(borrow_price),
            empty(obligation),
            empty(obligation_vault),
            // Funded user token accounts.
            token(SUPPLIER_BORROW, BORROW_MINT, SUPPLIER, 1_000 * UNIT),
            token(SUPPLIER_BORROW_SHARE, borrow_share_mint, SUPPLIER, 0),
            token(BORROWER_COLL, COLL_MINT, BORROWER, 1_000 * UNIT),
            token(BORROWER_COLL_SHARE, coll_share_mint, BORROWER, 0),
            token(BORROWER_BORROW, BORROW_MINT, BORROWER, 0),
            token(LIQUIDATOR_BORROW, BORROW_MINT, LIQUIDATOR, 1_000 * UNIT),
            token(LIQUIDATOR_COLL_SHARE, coll_share_mint, LIQUIDATOR, 0),
            // Where the market owner receives collected protocol fees.
            token(OWNER_BORROW, BORROW_MINT, OWNER, 0),
        ] {
            svm.set_account(account);
        }

        World {
            svm,
            market,
            coll_reserve,
            coll_vault,
            coll_share_mint,
            coll_price,
            borrow_reserve,
            borrow_vault,
            borrow_share_mint,
            borrow_price,
            obligation,
            obligation_vault,
        }
    }

    fn run(&mut self, data: Vec<u8>, metas: Vec<AccountMeta>) -> quasar_svm::ExecutionResult {
        let instruction = Instruction {
            program_id: crate::ID,
            accounts: metas,
            data,
        };
        self.svm.process_instruction(&instruction, &[])
    }

    fn init_market(&mut self) {
        // Instruction data: [discriminator 0][market_id u64 LE].
        let mut data = vec![0u8];
        data.extend_from_slice(&MARKET_ID.to_le_bytes());
        let metas = vec![
            meta(OWNER, true, true),
            meta(self.market, true, false),
            meta(QUOTE_MINT, false, false),
            meta(system_program(), false, false),
        ];
        self.run(data, metas).assert_success();
    }

    fn set_price(&mut self, the_mint: Pubkey, price_feed: Pubkey, mantissa: i128) {
        let mut data = vec![2u8];
        data.extend_from_slice(&mantissa.to_le_bytes());
        data.extend_from_slice(&EXP.to_le_bytes());
        let metas = vec![
            meta(OWNER, true, true),
            meta(self.market, false, false),
            meta(price_feed, true, false),
            meta(the_mint, false, false),
            meta(system_program(), false, false),
        ];
        self.run(data, metas).assert_success();
    }

    #[allow(clippy::too_many_arguments)]
    fn init_reserve(&mut self, the_mint: Pubkey, reserve: Pubkey, vault: Pubkey, share: Pubkey, price: Pubkey) {
        // 75% LTV, 80% liquidation threshold, 5% bonus, 50% close factor, 10% reserve
        // factor, kink 80%, 2% / 20% / 150% APR curve.
        let config: [u16; 9] = [7_500, 8_000, 500, 5_000, 1_000, 8_000, 200, 2_000, 15_000];
        let mut data = vec![1u8];
        for value in config {
            data.extend_from_slice(&value.to_le_bytes());
        }
        let metas = vec![
            meta(OWNER, true, true),
            meta(self.market, false, false),
            meta(reserve, true, false),
            meta(the_mint, false, false),
            meta(vault, true, false),
            meta(share, true, false),
            meta(price, false, false),
            meta(token_program(), false, false),
            meta(system_program(), false, false),
        ];
        self.run(data, metas).assert_success();
    }

    fn setup_markets(&mut self) {
        self.init_market();
        self.set_price(COLL_MINT, self.coll_price, dollars(1));
        self.set_price(BORROW_MINT, self.borrow_price, dollars(1));
        self.init_reserve(COLL_MINT, self.coll_reserve, self.coll_vault, self.coll_share_mint, self.coll_price);
        self.init_reserve(BORROW_MINT, self.borrow_reserve, self.borrow_vault, self.borrow_share_mint, self.borrow_price);
    }

    #[allow(clippy::too_many_arguments)]
    fn deposit(
        &mut self,
        supplier: Pubkey,
        reserve: Pubkey,
        the_mint: Pubkey,
        vault: Pubkey,
        share: Pubkey,
        supplier_liq: Pubkey,
        supplier_share: Pubkey,
        amount: u64,
    ) -> quasar_svm::ExecutionResult {
        let mut data = vec![3u8];
        data.extend_from_slice(&amount.to_le_bytes());
        let metas = vec![
            meta(supplier, true, true),
            meta(reserve, true, false),
            meta(the_mint, false, false),
            meta(vault, true, false),
            meta(share, true, false),
            meta(supplier_liq, true, false),
            meta(supplier_share, true, false),
            meta(token_program(), false, false),
        ];
        self.run(data, metas)
    }

    fn redeem(
        &mut self,
        supplier_liq: Pubkey,
        supplier_share: Pubkey,
        shares: u64,
    ) -> quasar_svm::ExecutionResult {
        let mut data = vec![4u8];
        data.extend_from_slice(&shares.to_le_bytes());
        let metas = vec![
            meta(SUPPLIER, true, true),
            meta(self.borrow_reserve, true, false),
            meta(BORROW_MINT, false, false),
            meta(self.borrow_vault, true, false),
            meta(self.borrow_share_mint, true, false),
            meta(supplier_liq, true, false),
            meta(supplier_share, true, false),
            meta(token_program(), false, false),
        ];
        self.run(data, metas)
    }

    fn init_obligation(&mut self) {
        let metas = vec![
            meta(BORROWER, true, true),
            meta(self.market, false, false),
            meta(self.obligation, true, false),
            meta(system_program(), false, false),
        ];
        self.run(vec![5], metas).assert_success();
    }

    fn post_collateral(&mut self, shares: u64) -> quasar_svm::ExecutionResult {
        let mut data = vec![6u8];
        data.extend_from_slice(&shares.to_le_bytes());
        let metas = vec![
            meta(BORROWER, true, true),
            meta(self.market, false, false),
            meta(self.obligation, true, false),
            meta(self.coll_reserve, false, false),
            meta(self.coll_share_mint, false, false),
            meta(self.obligation_vault, true, false),
            meta(BORROWER_COLL_SHARE, true, false),
            meta(quasar_svm::solana_sdk_ids::sysvar::rent::ID, false, false),
            meta(token_program(), false, false),
            meta(system_program(), false, false),
        ];
        self.run(data, metas)
    }

    fn borrow(&mut self, amount: u64) -> quasar_svm::ExecutionResult {
        let mut data = vec![8u8];
        data.extend_from_slice(&amount.to_le_bytes());
        let metas = vec![
            meta(BORROWER, true, true),
            meta(self.market, false, false),
            meta(self.obligation, true, false),
            meta(self.coll_reserve, true, false),
            meta(self.coll_price, false, false),
            meta(self.borrow_reserve, true, false),
            meta(self.borrow_price, false, false),
            meta(BORROW_MINT, false, false),
            meta(self.borrow_vault, true, false),
            meta(BORROWER_BORROW, true, false),
            meta(token_program(), false, false),
        ];
        self.run(data, metas)
    }

    fn repay(&mut self, amount: u64) -> quasar_svm::ExecutionResult {
        let mut data = vec![9u8];
        data.extend_from_slice(&amount.to_le_bytes());
        let metas = vec![
            meta(BORROWER, true, true),
            meta(self.obligation, true, false),
            meta(self.borrow_reserve, true, false),
            meta(BORROW_MINT, false, false),
            meta(self.borrow_vault, true, false),
            meta(BORROWER_BORROW, true, false),
            meta(token_program(), false, false),
        ];
        self.run(data, metas)
    }

    fn liquidate(&mut self, amount: u64) -> quasar_svm::ExecutionResult {
        let mut data = vec![10u8];
        data.extend_from_slice(&amount.to_le_bytes());
        let metas = vec![
            meta(LIQUIDATOR, true, true),
            meta(self.obligation, true, false),
            meta(self.market, false, false),
            meta(self.coll_reserve, true, false),
            meta(self.coll_price, false, false),
            meta(self.coll_share_mint, false, false),
            meta(self.obligation_vault, true, false),
            meta(LIQUIDATOR_COLL_SHARE, true, false),
            meta(self.borrow_reserve, true, false),
            meta(self.borrow_price, false, false),
            meta(BORROW_MINT, false, false),
            meta(self.borrow_vault, true, false),
            meta(LIQUIDATOR_BORROW, true, false),
            meta(token_program(), false, false),
        ];
        self.run(data, metas)
    }

    /// Supplier funds the borrow reserve; borrower posts 1000 units of collateral.
    fn bootstrap_position(&mut self) {
        self.setup_markets();
        self.deposit(
            SUPPLIER, self.borrow_reserve, BORROW_MINT, self.borrow_vault,
            self.borrow_share_mint, SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE, 1_000 * UNIT,
        )
        .assert_success();
        self.deposit(
            BORROWER, self.coll_reserve, COLL_MINT, self.coll_vault,
            self.coll_share_mint, BORROWER_COLL, BORROWER_COLL_SHARE, 1_000 * UNIT,
        )
        .assert_success();
        self.init_obligation();
        self.post_collateral(1_000 * UNIT).assert_success();
    }

    /// Market owner collects accrued protocol fees from the borrow reserve into
    /// `OWNER_BORROW`. The handler accrues interest itself, so no separate refresh.
    fn collect_borrow_fees(&mut self) -> quasar_svm::ExecutionResult {
        let metas = vec![
            meta(OWNER, true, true),
            meta(self.market, false, false),
            meta(self.borrow_reserve, true, false),
            meta(BORROW_MINT, false, false),
            meta(self.borrow_vault, true, false),
            meta(OWNER_BORROW, true, false),
            meta(token_program(), false, false),
        ];
        self.run(vec![11u8], metas)
    }
}

#[test]
fn supply_mints_shares_one_to_one_and_redeem_returns_liquidity() {
    let mut world = World::new();
    world.setup_markets();

    let result = world.deposit(
        SUPPLIER, world.borrow_reserve, BORROW_MINT, world.borrow_vault,
        world.borrow_share_mint, SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE, 1_000 * UNIT,
    );
    result.assert_success();
    assert_eq!(balance(&result, SUPPLIER_BORROW_SHARE), 1_000 * UNIT, "first deposit mints 1:1");
    assert_eq!(balance(&result, SUPPLIER_BORROW), 0);

    let result = world.redeem(SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE, 1_000 * UNIT);
    result.assert_success();
    assert_eq!(balance(&result, SUPPLIER_BORROW), 1_000 * UNIT, "redeem returns liquidity");
    assert_eq!(balance(&result, SUPPLIER_BORROW_SHARE), 0);
}

#[test]
fn borrow_up_to_ltv_succeeds_and_beyond_fails() {
    let mut world = World::new();
    world.bootstrap_position();

    // $1000 collateral, 75% LTV => borrow up to 750 units of the $1 borrow token.
    let result = world.borrow(750 * UNIT);
    result.assert_success();
    assert_eq!(balance(&result, BORROWER_BORROW), 750 * UNIT);

    // One unit more exceeds the allowed borrow value.
    assert!(world.borrow(UNIT).is_err(), "borrowing past LTV must fail");
}

#[test]
fn repay_reduces_debt() {
    let mut world = World::new();
    world.bootstrap_position();
    world.borrow(500 * UNIT).assert_success();

    let result = world.repay(200 * UNIT);
    result.assert_success();
    // Borrower spent 200 of the 500 borrowed.
    assert_eq!(balance(&result, BORROWER_BORROW), 300 * UNIT);
}

#[test]
fn interest_accrues_and_lifts_share_value() {
    let mut world = World::new();
    world.bootstrap_position();
    world.borrow(500 * UNIT).assert_success();

    // ~0.1 year passes; re-publish prices so feeds stay fresh.
    world.svm.sysvars.warp_to_slot(7_884_000);
    world.set_price(COLL_MINT, world.coll_price, dollars(1));
    world.set_price(BORROW_MINT, world.borrow_price, dollars(1));

    // Supplier redeems 100 shares; interest on the 500 borrowed means each share
    // is now worth more than one liquidity unit.
    let result = world.redeem(SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE, 100 * UNIT);
    result.assert_success();
    assert!(
        balance(&result, SUPPLIER_BORROW) > 100 * UNIT,
        "100 shares should redeem for more than 100 units after interest, got {}",
        balance(&result, SUPPLIER_BORROW)
    );
}

#[test]
fn unhealthy_position_is_liquidated_and_healthy_is_rejected() {
    let mut world = World::new();
    world.bootstrap_position();
    world.borrow(700 * UNIT).assert_success();

    // Healthy at $1 collateral ($1000 * 80% = $800 threshold > $700 debt).
    assert!(world.liquidate(350 * UNIT).is_err(), "healthy obligation must not be liquidatable");

    // Collateral price halves to $0.50: $500 collateral, $400 threshold < $700 debt.
    world.set_price(COLL_MINT, world.coll_price, cents(50));

    let result = world.liquidate(350 * UNIT);
    result.assert_success();
    // Liquidator repaid 350 of the borrow token and seized collateral share tokens.
    assert_eq!(balance(&result, LIQUIDATOR_BORROW), 650 * UNIT);
    assert!(
        balance(&result, LIQUIDATOR_COLL_SHARE) > 0,
        "liquidator should receive seized collateral shares"
    );
}

#[test]
fn protocol_fees_accrue_and_owner_can_collect() {
    let mut world = World::new();
    world.bootstrap_position();
    world.borrow(500 * UNIT).assert_success();

    // ~0.1 year passes; interest accrues, and the reserve factor (10%) sets some
    // of it aside for the market owner.
    world.svm.sysvars.warp_to_slot(7_884_000);

    let result = world.collect_borrow_fees();
    result.assert_success();
    assert!(
        balance(&result, OWNER_BORROW) > 0,
        "owner should collect a positive protocol fee, got {}",
        balance(&result, OWNER_BORROW)
    );
}
