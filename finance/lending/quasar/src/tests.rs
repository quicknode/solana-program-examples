//! Integration tests. Most scenarios drive the program through `quasar-test`
//! (`#[quasar_test]` fixtures + `crate::cpi` builders). The two scenarios that
//! must warp the SLOT (interest accrual is computed from `Clock::get()?.slot`)
//! keep the low-level QuasarSvm harness — see `slot_warp` at the bottom.

use {
    crate::{
        cpi::{
            BorrowObligationLiquidityInstruction, DepositObligationCollateralInstruction,
            DepositReserveLiquidityInstruction, InitLendingMarketInstruction,
            InitObligationInstruction, InitReserveInstruction, LiquidateObligationInstruction,
            RedeemReserveCollateralInstruction, RepayObligationLiquidityInstruction,
            SetPriceInstruction,
        },
        state::{LendingMarket, LiquidityVaultPda, Obligation, Reserve, ShareMintPda},
    },
    quasar_test::prelude::*,
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
const COLLATERAL_MINT: Pubkey = Pubkey::new_from_array([5; 32]);
const BORROW_MINT: Pubkey = Pubkey::new_from_array([6; 32]);
const QUOTE_MINT: Pubkey = Pubkey::new_from_array([7; 32]);
// Token accounts.
const SUPPLIER_BORROW: Pubkey = Pubkey::new_from_array([10; 32]);
const SUPPLIER_BORROW_SHARE: Pubkey = Pubkey::new_from_array([11; 32]);
const BORROWER_COLLATERAL: Pubkey = Pubkey::new_from_array([12; 32]);
const BORROWER_COLLATERAL_SHARE: Pubkey = Pubkey::new_from_array([13; 32]);
const BORROWER_BORROW: Pubkey = Pubkey::new_from_array([14; 32]);
const LIQUIDATOR_BORROW: Pubkey = Pubkey::new_from_array([15; 32]);
const LIQUIDATOR_COLLATERAL_SHARE: Pubkey = Pubkey::new_from_array([16; 32]);
const OWNER_BORROW: Pubkey = Pubkey::new_from_array([17; 32]);
// Per-owner market index this market is seeded from (owner's market 0).
const MARKET_ID: u64 = 0;

/// Every PDA the scenarios touch, derived from the typed seeds.
struct Pdas {
    market: Pubkey,
    collateral_reserve: Pubkey,
    collateral_vault: Pubkey,
    collateral_share_mint: Pubkey,
    collateral_price: Pubkey,
    borrow_reserve: Pubkey,
    borrow_vault: Pubkey,
    borrow_share_mint: Pubkey,
    borrow_price: Pubkey,
    obligation: Pubkey,
}

fn pdas(test: &Test) -> Pdas {
    let market = test.derive_pda(LendingMarket::seeds(MARKET_ID));
    let collateral_reserve = test.derive_pda(Reserve::seeds(&market, &COLLATERAL_MINT));
    let borrow_reserve = test.derive_pda(Reserve::seeds(&market, &BORROW_MINT));
    Pdas {
        market,
        collateral_reserve,
        collateral_vault: test.derive_pda(LiquidityVaultPda::seeds(&collateral_reserve)),
        collateral_share_mint: test.derive_pda(ShareMintPda::seeds(&collateral_reserve)),
        // Feed PDAs are seeded by (market, mint) — scoped to the market, not
        // to any individual.
        collateral_price: test.derive_pda(crate::state::PriceFeed::seeds(&market, &COLLATERAL_MINT)),
        borrow_reserve,
        borrow_vault: test.derive_pda(LiquidityVaultPda::seeds(&borrow_reserve)),
        borrow_share_mint: test.derive_pda(ShareMintPda::seeds(&borrow_reserve)),
        borrow_price: test.derive_pda(crate::state::PriceFeed::seeds(&market, &BORROW_MINT)),
        obligation: test.derive_pda(Obligation::seeds(&market, &BORROWER)),
    }
}

/// Wallets, mints, and funded user token accounts (mirrors the low-level
/// harness's world).
fn base_world(test: &mut Test) -> Pdas {
    let w = pdas(test);
    for wallet in [OWNER, SUPPLIER, BORROWER, LIQUIDATOR] {
        test.add(Wallet::new().at(wallet));
    }
    for the_mint in [COLLATERAL_MINT, BORROW_MINT, QUOTE_MINT] {
        test.add(
            Mint::new(OWNER)
                .at(the_mint)
                .supply(1_000_000_000_000)
                .decimals(DECIMALS),
        );
    }
    test.add(
        TokenAccount::new(BORROW_MINT, SUPPLIER)
            .at(SUPPLIER_BORROW)
            .amount(1_000 * UNIT),
    );
    test.add(TokenAccount::new(w.borrow_share_mint, SUPPLIER).at(SUPPLIER_BORROW_SHARE));
    test.add(
        TokenAccount::new(COLLATERAL_MINT, BORROWER)
            .at(BORROWER_COLLATERAL)
            .amount(1_000 * UNIT),
    );
    test.add(TokenAccount::new(w.collateral_share_mint, BORROWER).at(BORROWER_COLLATERAL_SHARE));
    test.add(TokenAccount::new(BORROW_MINT, BORROWER).at(BORROWER_BORROW));
    test.add(
        TokenAccount::new(BORROW_MINT, LIQUIDATOR)
            .at(LIQUIDATOR_BORROW)
            .amount(1_000 * UNIT),
    );
    test.add(TokenAccount::new(w.collateral_share_mint, LIQUIDATOR).at(LIQUIDATOR_COLLATERAL_SHARE));
    // Where the market owner receives collected protocol fees.
    test.add(TokenAccount::new(BORROW_MINT, OWNER).at(OWNER_BORROW));
    w
}

fn set_price(test: &mut Test, w: &Pdas, the_mint: Pubkey, mantissa: i128) {
    test.send(SetPriceInstruction {
        owner: OWNER,
        lending_market: w.market,
        mint: the_mint,
        price_mantissa: mantissa,
        exponent: EXP,
    })
    .succeeds();
}

fn init_reserve(test: &mut Test, w: &Pdas, the_mint: Pubkey) {
    // 75% LTV, 80% liquidation threshold, 5% bonus, 50% close factor, 10%
    // reserve factor, kink 80%, 2% / 20% / 150% APR curve.
    test.send(InitReserveInstruction {
        owner: OWNER,
        lending_market: w.market,
        liquidity_mint: the_mint,
        loan_to_value_bps: 7_500,
        liquidation_threshold_bps: 8_000,
        liquidation_bonus_bps: 500,
        close_factor_bps: 5_000,
        reserve_factor_bps: 1_000,
        optimal_utilization_bps: 8_000,
        min_borrow_rate_bps: 200,
        optimal_borrow_rate_bps: 2_000,
        max_borrow_rate_bps: 15_000,
    })
    .succeeds();
}

fn setup_markets(test: &mut Test, w: &Pdas) {
    test.send(InitLendingMarketInstruction {
        owner: OWNER,
        quote_mint: QUOTE_MINT,
        market_id: MARKET_ID,
    })
    .succeeds();
    set_price(test, w, COLLATERAL_MINT, dollars(1));
    set_price(test, w, BORROW_MINT, dollars(1));
    init_reserve(test, w, COLLATERAL_MINT);
    init_reserve(test, w, BORROW_MINT);
}

fn deposit_borrow_side(test: &mut Test, w: &Pdas, amount: u64) -> Outcome {
    test.send(DepositReserveLiquidityInstruction {
        supplier: SUPPLIER,
        reserve: w.borrow_reserve,
        liquidity_mint: BORROW_MINT,
        liquidity_vault: w.borrow_vault,
        share_mint: w.borrow_share_mint,
        supplier_liquidity: SUPPLIER_BORROW,
        supplier_share: SUPPLIER_BORROW_SHARE,
        amount,
    })
}

fn deposit_collateral_side(test: &mut Test, w: &Pdas, amount: u64) -> Outcome {
    test.send(DepositReserveLiquidityInstruction {
        supplier: BORROWER,
        reserve: w.collateral_reserve,
        liquidity_mint: COLLATERAL_MINT,
        liquidity_vault: w.collateral_vault,
        share_mint: w.collateral_share_mint,
        supplier_liquidity: BORROWER_COLLATERAL,
        supplier_share: BORROWER_COLLATERAL_SHARE,
        amount,
    })
}

fn redeem(test: &mut Test, w: &Pdas, shares: u64) -> Outcome {
    test.send(RedeemReserveCollateralInstruction {
        supplier: SUPPLIER,
        reserve: w.borrow_reserve,
        liquidity_mint: BORROW_MINT,
        liquidity_vault: w.borrow_vault,
        share_mint: w.borrow_share_mint,
        supplier_liquidity: SUPPLIER_BORROW,
        supplier_share: SUPPLIER_BORROW_SHARE,
        shares,
    })
}

fn borrow(test: &mut Test, w: &Pdas, amount: u64) -> Outcome {
    test.send(BorrowObligationLiquidityInstruction {
        owner: BORROWER,
        lending_market: w.market,
        collateral_reserve: w.collateral_reserve,
        collateral_price: w.collateral_price,
        borrow_reserve: w.borrow_reserve,
        borrow_price: w.borrow_price,
        liquidity_mint: BORROW_MINT,
        liquidity_vault: w.borrow_vault,
        owner_liquidity: BORROWER_BORROW,
        amount,
    })
}

fn repay(test: &mut Test, w: &Pdas, amount: u64) -> Outcome {
    test.send(RepayObligationLiquidityInstruction {
        repayer: BORROWER,
        obligation: w.obligation,
        borrow_reserve: w.borrow_reserve,
        liquidity_mint: BORROW_MINT,
        liquidity_vault: w.borrow_vault,
        repayer_liquidity: BORROWER_BORROW,
        amount,
    })
}

fn liquidate(test: &mut Test, w: &Pdas, amount: u64) -> Outcome {
    test.send(LiquidateObligationInstruction {
        liquidator: LIQUIDATOR,
        obligation: w.obligation,
        lending_market: w.market,
        collateral_reserve: w.collateral_reserve,
        collateral_price: w.collateral_price,
        share_mint: w.collateral_share_mint,
        liquidator_collateral: LIQUIDATOR_COLLATERAL_SHARE,
        borrow_reserve: w.borrow_reserve,
        borrow_price: w.borrow_price,
        liquidity_mint: BORROW_MINT,
        liquidity_vault: w.borrow_vault,
        liquidator_liquidity: LIQUIDATOR_BORROW,
        amount,
    })
}

/// Supplier funds the borrow reserve; borrower posts 1000 units of collateral.
fn bootstrap_position(test: &mut Test, w: &Pdas) {
    setup_markets(test, w);
    deposit_borrow_side(test, w, 1_000 * UNIT).succeeds();
    deposit_collateral_side(test, w, 1_000 * UNIT).succeeds();
    test.send(InitObligationInstruction {
        owner: BORROWER,
        lending_market: w.market,
    })
    .succeeds();
    test.send(DepositObligationCollateralInstruction {
        owner: BORROWER,
        lending_market: w.market,
        reserve: w.collateral_reserve,
        share_mint: w.collateral_share_mint,
        owner_share: BORROWER_COLLATERAL_SHARE,
        shares: 1_000 * UNIT,
    })
    .succeeds();
}

#[quasar_test]
fn supply_mints_shares_one_to_one_and_redeem_returns_liquidity(test: &mut Test) {
    let w = base_world(test);
    setup_markets(test, &w);

    deposit_borrow_side(test, &w, 1_000 * UNIT)
        .succeeds()
        // First deposit mints 1:1.
        .has_tokens(SUPPLIER_BORROW_SHARE, 1_000 * UNIT)
        .has_tokens(SUPPLIER_BORROW, 0);

    redeem(test, &w, 1_000 * UNIT)
        .succeeds()
        // Redeem returns liquidity.
        .has_tokens(SUPPLIER_BORROW, 1_000 * UNIT)
        .has_tokens(SUPPLIER_BORROW_SHARE, 0);
}

#[quasar_test]
fn borrow_up_to_ltv_succeeds_and_beyond_fails(test: &mut Test) {
    let w = base_world(test);
    bootstrap_position(test, &w);

    // $1000 collateral, 75% LTV => borrow up to 750 units of the $1 borrow token.
    borrow(test, &w, 750 * UNIT)
        .succeeds()
        .has_tokens(BORROWER_BORROW, 750 * UNIT);

    // One unit more exceeds the allowed borrow value.
    assert!(
        borrow(test, &w, UNIT).is_err(),
        "borrowing past LTV must fail"
    );
}

#[quasar_test]
fn repay_reduces_debt(test: &mut Test) {
    let w = base_world(test);
    bootstrap_position(test, &w);
    borrow(test, &w, 500 * UNIT).succeeds();

    // Borrower spent 200 of the 500 borrowed.
    repay(test, &w, 200 * UNIT)
        .succeeds()
        .has_tokens(BORROWER_BORROW, 300 * UNIT);
}

#[quasar_test]
fn unhealthy_position_is_liquidated_and_healthy_is_rejected(test: &mut Test) {
    let w = base_world(test);
    bootstrap_position(test, &w);
    borrow(test, &w, 700 * UNIT).succeeds();

    // Healthy at $1 collateral ($1000 * 80% = $800 threshold > $700 debt).
    assert!(
        liquidate(test, &w, 350 * UNIT).is_err(),
        "healthy obligation must not be liquidatable"
    );

    // Collateral price halves to $0.50: $500 collateral, $400 threshold < $700 debt.
    set_price(test, &w, COLLATERAL_MINT, cents(50));

    liquidate(test, &w, 350 * UNIT)
        .succeeds()
        // Liquidator repaid 350 of the borrow token...
        .has_tokens(LIQUIDATOR_BORROW, 650 * UNIT);
    // ...and seized collateral share tokens.
    assert!(
        test.tokens(LIQUIDATOR_COLLATERAL_SHARE) > 0,
        "liquidator should receive seized collateral shares"
    );
}

/// The two scenarios below warp the SLOT so that interest accrues
/// (`Clock::get()?.slot` drives the interest index). quasar-test has no slot
/// warp — `warp_to_timestamp` only moves `unix_timestamp` — so these keep the
/// low-level quasar-svm harness (`QuasarSvm` + `sysvars.warp_to_slot` + raw
/// instructions), loading the compiled program at runtime.
mod slot_warp {
    use {
        super::{dollars, EXP},
        super::{
            BORROWER, BORROWER_BORROW, BORROWER_COLLATERAL, BORROWER_COLLATERAL_SHARE,
            COLLATERAL_MINT, BORROW_MINT, DECIMALS, MARKET_ID, OWNER, OWNER_BORROW, QUOTE_MINT,
            SUPPLIER, SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE, UNIT,
        },
        quasar_svm::{Account, AccountMeta, Instruction, Pubkey, QuasarSvm},
        spl_token::state::{Account as SplToken, AccountState, Mint as SplMint},
    };

    fn pda(seeds: &[&[u8]]) -> (Pubkey, u8) {
        Pubkey::find_program_address(seeds, &crate::ID)
    }

    fn meta(address: Pubkey, writable: bool, signer: bool) -> AccountMeta {
        if writable {
            AccountMeta::new(address, signer)
        } else {
            AccountMeta::new_readonly(address, signer)
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
            owner: quasar_svm::system_program::ID,
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
        collateral_reserve: Pubkey,
        collateral_vault: Pubkey,
        collateral_share_mint: Pubkey,
        collateral_price: Pubkey,
        borrow_reserve: Pubkey,
        borrow_vault: Pubkey,
        borrow_share_mint: Pubkey,
        borrow_price: Pubkey,
        obligation: Pubkey,
        obligation_vault: Pubkey,
    }

    impl World {
        fn new() -> Self {
            // Runtime read (NOT include_bytes!) so the crate compiles without
            // the .so; only running the test requires a prior `quasar build`.
            let elf = std::fs::read("target/deploy/quasar_lending.so").unwrap();
            let mut svm = QuasarSvm::new()
                .with_program(&crate::ID, &elf)
                .with_token_program();

            let (market, _) = pda(&[b"lending_market", &MARKET_ID.to_le_bytes()]);
            let (collateral_reserve, _) =
                pda(&[b"reserve", market.as_ref(), COLLATERAL_MINT.as_ref()]);
            let (borrow_reserve, _) = pda(&[b"reserve", market.as_ref(), BORROW_MINT.as_ref()]);
            let (collateral_vault, _) = pda(&[b"liquidity_vault", collateral_reserve.as_ref()]);
            let (borrow_vault, _) = pda(&[b"liquidity_vault", borrow_reserve.as_ref()]);
            let (collateral_share_mint, _) = pda(&[b"share_mint", collateral_reserve.as_ref()]);
            let (borrow_share_mint, _) = pda(&[b"share_mint", borrow_reserve.as_ref()]);
            // Feed PDAs are seeded by (market, mint).
            let (collateral_price, _) =
                pda(&[b"price_feed", market.as_ref(), COLLATERAL_MINT.as_ref()]);
            let (borrow_price, _) = pda(&[b"price_feed", market.as_ref(), BORROW_MINT.as_ref()]);
            let (obligation, _) = pda(&[b"obligation", market.as_ref(), BORROWER.as_ref()]);
            let (obligation_vault, _) = pda(&[
                b"obligation_vault",
                collateral_reserve.as_ref(),
                obligation.as_ref(),
            ]);

            for account in [
                system(OWNER),
                system(SUPPLIER),
                system(BORROWER),
                mint(COLLATERAL_MINT, OWNER),
                mint(BORROW_MINT, OWNER),
                mint(QUOTE_MINT, OWNER),
                // PDAs created by the program.
                empty(market),
                empty(collateral_reserve),
                empty(borrow_reserve),
                empty(collateral_vault),
                empty(borrow_vault),
                empty(collateral_share_mint),
                empty(borrow_share_mint),
                empty(collateral_price),
                empty(borrow_price),
                empty(obligation),
                empty(obligation_vault),
                // Funded user token accounts.
                token(SUPPLIER_BORROW, BORROW_MINT, SUPPLIER, 1_000 * UNIT),
                token(SUPPLIER_BORROW_SHARE, borrow_share_mint, SUPPLIER, 0),
                token(BORROWER_COLLATERAL, COLLATERAL_MINT, BORROWER, 1_000 * UNIT),
                token(BORROWER_COLLATERAL_SHARE, collateral_share_mint, BORROWER, 0),
                token(BORROWER_BORROW, BORROW_MINT, BORROWER, 0),
                // Where the market owner receives collected protocol fees.
                token(OWNER_BORROW, BORROW_MINT, OWNER, 0),
            ] {
                svm.set_account(account);
            }

            World {
                svm,
                market,
                collateral_reserve,
                collateral_vault,
                collateral_share_mint,
                collateral_price,
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
                meta(quasar_svm::system_program::ID, false, false),
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
                meta(quasar_svm::system_program::ID, false, false),
            ];
            self.run(data, metas).assert_success();
        }

        fn init_reserve(
            &mut self,
            the_mint: Pubkey,
            reserve: Pubkey,
            vault: Pubkey,
            share: Pubkey,
            price: Pubkey,
        ) {
            // 75% LTV, 80% liquidation threshold, 5% bonus, 50% close factor,
            // 10% reserve factor, kink 80%, 2% / 20% / 150% APR curve.
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
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
                meta(quasar_svm::system_program::ID, false, false),
            ];
            self.run(data, metas).assert_success();
        }

        fn setup_markets(&mut self) {
            self.init_market();
            self.set_price(COLLATERAL_MINT, self.collateral_price, dollars(1));
            self.set_price(BORROW_MINT, self.borrow_price, dollars(1));
            self.init_reserve(
                COLLATERAL_MINT,
                self.collateral_reserve,
                self.collateral_vault,
                self.collateral_share_mint,
                self.collateral_price,
            );
            self.init_reserve(
                BORROW_MINT,
                self.borrow_reserve,
                self.borrow_vault,
                self.borrow_share_mint,
                self.borrow_price,
            );
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
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
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
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
            ];
            self.run(data, metas)
        }

        fn init_obligation(&mut self) {
            let metas = vec![
                meta(BORROWER, true, true),
                meta(self.market, false, false),
                meta(self.obligation, true, false),
                meta(quasar_svm::system_program::ID, false, false),
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
                meta(self.collateral_reserve, false, false),
                meta(self.collateral_share_mint, false, false),
                meta(self.obligation_vault, true, false),
                meta(BORROWER_COLLATERAL_SHARE, true, false),
                meta(quasar_svm::solana_sdk_ids::sysvar::rent::ID, false, false),
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
                meta(quasar_svm::system_program::ID, false, false),
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
                meta(self.collateral_reserve, true, false),
                meta(self.collateral_price, false, false),
                meta(self.borrow_reserve, true, false),
                meta(self.borrow_price, false, false),
                meta(BORROW_MINT, false, false),
                meta(self.borrow_vault, true, false),
                meta(BORROWER_BORROW, true, false),
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
            ];
            self.run(data, metas)
        }

        /// Supplier funds the borrow reserve; borrower posts 1000 units of collateral.
        fn bootstrap_position(&mut self) {
            self.setup_markets();
            self.deposit(
                SUPPLIER,
                self.borrow_reserve,
                BORROW_MINT,
                self.borrow_vault,
                self.borrow_share_mint,
                SUPPLIER_BORROW,
                SUPPLIER_BORROW_SHARE,
                1_000 * UNIT,
            )
            .assert_success();
            self.deposit(
                BORROWER,
                self.collateral_reserve,
                COLLATERAL_MINT,
                self.collateral_vault,
                self.collateral_share_mint,
                BORROWER_COLLATERAL,
                BORROWER_COLLATERAL_SHARE,
                1_000 * UNIT,
            )
            .assert_success();
            self.init_obligation();
            self.post_collateral(1_000 * UNIT).assert_success();
        }

        /// Market owner collects accrued protocol fees from the borrow reserve
        /// into `OWNER_BORROW`. The handler accrues interest itself, so no
        /// separate refresh.
        fn collect_borrow_fees(&mut self) -> quasar_svm::ExecutionResult {
            let metas = vec![
                meta(OWNER, true, true),
                meta(self.market, false, false),
                meta(self.borrow_reserve, true, false),
                meta(BORROW_MINT, false, false),
                meta(self.borrow_vault, true, false),
                meta(OWNER_BORROW, true, false),
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
            ];
            self.run(vec![11u8], metas)
        }
    }

    #[test]
    fn interest_accrues_and_lifts_share_value() {
        let mut world = World::new();
        world.bootstrap_position();
        world.borrow(500 * UNIT).assert_success();

        // ~0.1 year passes; re-publish prices so feeds stay fresh.
        world.svm.sysvars.warp_to_slot(7_884_000);
        world.set_price(COLLATERAL_MINT, world.collateral_price, dollars(1));
        world.set_price(BORROW_MINT, world.borrow_price, dollars(1));

        // Supplier redeems 100 shares; interest on the 500 borrowed means each
        // share is now worth more than one liquidity unit.
        let result = world.redeem(SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE, 100 * UNIT);
        result.assert_success();
        assert!(
            balance(&result, SUPPLIER_BORROW) > 100 * UNIT,
            "100 shares should redeem for more than 100 units after interest, got {}",
            balance(&result, SUPPLIER_BORROW)
        );
    }

    #[test]
    fn protocol_fees_accrue_and_owner_can_collect() {
        let mut world = World::new();
        world.bootstrap_position();
        world.borrow(500 * UNIT).assert_success();

        // ~0.1 year passes; interest accrues, and the reserve factor (10%)
        // sets some of it aside for the market owner.
        world.svm.sysvars.warp_to_slot(7_884_000);

        let result = world.collect_borrow_fees();
        result.assert_success();
        assert!(
            balance(&result, OWNER_BORROW) > 0,
            "owner should collect a positive protocol fee, got {}",
            balance(&result, OWNER_BORROW)
        );
    }
}
