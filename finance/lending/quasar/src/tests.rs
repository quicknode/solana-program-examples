//! Integration tests. Most scenarios drive the program through `quasar-test`
//! (`#[quasar_test]` fixtures + `crate::cpi` builders). The scenarios that must
//! move the slot and the Clock's timestamp independently (interest accrues on
//! the timestamp, price freshness is counted in slots) keep the low-level
//! QuasarSvm harness — see `clock_warp` at the bottom.

use {
    crate::{
        cpi::{
            BorrowObligationLiquidityInstruction, DepositObligationCollateralInstruction,
            DepositReserveLiquidityInstruction, InitializeLendingMarketInstruction,
            InitializeObligationInstruction, InitializeReserveInstruction,
            LiquidateObligationInstruction, RedeemReserveCollateralInstruction,
            RepayObligationLiquidityInstruction, SetPriceInstruction,
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

/// A tenth of a 365-day year, in seconds: long enough for interest to show.
const TENTH_OF_A_YEAR: i64 = crate::constants::SECONDS_PER_YEAR as i64 / 10;

// Deterministic addresses.
const OWNER: Pubkey = Pubkey::new_from_array([1; 32]);
const SUPPLIER: Pubkey = Pubkey::new_from_array([2; 32]);
const BORROWER: Pubkey = Pubkey::new_from_array([3; 32]);
const LIQUIDATOR: Pubkey = Pubkey::new_from_array([4; 32]);
const COLLATERAL_MINT: Pubkey = Pubkey::new_from_array([5; 32]);
const BORROW_MINT: Pubkey = Pubkey::new_from_array([6; 32]);
const QUOTE_MINT: Pubkey = Pubkey::new_from_array([7; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([8; 32]);
const VICTIM: Pubkey = Pubkey::new_from_array([9; 32]);
// Token accounts.
const SUPPLIER_BORROW: Pubkey = Pubkey::new_from_array([10; 32]);
const SUPPLIER_BORROW_SHARE: Pubkey = Pubkey::new_from_array([11; 32]);
const BORROWER_COLLATERAL: Pubkey = Pubkey::new_from_array([12; 32]);
const BORROWER_COLLATERAL_SHARE: Pubkey = Pubkey::new_from_array([13; 32]);
const BORROWER_BORROW: Pubkey = Pubkey::new_from_array([14; 32]);
const LIQUIDATOR_BORROW: Pubkey = Pubkey::new_from_array([15; 32]);
const LIQUIDATOR_COLLATERAL_SHARE: Pubkey = Pubkey::new_from_array([16; 32]);
const OWNER_BORROW: Pubkey = Pubkey::new_from_array([17; 32]);
const OWNER_COLLATERAL: Pubkey = Pubkey::new_from_array([18; 32]);
const OWNER_COLLATERAL_SHARE: Pubkey = Pubkey::new_from_array([19; 32]);
const OWNER_BORROW_SHARE: Pubkey = Pubkey::new_from_array([20; 32]);
const ATTACKER_BORROW: Pubkey = Pubkey::new_from_array([21; 32]);
const ATTACKER_BORROW_SHARE: Pubkey = Pubkey::new_from_array([22; 32]);
const ATTACKER_COLLATERAL: Pubkey = Pubkey::new_from_array([23; 32]);
const ATTACKER_COLLATERAL_SHARE: Pubkey = Pubkey::new_from_array([24; 32]);
const VICTIM_BORROW: Pubkey = Pubkey::new_from_array([25; 32]);
const VICTIM_BORROW_SHARE: Pubkey = Pubkey::new_from_array([26; 32]);
/// What the market owner deposits to open each reserve: the smallest first
/// deposit that clears the withheld minimum, minting the owner one share.
const OPENING_DEPOSIT: u64 = crate::constants::MINIMUM_SHARES + 1;
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
        collateral_price: test
            .derive_pda(crate::state::PriceFeed::seeds(&market, &COLLATERAL_MINT)),
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
    test.add(
        TokenAccount::new(w.collateral_share_mint, LIQUIDATOR).at(LIQUIDATOR_COLLATERAL_SHARE),
    );
    // The owner's opening deposits come from these; once they are made,
    // `OWNER_BORROW` is empty again and receives collected protocol fees.
    test.add(
        TokenAccount::new(BORROW_MINT, OWNER)
            .at(OWNER_BORROW)
            .amount(OPENING_DEPOSIT),
    );
    test.add(TokenAccount::new(w.borrow_share_mint, OWNER).at(OWNER_BORROW_SHARE));
    test.add(
        TokenAccount::new(COLLATERAL_MINT, OWNER)
            .at(OWNER_COLLATERAL)
            .amount(OPENING_DEPOSIT),
    );
    test.add(TokenAccount::new(w.collateral_share_mint, OWNER).at(OWNER_COLLATERAL_SHARE));
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

fn initialize_reserve(test: &mut Test, w: &Pdas, the_mint: Pubkey) {
    // 75% LTV, 80% liquidation threshold, 5% bonus, 50% close factor, 10%
    // reserve factor, kink 80%, 2% / 20% / 150% APR curve.
    test.send(InitializeReserveInstruction {
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

/// Create the market and both reserves, then open each reserve with the
/// owner's deposit so the withheld minimum is in place and later deposits mint
/// shares one-for-one until interest accrues.
fn setup_markets(test: &mut Test, w: &Pdas) {
    setup_empty_markets(test, w);
    test.send(DepositReserveLiquidityInstruction {
        supplier: OWNER,
        reserve: w.collateral_reserve,
        liquidity_mint: COLLATERAL_MINT,
        liquidity_vault: w.collateral_vault,
        share_mint: w.collateral_share_mint,
        supplier_liquidity: OWNER_COLLATERAL,
        supplier_share: OWNER_COLLATERAL_SHARE,
        amount: OPENING_DEPOSIT,
    })
    .succeeds();
    test.send(DepositReserveLiquidityInstruction {
        supplier: OWNER,
        reserve: w.borrow_reserve,
        liquidity_mint: BORROW_MINT,
        liquidity_vault: w.borrow_vault,
        share_mint: w.borrow_share_mint,
        supplier_liquidity: OWNER_BORROW,
        supplier_share: OWNER_BORROW_SHARE,
        amount: OPENING_DEPOSIT,
    })
    .succeeds();
}

/// Create the market and both reserves with no deposits, for tests of the
/// first deposit itself.
fn setup_empty_markets(test: &mut Test, w: &Pdas) {
    test.send(InitializeLendingMarketInstruction {
        owner: OWNER,
        quote_mint: QUOTE_MINT,
        market_id: MARKET_ID,
    })
    .succeeds();
    set_price(test, w, COLLATERAL_MINT, dollars(1));
    set_price(test, w, BORROW_MINT, dollars(1));
    initialize_reserve(test, w, COLLATERAL_MINT);
    initialize_reserve(test, w, BORROW_MINT);
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
    test.send(InitializeObligationInstruction {
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

/// The first deposit into a reserve mints one share per unit, less the
/// `MINIMUM_SHARES` withheld. Those shares belong to nobody, so even as the
/// only supplier the depositor gets back their deposit less the minimum, and
/// the minimum's liquidity stays in the pool.
#[quasar_test]
fn first_deposit_withholds_the_minimum(test: &mut Test) {
    let w = base_world(test);
    setup_empty_markets(test, &w);
    let minimum = crate::constants::MINIMUM_SHARES;

    deposit_borrow_side(test, &w, 1_000 * UNIT)
        .succeeds()
        .has_tokens(SUPPLIER_BORROW_SHARE, 1_000 * UNIT - minimum);

    redeem(test, &w, 1_000 * UNIT - minimum)
        .succeeds()
        .has_tokens(SUPPLIER_BORROW, 1_000 * UNIT - minimum)
        .has_tokens(SUPPLIER_BORROW_SHARE, 0);
}

/// A first deposit no larger than the minimum would mint nothing, so it is
/// refused.
#[quasar_test]
fn first_deposit_must_exceed_the_minimum(test: &mut Test) {
    let w = base_world(test);
    setup_empty_markets(test, &w);
    assert!(
        deposit_borrow_side(test, &w, crate::constants::MINIMUM_SHARES).is_err(),
        "a first deposit of only the minimum mints nothing and must be rejected"
    );
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

/// The scenarios below move the slot and the Clock's timestamp independently:
/// interest accrues on `unix_timestamp`, while price freshness and the restart
/// check are counted in slots. quasar-test has no slot warp (`warp_to_timestamp`
/// only moves `unix_timestamp`), so these keep the low-level quasar-svm harness
/// (`QuasarSvm` + `sysvars` + raw instructions), loading the compiled program at
/// runtime.
mod clock_warp {
    use {
        super::{dollars, EXP, TENTH_OF_A_YEAR},
        super::{
            ATTACKER, ATTACKER_BORROW, ATTACKER_BORROW_SHARE, ATTACKER_COLLATERAL,
            ATTACKER_COLLATERAL_SHARE, BORROWER, BORROWER_BORROW, BORROWER_COLLATERAL,
            BORROWER_COLLATERAL_SHARE, BORROW_MINT, COLLATERAL_MINT, DECIMALS, MARKET_ID,
            OPENING_DEPOSIT, OWNER, OWNER_BORROW, OWNER_BORROW_SHARE, OWNER_COLLATERAL,
            OWNER_COLLATERAL_SHARE, QUOTE_MINT, SUPPLIER, SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE,
            UNIT, VICTIM, VICTIM_BORROW, VICTIM_BORROW_SHARE,
        },
        crate::{
            constants::{FIXED_POINT_SCALE, MINIMUM_SHARES},
            math::{
                borrow_rate_per_second, current_debt, net_total_liquidity, total_shares,
                utilization_bps,
            },
            state::Reserve,
        },
        quasar_lang::traits::Discriminator,
        quasar_svm::{Account, AccountMeta, Instruction, Pubkey, QuasarSvm},
        spl_token::state::{Account as SplToken, AccountState, Mint as SplMint},
    };

    /// The reserve's zero-copy fields, as the program reads them.
    type ReserveState = <Reserve as core::ops::Deref>::Target;

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
    }

    impl World {
        fn new() -> Self {
            // Runtime read (NOT include_bytes!) so the crate compiles without
            // the .so; only running the test requires a prior `quasar build`.
            let elf = std::fs::read("target/deploy/quasar_lending.so").unwrap();
            let svm = QuasarSvm::new()
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
            let mut world = World {
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
            };
            let (borrower_obligation, borrower_obligation_vault) = world.obligation(BORROWER);
            let (attacker_obligation, attacker_obligation_vault) = world.obligation(ATTACKER);

            for account in [
                system(OWNER),
                system(SUPPLIER),
                system(BORROWER),
                system(ATTACKER),
                system(VICTIM),
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
                empty(borrower_obligation),
                empty(borrower_obligation_vault),
                empty(attacker_obligation),
                empty(attacker_obligation_vault),
                // Funded user token accounts.
                token(SUPPLIER_BORROW, BORROW_MINT, SUPPLIER, 1_000 * UNIT),
                token(SUPPLIER_BORROW_SHARE, borrow_share_mint, SUPPLIER, 0),
                token(BORROWER_COLLATERAL, COLLATERAL_MINT, BORROWER, 1_000 * UNIT),
                token(
                    BORROWER_COLLATERAL_SHARE,
                    collateral_share_mint,
                    BORROWER,
                    0,
                ),
                token(BORROWER_BORROW, BORROW_MINT, BORROWER, 0),
                // The owner's opening deposits come from these; once they are
                // made, `OWNER_BORROW` is empty again and receives collected
                // protocol fees.
                token(OWNER_BORROW, BORROW_MINT, OWNER, OPENING_DEPOSIT),
                token(OWNER_BORROW_SHARE, borrow_share_mint, OWNER, 0),
                token(OWNER_COLLATERAL, COLLATERAL_MINT, OWNER, OPENING_DEPOSIT),
                token(OWNER_COLLATERAL_SHARE, collateral_share_mint, OWNER, 0),
                token(ATTACKER_BORROW, BORROW_MINT, ATTACKER, 4_000 * UNIT),
                token(ATTACKER_BORROW_SHARE, borrow_share_mint, ATTACKER, 0),
                token(ATTACKER_COLLATERAL, COLLATERAL_MINT, ATTACKER, 1_000 * UNIT),
                token(
                    ATTACKER_COLLATERAL_SHARE,
                    collateral_share_mint,
                    ATTACKER,
                    0,
                ),
                token(VICTIM_BORROW, BORROW_MINT, VICTIM, 1_000 * UNIT),
                token(VICTIM_BORROW_SHARE, borrow_share_mint, VICTIM, 0),
            ] {
                world.svm.set_account(account);
            }

            world
        }

        /// The obligation PDA `owner` opens in this market, and the vault it
        /// holds posted collateral shares in.
        fn obligation(&self, owner: Pubkey) -> (Pubkey, Pubkey) {
            let (obligation, _) = pda(&[b"obligation", self.market.as_ref(), owner.as_ref()]);
            let (obligation_vault, _) = pda(&[
                b"obligation_vault",
                self.collateral_reserve.as_ref(),
                obligation.as_ref(),
            ]);
            (obligation, obligation_vault)
        }

        fn current_timestamp(&self) -> i64 {
            self.svm.sysvars.clock.unix_timestamp
        }

        /// Advance the slot only, leaving the Clock's timestamp where it is.
        /// Price freshness is counted in slots, so this ages prices; interest
        /// is counted in seconds, so on its own this accrues none.
        /// quasar-svm's `warp_to_slot` resets the timestamp to zero, so this
        /// puts it back.
        fn warp_slots(&mut self, slots: u64) {
            let timestamp = self.current_timestamp();
            let target = self.svm.sysvars.clock.slot + slots;
            self.svm.sysvars.warp_to_slot(target);
            self.svm.sysvars.clock.unix_timestamp = timestamp;
        }

        /// Move the Clock's timestamp by `seconds` (backwards when negative),
        /// leaving the slot where it is. Interest accrues on this clock.
        fn shift_timestamp(&mut self, seconds: i64) {
            self.svm.sysvars.clock.unix_timestamp += seconds;
        }

        /// Let `seconds` of wall-clock time pass: the timestamp moves by
        /// `seconds` and the slot by five a second, the network's 200 ms
        /// target, so prices age as they would. Interest accrues for exactly
        /// `seconds`, however many slots that turns out to be.
        fn warp_seconds(&mut self, seconds: i64) {
            self.warp_slots(seconds as u64 * 5);
            self.shift_timestamp(seconds);
        }

        /// Read a reserve's fields from its committed bytes: the discriminator,
        /// then the zero-copy layout, which is how `quasar-test`'s `Test::read`
        /// decodes an account.
        fn reserve(&self, address: Pubkey) -> ReserveState {
            let account = self.svm.get_account(&address).expect("reserve present");
            let discriminator = <Reserve as Discriminator>::DISCRIMINATOR;
            assert_eq!(&account.data[..discriminator.len()], discriminator);
            let fields = &account.data[discriminator.len()..];
            assert!(fields.len() >= core::mem::size_of::<ReserveState>());
            // SAFETY: the zero-copy layout has alignment one and no padding, and
            // the length check above keeps the read in bounds.
            unsafe { core::ptr::read_unaligned(fields.as_ptr() as *const ReserveState) }
        }

        /// Read an SPL token account's amount from the committed state, as
        /// `balance` reads it from one instruction's result.
        fn tokens(&self, address: Pubkey) -> u64 {
            let account = self.svm.get_account(&address).expect("account present");
            u64::from_le_bytes(account.data[64..72].try_into().unwrap())
        }

        /// The borrow reserve's liquidity, net of protocol fees, and the share
        /// count every conversion divides by, as the program prices them.
        fn borrow_reserve_totals(&self) -> (u128, u128) {
            let reserve = self.reserve(self.borrow_reserve);
            let total = net_total_liquidity(
                u64::from(reserve.available_liquidity),
                u128::from(reserve.borrowed_principal),
                u128::from(reserve.borrow_accumulation_factor),
                u64::from(reserve.accumulated_protocol_fees),
            )
            .unwrap();
            let shares = total_shares(u64::from(reserve.share_mint_supply)).unwrap();
            (total, shares)
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

        fn initialize_reserve(
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

        /// Create the market and both reserves, then open each reserve with the
        /// owner's deposit, as the quasar-test harness does.
        fn setup_markets(&mut self) {
            self.setup_empty_markets();
            self.open_collateral_reserve();
            self.open_borrow_reserve();
        }

        /// Create the market and both reserves with no deposits, for tests of
        /// the first deposit itself.
        fn setup_empty_markets(&mut self) {
            self.init_market();
            self.set_price(COLLATERAL_MINT, self.collateral_price, dollars(1));
            self.set_price(BORROW_MINT, self.borrow_price, dollars(1));
            self.initialize_reserve(
                COLLATERAL_MINT,
                self.collateral_reserve,
                self.collateral_vault,
                self.collateral_share_mint,
                self.collateral_price,
            );
            self.initialize_reserve(
                BORROW_MINT,
                self.borrow_reserve,
                self.borrow_vault,
                self.borrow_share_mint,
                self.borrow_price,
            );
        }

        fn open_collateral_reserve(&mut self) {
            self.deposit(
                OWNER,
                self.collateral_reserve,
                COLLATERAL_MINT,
                self.collateral_vault,
                self.collateral_share_mint,
                OWNER_COLLATERAL,
                OWNER_COLLATERAL_SHARE,
                OPENING_DEPOSIT,
            )
            .assert_success();
        }

        fn open_borrow_reserve(&mut self) {
            self.supply(OWNER, OWNER_BORROW, OWNER_BORROW_SHARE, OPENING_DEPOSIT)
                .assert_success();
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

        /// Deposit into the borrow reserve.
        fn supply(
            &mut self,
            supplier: Pubkey,
            supplier_liq: Pubkey,
            supplier_share: Pubkey,
            amount: u64,
        ) -> quasar_svm::ExecutionResult {
            self.deposit(
                supplier,
                self.borrow_reserve,
                BORROW_MINT,
                self.borrow_vault,
                self.borrow_share_mint,
                supplier_liq,
                supplier_share,
                amount,
            )
        }

        fn redeem(
            &mut self,
            supplier: Pubkey,
            supplier_liq: Pubkey,
            supplier_share: Pubkey,
            shares: u64,
        ) -> quasar_svm::ExecutionResult {
            let mut data = vec![4u8];
            data.extend_from_slice(&shares.to_le_bytes());
            let metas = vec![
                meta(supplier, true, true),
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

        fn initialize_obligation(&mut self, owner: Pubkey) {
            let (obligation, _) = self.obligation(owner);
            let metas = vec![
                meta(owner, true, true),
                meta(self.market, false, false),
                meta(obligation, true, false),
                meta(quasar_svm::system_program::ID, false, false),
            ];
            self.run(vec![5], metas).assert_success();
        }

        fn post_collateral(
            &mut self,
            owner: Pubkey,
            owner_share: Pubkey,
            shares: u64,
        ) -> quasar_svm::ExecutionResult {
            let (obligation, obligation_vault) = self.obligation(owner);
            let mut data = vec![6u8];
            data.extend_from_slice(&shares.to_le_bytes());
            let metas = vec![
                meta(owner, true, true),
                meta(self.market, false, false),
                meta(obligation, true, false),
                meta(self.collateral_reserve, false, false),
                meta(self.collateral_share_mint, false, false),
                meta(obligation_vault, true, false),
                meta(owner_share, true, false),
                meta(quasar_svm::solana_sdk_ids::sysvar::rent::ID, false, false),
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
                meta(quasar_svm::system_program::ID, false, false),
            ];
            self.run(data, metas)
        }

        fn borrow(
            &mut self,
            owner: Pubkey,
            owner_liquidity: Pubkey,
            amount: u64,
        ) -> quasar_svm::ExecutionResult {
            let (obligation, _) = self.obligation(owner);
            let mut data = vec![8u8];
            data.extend_from_slice(&amount.to_le_bytes());
            let metas = vec![
                meta(owner, true, true),
                meta(self.market, false, false),
                meta(obligation, true, false),
                meta(self.collateral_reserve, true, false),
                meta(self.collateral_price, false, false),
                meta(self.borrow_reserve, true, false),
                meta(self.borrow_price, false, false),
                meta(BORROW_MINT, false, false),
                meta(self.borrow_vault, true, false),
                meta(owner_liquidity, true, false),
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
            ];
            self.run(data, metas)
        }

        /// `repayer` pays down the debt on `owner`'s obligation. The handler
        /// caps the repayment at what is owed.
        fn repay(
            &mut self,
            repayer: Pubkey,
            repayer_liquidity: Pubkey,
            owner: Pubkey,
            amount: u64,
        ) -> quasar_svm::ExecutionResult {
            let (obligation, _) = self.obligation(owner);
            let mut data = vec![9u8];
            data.extend_from_slice(&amount.to_le_bytes());
            let metas = vec![
                meta(repayer, true, true),
                meta(obligation, true, false),
                meta(self.borrow_reserve, true, false),
                meta(BORROW_MINT, false, false),
                meta(self.borrow_vault, true, false),
                meta(repayer_liquidity, true, false),
                meta(quasar_svm::SPL_TOKEN_PROGRAM_ID, false, false),
            ];
            self.run(data, metas)
        }

        /// Supplier funds the borrow reserve; borrower posts 1000 units of collateral.
        fn bootstrap_position(&mut self) {
            self.setup_markets();
            self.supply(
                SUPPLIER,
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
            self.initialize_obligation(BORROWER);
            self.post_collateral(BORROWER, BORROWER_COLLATERAL_SHARE, 1_000 * UNIT)
                .assert_success();
        }

        /// Accrue the borrow reserve. This port has no `refresh_reserve`: every
        /// handler that reads a reserve accrues it first. Redeeming one share
        /// is the smallest such call that needs no price, so it stands in for
        /// the refresh here.
        fn refresh_borrow_reserve(&mut self) {
            self.redeem(SUPPLIER, SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE, 1)
                .assert_success();
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
        world
            .borrow(BORROWER, BORROWER_BORROW, 500 * UNIT)
            .assert_success();

        // A tenth of a year passes; re-publish prices so feeds stay fresh.
        world.warp_seconds(TENTH_OF_A_YEAR);
        world.set_price(COLLATERAL_MINT, world.collateral_price, dollars(1));
        world.set_price(BORROW_MINT, world.borrow_price, dollars(1));

        // Supplier redeems 100 shares; interest on the 500 borrowed means each
        // share is now worth more than one liquidity unit.
        let result = world.redeem(SUPPLIER, SUPPLIER_BORROW, SUPPLIER_BORROW_SHARE, 100 * UNIT);
        result.assert_success();
        assert!(
            balance(&result, SUPPLIER_BORROW) > 100 * UNIT,
            "100 shares should redeem for more than 100 units after interest, got {}",
            balance(&result, SUPPLIER_BORROW)
        );
    }

    /// A cluster restart passes hours of wall-clock time in zero slots, so a
    /// price published before the halt can still look fresh by slot count.
    /// `price_scaled` must reject it until the publisher posts again.
    #[test]
    fn borrow_with_price_from_before_a_restart_is_rejected() {
        let mut world = World::new();
        world.bootstrap_position();

        // Prices were published at the current slot. Simulate a halt: the
        // cluster restarts a few slots later, well inside the staleness
        // window, so only the restart check can catch the pre-halt price.
        let restart_slot = world.svm.sysvars.clock.slot + 3;
        world.svm.sysvars.warp_to_slot(restart_slot + 2);
        world.svm.sysvars.last_restart_slot.last_restart_slot = restart_slot;

        world
            .borrow(BORROWER, BORROWER_BORROW, 100 * UNIT)
            .assert_error(quasar_svm::ProgramError::Custom(
                crate::error::LendingError::PricePredatesRestart as u32,
            ));

        // Publishing after the restart reopens the market.
        world.set_price(COLLATERAL_MINT, world.collateral_price, dollars(1));
        world.set_price(BORROW_MINT, world.borrow_price, dollars(1));
        world
            .borrow(BORROWER, BORROWER_BORROW, 100 * UNIT)
            .assert_success();
    }

    /// The factor after one accrual `seconds` after the last: one multiply by
    /// `1 + rate_per_second * seconds`, floored, exactly as the program does it.
    fn factor_after(reserve: &ReserveState, seconds: u128) -> u128 {
        let factor = u128::from(reserve.borrow_accumulation_factor);
        let utilization = utilization_bps(
            u64::from(reserve.available_liquidity),
            u128::from(reserve.borrowed_principal),
            factor,
        )
        .unwrap();
        let rate = borrow_rate_per_second(
            utilization,
            u16::from(reserve.optimal_utilization_bps),
            u16::from(reserve.min_borrow_rate_bps),
            u16::from(reserve.optimal_borrow_rate_bps),
            u16::from(reserve.max_borrow_rate_bps),
        )
        .unwrap();
        factor * (FIXED_POINT_SCALE + rate * seconds) / FIXED_POINT_SCALE
    }

    /// The rate fields are annual, and a year is a length of wall-clock time, so
    /// interest accrues for the seconds on the Clock's timestamp and ignores the
    /// slot count. Slots passing on their own accrue nothing; seconds passing on
    /// their own accrue exactly the per-second rate times the seconds. A shorter
    /// or longer slot therefore cannot change what a borrower pays.
    #[test]
    fn interest_accrues_by_seconds_not_slots() {
        let mut world = World::new();
        world.bootstrap_position();
        world
            .borrow(BORROWER, BORROWER_BORROW, 500 * UNIT)
            .assert_success();
        let before = world.reserve(world.borrow_reserve);

        world.warp_slots(1_000_000);
        world.refresh_borrow_reserve();
        let idle = world.reserve(world.borrow_reserve);
        assert_eq!(
            u128::from(idle.borrow_accumulation_factor),
            u128::from(before.borrow_accumulation_factor),
            "a million slots with the clock standing still must accrue nothing"
        );

        let seconds = TENTH_OF_A_YEAR;
        world.shift_timestamp(seconds);
        world.refresh_borrow_reserve();
        let after = world.reserve(world.borrow_reserve);
        assert!(
            u128::from(after.borrow_accumulation_factor)
                > u128::from(before.borrow_accumulation_factor)
        );
        assert_eq!(
            u128::from(after.borrow_accumulation_factor),
            factor_after(&idle, seconds as u128)
        );
        assert_eq!(
            i64::from(after.last_accrual_timestamp),
            world.current_timestamp()
        );
    }

    /// The leader writes the timestamp, and a timestamp at or before the stored
    /// one accrues nothing and leaves the stored stamp alone. When the clock
    /// moves forward again, only the seconds past the stored stamp are charged,
    /// so no second is charged twice.
    #[test]
    fn a_timestamp_behind_the_last_accrual_charges_nothing() {
        let mut world = World::new();
        world.bootstrap_position();
        world
            .borrow(BORROWER, BORROWER_BORROW, 500 * UNIT)
            .assert_success();
        let before = world.reserve(world.borrow_reserve);

        world.shift_timestamp(-600);
        world.refresh_borrow_reserve();
        let behind = world.reserve(world.borrow_reserve);
        assert_eq!(
            u128::from(behind.borrow_accumulation_factor),
            u128::from(before.borrow_accumulation_factor)
        );
        assert_eq!(
            i64::from(behind.last_accrual_timestamp),
            i64::from(before.last_accrual_timestamp)
        );

        // 1,600 seconds forward from the shifted clock is 1,000 past the stamp.
        world.shift_timestamp(1_600);
        world.refresh_borrow_reserve();
        assert_eq!(
            u128::from(
                world
                    .reserve(world.borrow_reserve)
                    .borrow_accumulation_factor
            ),
            factor_after(&behind, 1_000)
        );
    }

    #[test]
    fn protocol_fees_accrue_and_owner_can_collect() {
        let mut world = World::new();
        world.bootstrap_position();
        world
            .borrow(BORROWER, BORROWER_BORROW, 500 * UNIT)
            .assert_success();

        // A tenth of a year passes; interest accrues, and the reserve factor
        // (10%) sets some of it aside for the market owner.
        world.warp_seconds(TENTH_OF_A_YEAR);

        let result = world.collect_borrow_fees();
        result.assert_success();
        assert!(
            balance(&result, OWNER_BORROW) > 0,
            "owner should collect a positive protocol fee, got {}",
            balance(&result, OWNER_BORROW)
        );
    }

    /// First-depositor share inflation without a donation. Shares are priced
    /// against tracked total liquidity, not the vault balance, so tokens sent
    /// straight to the vault move nothing. But total liquidity also counts
    /// interest owed on borrows, and a supplier can borrow from their own
    /// reserve.
    ///
    /// The attacker opens the reserve holding a single share, borrows one base
    /// unit of it against collateral in another reserve, and lets one second
    /// pass. Debt is rounded up, so the one unit now reads as two and the lone
    /// share is worth two units without having minted anything. From there,
    /// deposits and redemptions that round down in the pool's favor ratchet
    /// the price up: each deposit is the largest that still mints one share,
    /// and redeeming that share leaves the rounding behind for the only other
    /// share, the attacker's own. A deposit that would mint zero shares is
    /// refused (`DepositTooSmall`), so the victim is not robbed outright;
    /// instead a deposit just under two shares' worth mints one, and the
    /// attacker's share redeems half the pool.
    ///
    /// The rate curve is the harness's usual one. Nothing about the attack
    /// needs the market owner's cooperation or bad debt: the attacker repays
    /// what they owe.
    ///
    /// `MINIMUM_SHARES` counts as shares nobody holds in every share
    /// conversion, so the attacker's one share is 1 of 1_001 and whatever the
    /// rounding leaves behind is spread mostly across shares they cannot
    /// redeem.
    #[test]
    fn inflating_shares_through_own_borrow_does_not_pay() {
        let mut world = World::new();
        world.setup_empty_markets();
        world.open_collateral_reserve();
        let budget = world.tokens(ATTACKER_BORROW);

        // Open the reserve with two shares. This port has no `refresh_reserve`,
        // so the second share is spent below as the refresh, leaving the
        // attacker holding exactly one.
        world
            .supply(
                ATTACKER,
                ATTACKER_BORROW,
                ATTACKER_BORROW_SHARE,
                MINIMUM_SHARES + 2,
            )
            .assert_success();
        assert_eq!(world.tokens(ATTACKER_BORROW_SHARE), 2);

        // Borrow one base unit against collateral in another reserve, and let a
        // second of interest round that debt up to two.
        world
            .deposit(
                ATTACKER,
                world.collateral_reserve,
                COLLATERAL_MINT,
                world.collateral_vault,
                world.collateral_share_mint,
                ATTACKER_COLLATERAL,
                ATTACKER_COLLATERAL_SHARE,
                1_000 * UNIT,
            )
            .assert_success();
        world.initialize_obligation(ATTACKER);
        let collateral_shares = world.tokens(ATTACKER_COLLATERAL_SHARE);
        world
            .post_collateral(ATTACKER, ATTACKER_COLLATERAL_SHARE, collateral_shares)
            .assert_success();
        world.borrow(ATTACKER, ATTACKER_BORROW, 1).assert_success();
        world.warp_seconds(1);
        world
            .redeem(ATTACKER, ATTACKER_BORROW, ATTACKER_BORROW_SHARE, 1)
            .assert_success();
        assert_eq!(world.tokens(ATTACKER_BORROW_SHARE), 1);
        let reserve = world.reserve(world.borrow_reserve);
        let debt = current_debt(
            u128::from(reserve.borrowed_principal),
            u128::from(reserve.borrow_accumulation_factor),
        )
        .unwrap();
        assert_eq!(debt, 2);

        // Ratchet until one share is worth more than half the victim's deposit.
        let victim_deposit = world.tokens(VICTIM_BORROW);
        for _ in 0..64 {
            let (total, shares) = world.borrow_reserve_totals();
            if total * 2 > victim_deposit as u128 * shares {
                break;
            }
            // The largest deposit that mints exactly one share.
            let deposit = (2 * total).div_ceil(shares) - 1;
            world
                .supply(
                    ATTACKER,
                    ATTACKER_BORROW,
                    ATTACKER_BORROW_SHARE,
                    deposit as u64,
                )
                .assert_success();
            world
                .redeem(ATTACKER, ATTACKER_BORROW, ATTACKER_BORROW_SHARE, 1)
                .assert_success();
        }

        world
            .supply(VICTIM, VICTIM_BORROW, VICTIM_BORROW_SHARE, victim_deposit)
            .assert_success();

        // The attacker exits: redeems their share and repays the debt.
        let attacker_shares = world.tokens(ATTACKER_BORROW_SHARE);
        world
            .redeem(
                ATTACKER,
                ATTACKER_BORROW,
                ATTACKER_BORROW_SHARE,
                attacker_shares,
            )
            .assert_success();
        world
            .repay(ATTACKER, ATTACKER_BORROW, ATTACKER, 10)
            .assert_success();
        assert_eq!(
            u128::from(world.reserve(world.borrow_reserve).borrowed_principal),
            0
        );
        let attacker_end = world.tokens(ATTACKER_BORROW);
        assert!(
            attacker_end <= budget,
            "attacker started with {budget} and ended with {attacker_end}"
        );

        // The victim exits with everything and gets back all but a sliver.
        let victim_shares = world.tokens(VICTIM_BORROW_SHARE);
        world
            .redeem(VICTIM, VICTIM_BORROW, VICTIM_BORROW_SHARE, victim_shares)
            .assert_success();
        let victim_back = world.tokens(VICTIM_BORROW);
        assert!(
            victim_back * 1_000 >= victim_deposit * 999,
            "victim deposited {victim_deposit} and got back {victim_back}"
        );
    }
}
