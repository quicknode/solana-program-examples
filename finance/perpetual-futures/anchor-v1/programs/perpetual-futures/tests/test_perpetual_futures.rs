use {
    anchor_lang::{
        error::ErrorCode as AnchorErrorCode,
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    perpetual_futures::{
        errors::PerpError,
        instructions::{
            initialize_pool::PoolParameters,
            shared::{basis_points_of, basis_points_of_rounded_down},
        },
        state::{Pool, Position, Side},
    },
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
    solana_signer::Signer,
};

// Matches `MAX_FUNDING_RATE_PER_SECOND` in the program's constants: the
// steepest funding rate `initialize_pool` accepts.
const MAX_FUNDING_RATE_PER_SECOND: u64 = 277;
// Matches `PRICE_AVERAGE_WINDOW_SECONDS`: one fold after this many seconds
// replaces the pool's average price with the oracle price.
const PRICE_AVERAGE_WINDOW_SECONDS: i64 = 600;
// Ten years, in seconds.
const TEN_YEARS: i64 = 315_360_000;
// The test market's profit warm-up: a position can be closed at a profit from
// this many slots after it opened.
const PROFIT_WARMUP_SLOTS: u64 = 10;
// Matches `HAIRCUT_PRECISION`: a haircut ratio of one.
const HAIRCUT_PRECISION: u64 = 1_000_000_000;
// Matches `FUNDING_PRECISION`: the fixed point the funding rate and index are
// carried in, so a position's funding is `size * rate * seconds / this`.
const FUNDING_PRECISION: u64 = 1_000_000_000;
// Collateral token has 6 decimals (like USDC), so one whole unit is 1_000_000
// base units.
const ONE_USDC: u64 = 1_000_000;
const DECIMALS: u8 = 6;

// The oracle quotes prices with 8 decimals, so $100 is 100 * 10^8.
const ORACLE_SCALE: u32 = 8;

fn token_program_id() -> Pubkey {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

fn ata_program_id() -> Pubkey {
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap()
}

fn derive_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id().as_ref(), mint.as_ref()],
        &ata_program_id(),
    )
    .0
}

/// Oracle price for a whole-dollar amount, in the feed's fixed point.
fn dollars(whole: i128) -> i128 {
    whole * 10i128.pow(ORACLE_SCALE)
}

/// The parameters every test market uses unless a test overrides one: 0.1%
/// open and close fees, half of each fee paid into the insurance fund, a 10%
/// initial margin (10x leverage), a 5% maintenance margin, a 1% liquidation
/// fee, a 1% maximum confidence band, a 20% price band around the pool's
/// average price, and a 10-slot profit warm-up.
fn default_parameters(funding_rate_per_second: u64) -> PoolParameters {
    PoolParameters {
        oracle_scale: ORACLE_SCALE,
        funding_rate_per_second,
        open_fee_bps: 10,
        close_fee_bps: 10,
        initial_margin_bps: 1_000,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        max_confidence_bps: 100,
        max_price_deviation_bps: 2_000,
        insurance_fee_bps: 5_000,
        profit_warmup_slots: PROFIT_WARMUP_SLOTS,
    }
}

/// Assert that `result` failed with the program's `expected` error. Anchor
/// reports a program error as `Custom(6000 + the variant's index)`.
fn assert_fails_with<T>(result: Result<T, String>, expected: PerpError) {
    assert_fails_with_code(result, expected as u32 + 6000);
}

/// Assert that `result` failed on one of Anchor's own account constraints,
/// which report their code without the program-error offset.
fn assert_fails_with_anchor_error<T>(result: Result<T, String>, expected: AnchorErrorCode) {
    assert_fails_with_code(result, expected as u32);
}

fn assert_fails_with_code<T>(result: Result<T, String>, code: u32) {
    let Err(error) = result else {
        panic!("the transaction should have failed with error code {code}");
    };
    assert!(
        error.contains(&format!("Custom({code})")),
        "expected error code {code}, got: {error}"
    );
}

/// One deployed market plus the keys needed to drive it.
struct Market {
    svm: LiteSVM,
    payer: Keypair,
    admin: Keypair,
    collateral_mint: Pubkey,
    feed: Pubkey,
    pool: Pubkey,
    lp_mint: Pubkey,
    custody_vault: Pubkey,
}

impl Market {
    /// Stand up a market with the given starting oracle price and per-second
    /// funding rate. The admin is both the pool operator and the oracle feed
    /// authority.
    fn new(initial_price: i128, funding_rate_per_second: u64) -> Market {
        Market::try_new(initial_price, default_parameters(funding_rate_per_second))
            .expect("pool initialization should succeed")
    }

    /// Like `new`, but takes the full parameter set and surfaces an
    /// `initialize_pool` rejection instead of panicking, so tests can probe the
    /// parameter validation.
    fn try_new(initial_price: i128, parameters: PoolParameters) -> Result<Market, String> {
        let mut svm = LiteSVM::new();
        svm.add_program(
            perpetual_futures::id(),
            include_bytes!("../../../target/deploy/perpetual_futures.so"),
        )
        .unwrap();
        // Use std::fs::read() instead of include_bytes!() for the mock feed program because
        // include_bytes!() runs at compile time, and during `anchor build` the IDL generation
        // step compiles tests before the .so files exist. Since this is a cross-program
        // dependency (not our own program), mock_price_feed.so may not be built yet at compile time.
        let mock_feed_bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/deploy/mock_price_feed.so"
        ))
        .expect("mock_price_feed.so not found - run `anchor build` first");
        svm.add_program(mock_price_feed::id(), &mock_feed_bytes)
            .unwrap();

        let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let admin = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let collateral_mint = create_token_mint(&mut svm, &admin, DECIMALS, None).unwrap();

        // Create the mock oracle feed as a fresh account owned by the mock
        // program; the admin is its update authority.
        let feed_keypair = Keypair::new();
        let initialize_feed = Instruction::new_with_bytes(
            mock_price_feed::id(),
            &mock_price_feed::instruction::InitializeFeed {
                price: initial_price,
                scale: ORACLE_SCALE,
                confidence: 0,
            }
            .data(),
            mock_price_feed::accounts::InitializeFeedAccountConstraints {
                feed: feed_keypair.pubkey(),
                authority: admin.pubkey(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut svm,
            vec![initialize_feed],
            &[&admin, &feed_keypair],
            &admin.pubkey(),
        )
        .unwrap();
        let feed = feed_keypair.pubkey();

        let pool = Pubkey::find_program_address(
            &[b"pool", collateral_mint.as_ref(), feed.as_ref()],
            &perpetual_futures::id(),
        )
        .0;
        let lp_mint =
            Pubkey::find_program_address(&[b"lp_mint", pool.as_ref()], &perpetual_futures::id()).0;
        let custody_vault =
            Pubkey::find_program_address(&[b"vault", pool.as_ref()], &perpetual_futures::id()).0;

        let initialize_pool = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::InitializePool { parameters }.data(),
            perpetual_futures::accounts::InitializePoolAccountConstraints {
                authority: admin.pubkey(),
                pool,
                collateral_mint,
                oracle_feed: feed,
                lp_mint,
                custody_vault,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut svm,
            vec![initialize_pool],
            &[&admin],
            &admin.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))?;

        Ok(Market {
            svm,
            payer,
            admin,
            collateral_mint,
            feed,
            pool,
            lp_mint,
            custody_vault,
        })
    }

    fn default_market() -> Market {
        // Funding off by default so profit/loss assertions are exact.
        Market::new(dollars(100), 0)
    }

    fn pool_state(&self) -> Pool {
        let account = self.svm.get_account(&self.pool).unwrap();
        Pool::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    fn set_price(&mut self, price: i128) {
        self.set_price_with_confidence(price, 0);
    }

    fn set_price_with_confidence(&mut self, price: i128, confidence: u64) {
        let set_price = Instruction::new_with_bytes(
            mock_price_feed::id(),
            &mock_price_feed::instruction::SetPrice { price, confidence }.data(),
            mock_price_feed::accounts::SetPriceAccountConstraints {
                feed: self.feed,
                authority: self.admin.pubkey(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![set_price],
            &[&self.admin],
            &self.admin.pubkey(),
        )
        .unwrap();
    }

    /// Move to `slot`, leaving the Clock's timestamp where it is. Price
    /// freshness is counted in slots, so this ages prices; funding is counted
    /// in seconds, so on its own this accrues none.
    fn warp(&mut self, slot: u64) {
        self.svm.warp_to_slot(slot);
        self.svm.expire_blockhash();
    }

    /// Let `seconds` of wall-clock time pass: the timestamp moves by `seconds`
    /// and the slot by five a second, the network's 200 ms target. Funding
    /// accrues for exactly `seconds`, however many slots that turns out to be.
    fn pass_seconds(&mut self, seconds: i64) {
        let target = self.current_slot() + seconds as u64 * 5;
        self.svm.warp_to_slot(target);
        let mut clock = self.svm.get_sysvar::<anchor_lang::prelude::Clock>();
        clock.unix_timestamp += seconds;
        self.svm.set_sysvar(&clock);
        self.svm.expire_blockhash();
    }

    fn current_slot(&self) -> u64 {
        self.svm.get_sysvar::<anchor_lang::prelude::Clock>().slot
    }

    /// Let the profit warm-up pass, so a position opened in the current slot
    /// can be closed at a profit. The caller republishes the price after.
    fn pass_warmup(&mut self) {
        let slot = self.current_slot();
        self.warp(slot + PROFIT_WARMUP_SLOTS);
    }

    fn position_state(&self, owner: &Pubkey, side: Side) -> Position {
        let account = self
            .svm
            .get_account(&self.position_pda(owner, side))
            .unwrap();
        Position::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    /// Assert the custody vault holds exactly what the pool's ledger says it
    /// does: liquidity, open positions' collateral, program fees and the
    /// insurance fund.
    fn assert_vault_matches_ledger(&self) {
        let pool = self.pool_state();
        assert_eq!(
            get_token_account_balance(&self.svm, &self.custody_vault).unwrap(),
            pool.liquidity + pool.total_collateral + pool.program_fees + pool.insurance_fund
        );
    }

    /// A wallet with an empty collateral token account, to liquidate from.
    fn liquidator(&mut self) -> (Keypair, Pubkey) {
        let liquidator = create_wallet(&mut self.svm, 100_000_000_000).unwrap();
        let liquidator_collateral = create_associated_token_account(
            &mut self.svm,
            &liquidator.pubkey(),
            &self.collateral_mint,
            &self.payer,
        )
        .unwrap();
        (liquidator, liquidator_collateral)
    }

    /// Replace the feed account at the pool's recorded feed address with a
    /// copy whose owning program is `owner`. The bytes are unchanged, so the
    /// copy still decodes as a fresh, confident price at the pinned scale.
    fn set_feed_owner(&mut self, owner: Pubkey) {
        let mut feed_account = self.svm.get_account(&self.feed).unwrap();
        feed_account.owner = owner;
        self.svm.set_account(self.feed, feed_account).unwrap();
    }

    /// Simulate a cluster restart at `slot`: prices stamped at or before it
    /// must be rejected until the publisher posts again.
    fn set_last_restart_slot(&mut self, slot: u64) {
        self.svm
            .set_sysvar(&solana_sysvar::last_restart_slot::LastRestartSlot {
                last_restart_slot: slot,
            });
    }

    /// Create a wallet holding `amount` collateral tokens in its associated
    /// token account.
    fn funded_trader(&mut self, amount: u64) -> (Keypair, Pubkey) {
        let trader = create_wallet(&mut self.svm, 100_000_000_000).unwrap();
        let token_account = create_associated_token_account(
            &mut self.svm,
            &trader.pubkey(),
            &self.collateral_mint,
            &self.payer,
        )
        .unwrap();
        mint_tokens_to_token_account(
            &mut self.svm,
            &self.collateral_mint,
            &token_account,
            amount,
            &self.admin,
        )
        .unwrap();
        (trader, token_account)
    }

    fn add_liquidity(
        &mut self,
        provider: &Keypair,
        provider_collateral: Pubkey,
        amount: u64,
        minimum_shares_out: u64,
    ) -> Result<(), String> {
        let provider_lp = derive_ata(&provider.pubkey(), &self.lp_mint);
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::AddLiquidity {
                amount,
                minimum_shares_out,
            }
            .data(),
            perpetual_futures::accounts::AddLiquidityAccountConstraints {
                provider: provider.pubkey(),
                pool: self.pool,
                oracle_feed: self.feed,
                collateral_mint: self.collateral_mint,
                lp_mint: self.lp_mint,
                custody_vault: self.custody_vault,
                provider_collateral,
                provider_lp,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[provider],
            &provider.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))
    }

    fn remove_liquidity(
        &mut self,
        provider: &Keypair,
        provider_collateral: Pubkey,
        shares: u64,
        minimum_amount_out: u64,
    ) -> Result<(), String> {
        let provider_lp = derive_ata(&provider.pubkey(), &self.lp_mint);
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::RemoveLiquidity {
                shares,
                minimum_amount_out,
            }
            .data(),
            perpetual_futures::accounts::RemoveLiquidityAccountConstraints {
                provider: provider.pubkey(),
                pool: self.pool,
                oracle_feed: self.feed,
                collateral_mint: self.collateral_mint,
                lp_mint: self.lp_mint,
                custody_vault: self.custody_vault,
                provider_collateral,
                provider_lp,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[provider],
            &provider.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))
    }

    fn position_pda(&self, owner: &Pubkey, side: Side) -> Pubkey {
        let side_seed: &[u8] = match side {
            Side::Long => b"long",
            Side::Short => b"short",
        };
        Pubkey::find_program_address(
            &[b"position", self.pool.as_ref(), owner.as_ref(), side_seed],
            &perpetual_futures::id(),
        )
        .0
    }

    fn open_position(
        &mut self,
        trader: &Keypair,
        trader_collateral: Pubkey,
        side: Side,
        collateral_amount: u64,
        size: u64,
        acceptable_price: u64,
    ) -> Result<(), String> {
        let position = self.position_pda(&trader.pubkey(), side);
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::OpenPosition {
                side,
                collateral_amount,
                size,
                acceptable_price,
            }
            .data(),
            perpetual_futures::accounts::OpenPositionAccountConstraints {
                owner: trader.pubkey(),
                pool: self.pool,
                position,
                oracle_feed: self.feed,
                collateral_mint: self.collateral_mint,
                custody_vault: self.custody_vault,
                trader_collateral,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[trader],
            &trader.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))
    }

    fn close_position(
        &mut self,
        trader: &Keypair,
        trader_collateral: Pubkey,
        side: Side,
        minimum_payout: u64,
    ) -> Result<(), String> {
        let position = self.position_pda(&trader.pubkey(), side);
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::ClosePosition { minimum_payout }.data(),
            perpetual_futures::accounts::ClosePositionAccountConstraints {
                owner: trader.pubkey(),
                pool: self.pool,
                position,
                oracle_feed: self.feed,
                collateral_mint: self.collateral_mint,
                custody_vault: self.custody_vault,
                trader_collateral,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[trader],
            &trader.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))
    }

    fn liquidate(
        &mut self,
        liquidator: &Keypair,
        owner: &Pubkey,
        owner_collateral: Pubkey,
        side: Side,
    ) -> Result<(), String> {
        let position = self.position_pda(owner, side);
        let liquidator_collateral = derive_ata(&liquidator.pubkey(), &self.collateral_mint);
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::LiquidatePosition {}.data(),
            perpetual_futures::accounts::LiquidatePositionAccountConstraints {
                liquidator: liquidator.pubkey(),
                owner: *owner,
                pool: self.pool,
                position,
                oracle_feed: self.feed,
                collateral_mint: self.collateral_mint,
                custody_vault: self.custody_vault,
                trader_collateral: owner_collateral,
                liquidator_collateral,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[liquidator],
            &liquidator.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))
    }

    fn collect_fees(&mut self, authority: &Keypair) -> Result<(), String> {
        let authority_collateral = derive_ata(&authority.pubkey(), &self.collateral_mint);
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::CollectFees {}.data(),
            perpetual_futures::accounts::CollectFeesAccountConstraints {
                authority: authority.pubkey(),
                pool: self.pool,
                collateral_mint: self.collateral_mint,
                custody_vault: self.custody_vault,
                authority_collateral,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[authority],
            &authority.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))
    }

    fn update_price_average(&mut self, caller: &Keypair) -> Result<(), String> {
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::UpdatePriceAverage {}.data(),
            perpetual_futures::accounts::UpdatePriceAverageAccountConstraints {
                caller: caller.pubkey(),
                pool: self.pool,
                oracle_feed: self.feed,
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[caller],
            &caller.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))
    }

    /// Hold the oracle at `price` while the pool's average catches up with
    /// it: one update records `price` as the latest observation, then a full
    /// averaging window passes with the price republished so it is fresh, and
    /// a second update credits that window to `price`. A price more than the
    /// band away from the average cannot be traded at until this has run.
    fn settle_average_at(&mut self, price: i128) {
        let caller = self.payer.insecure_clone();
        self.update_price_average(&caller).unwrap();
        self.pass_seconds(PRICE_AVERAGE_WINDOW_SECONDS);
        self.set_price(price);
        self.update_price_average(&caller).unwrap();
    }

    /// Deposit a large amount of liquidity so the pool can pay trader profits,
    /// returning the provider and its collateral account.
    fn seed_liquidity(&mut self, amount: u64) -> (Keypair, Pubkey) {
        let (provider, provider_collateral) = self.funded_trader(amount);
        self.add_liquidity(&provider, provider_collateral, amount, 0)
            .unwrap();
        (provider, provider_collateral)
    }
}

#[test]
fn test_initialize_pool() {
    let market = Market::default_market();
    let pool = market.pool_state();

    assert_eq!(pool.authority, market.admin.pubkey());
    assert_eq!(pool.collateral_mint, market.collateral_mint);
    assert_eq!(pool.oracle_feed, market.feed);
    assert_eq!(pool.price_feed_program, mock_price_feed::id());
    assert_eq!(pool.oracle_scale, ORACLE_SCALE);
    assert_eq!(pool.initial_margin_bps, 1_000);
    assert_eq!(pool.max_price_deviation_bps, 2_000);
    // The average starts at the oracle price the pool was created against.
    assert_eq!(pool.average_price, dollars(100) as u64);
    assert_eq!(
        pool.average_price_timestamp,
        market
            .svm
            .get_sysvar::<anchor_lang::prelude::Clock>()
            .unix_timestamp
    );
    assert_eq!(pool.liquidity, 0);
    assert_eq!(pool.total_collateral, 0);

    // The pool account itself owns the custody vault and is the LP mint's
    // authority; there is no separate signing PDA. A token account keeps its
    // owner at bytes 32..64, and a mint keeps its authority at bytes 4..36
    // behind a four-byte `COption` tag.
    let vault = market.svm.get_account(&market.custody_vault).unwrap();
    let vault_owner = Pubkey::new_from_array(vault.data[32..64].try_into().unwrap());
    assert_eq!(vault_owner, market.pool);
    let lp_mint = market.svm.get_account(&market.lp_mint).unwrap();
    let mint_authority = Pubkey::new_from_array(lp_mint.data[4..36].try_into().unwrap());
    assert_eq!(mint_authority, market.pool);
}

#[test]
fn test_add_liquidity_first_deposit_withholds_minimum() {
    let mut market = Market::default_market();
    let deposit = 10_000 * ONE_USDC;
    let (provider, provider_collateral) = market.funded_trader(deposit);

    market
        .add_liquidity(&provider, provider_collateral, deposit, 0)
        .unwrap();

    // The pool holds the full deposit; the provider's shares are the deposit
    // minus the withheld minimum.
    assert_eq!(market.pool_state().liquidity, deposit);
    let provider_lp = derive_ata(&provider.pubkey(), &market.lp_mint);
    let shares = get_token_account_balance(&market.svm, &provider_lp).unwrap();
    assert_eq!(shares, deposit - 1_000);
}

/// The first deposit must exceed the 1_000 withheld minimum: one base unit
/// short is refused, and one over mints a single share.
#[test]
fn test_first_deposit_below_minimum_fails() {
    let mut market = Market::default_market();
    let (provider, provider_collateral) = market.funded_trader(10_000);
    assert_fails_with(
        market.add_liquidity(&provider, provider_collateral, 999, 0),
        PerpError::DepositTooSmall,
    );
    market
        .add_liquidity(&provider, provider_collateral, 1_001, 0)
        .unwrap();
    let provider_lp = derive_ata(&provider.pubkey(), &market.lp_mint);
    assert_eq!(
        get_token_account_balance(&market.svm, &provider_lp).unwrap(),
        1
    );
}

#[test]
fn test_add_liquidity_subsequent_is_proportional() {
    let mut market = Market::default_market();
    let first = 10_000 * ONE_USDC;
    market.seed_liquidity(first);

    // With no open positions and an unchanged price, assets-under-management
    // equals liquidity, so a second equal deposit mints ~the same shares.
    let second = 10_000 * ONE_USDC;
    let (provider, provider_collateral) = market.funded_trader(second);
    market
        .add_liquidity(&provider, provider_collateral, second, 0)
        .unwrap();

    let provider_lp = derive_ata(&provider.pubkey(), &market.lp_mint);
    let shares = get_token_account_balance(&market.svm, &provider_lp).unwrap();
    // supply before second deposit was `first - 1_000`, and the withheld
    // 1_000 counts as shares too, so second shares =
    // second * (supply + 1_000) / aum = second * first / first = second.
    assert_eq!(shares, second);
}

#[test]
fn test_add_and_remove_liquidity_round_trip() {
    let mut market = Market::default_market();
    let deposit = 10_000 * ONE_USDC;
    let (provider, provider_collateral) = market.funded_trader(deposit);
    market
        .add_liquidity(&provider, provider_collateral, deposit, 0)
        .unwrap();

    let provider_lp = derive_ata(&provider.pubkey(), &market.lp_mint);
    let shares = get_token_account_balance(&market.svm, &provider_lp).unwrap();
    market
        .remove_liquidity(&provider, provider_collateral, shares, 0)
        .unwrap();

    // Even as the only liquidity provider, they reclaim their deposit less
    // the withheld minimum: those 1_000 shares belong to nobody, and their
    // 1_000 of liquidity stays in the pool.
    let returned = get_token_account_balance(&market.svm, &provider_collateral).unwrap();
    assert_eq!(returned, deposit - 1_000);
    assert_eq!(market.pool_state().liquidity, 1_000);

    // The next provider is priced against the minimum's slice rather than
    // bootstrapped: 5_000 * (0 + 1_000) / 1_000 = 5_000 shares, the same
    // one share per unit the pool has always charged.
    let (next, next_collateral) = market.funded_trader(5_000);
    market
        .add_liquidity(&next, next_collateral, 5_000, 0)
        .unwrap();
    let next_lp = derive_ata(&next.pubkey(), &market.lp_mint);
    assert_eq!(
        get_token_account_balance(&market.svm, &next_lp).unwrap(),
        5_000
    );
}

/// First-depositor share inflation without a donation. The vault balance is
/// not what shares are priced against, so tokens sent straight to the vault
/// move nothing, but `liquidity` itself grows with every funding payment and
/// every trader loss, and a liquidity provider can also be the pool's only
/// trader. An attacker opens the pool with the smallest deposit that clears
/// the minimum (1 share), then pays funding on a small long of their own until
/// `liquidity` is large, which makes each share expensive in the same way a
/// donation would. The funding rate is a pool parameter the pool's creator
/// sets, and nothing stops the attacker from being that creator.
///
/// The withheld `MINIMUM_LIQUIDITY` counts as shares in both directions, so
/// the attacker's single share is 1 of 1_001 and the value pumped into the
/// pool is spread across shares nobody can redeem. A later depositor is minted
/// their fair share, and the attacker gets back a small fraction of the
/// funding they paid in.
#[test]
fn test_inflating_liquidity_through_own_trades_does_not_pay() {
    // The steepest rate a pool may have, held for ten years.
    let mut market = Market::new(dollars(100), MAX_FUNDING_RATE_PER_SECOND);

    let (attacker, attacker_collateral) = market.funded_trader(10_000 * ONE_USDC);
    market
        .add_liquidity(&attacker, attacker_collateral, 1_001, 0)
        .unwrap();
    let attacker_lp = derive_ata(&attacker.pubkey(), &market.lp_mint);
    assert_eq!(
        get_token_account_balance(&market.svm, &attacker_lp).unwrap(),
        1
    );

    // A 1_000 long. Heavy collateral keeps the position far from liquidation
    // while funding drains it into `liquidity`.
    market
        .open_position(
            &attacker,
            attacker_collateral,
            Side::Long,
            2_000 * ONE_USDC,
            1_000,
            0,
        )
        .unwrap();
    market.pass_seconds(TEN_YEARS);
    market.set_price(dollars(100));
    market
        .close_position(&attacker, attacker_collateral, Side::Long, 0)
        .unwrap();
    let pumped_liquidity = market.pool_state().liquidity;
    assert!(pumped_liquidity > 50 * 1_001);
    let attacker_spent =
        10_000 * ONE_USDC - get_token_account_balance(&market.svm, &attacker_collateral).unwrap();

    // The victim deposits just under twice the pumped liquidity. Dividing by
    // the bare share supply of 1 would mint them a single share, and the
    // attacker's one share would then redeem half the pool.
    let victim_deposit = 2 * pumped_liquidity - 1;
    let (victim, victim_collateral) = market.funded_trader(victim_deposit);
    market
        .add_liquidity(&victim, victim_collateral, victim_deposit, 0)
        .unwrap();
    let victim_lp = derive_ata(&victim.pubkey(), &market.lp_mint);
    let victim_shares = get_token_account_balance(&market.svm, &victim_lp).unwrap();

    // The attacker exits with their one share.
    let before_exit = get_token_account_balance(&market.svm, &attacker_collateral).unwrap();
    market
        .remove_liquidity(&attacker, attacker_collateral, 1, 0)
        .unwrap();
    let attacker_back =
        get_token_account_balance(&market.svm, &attacker_collateral).unwrap() - before_exit;
    assert!(
        attacker_back * 100 < attacker_spent,
        "attacker spent {attacker_spent} and got back {attacker_back}"
    );

    // The victim exits with everything and gets back all but a sliver.
    market
        .remove_liquidity(&victim, victim_collateral, victim_shares, 0)
        .unwrap();
    let victim_back = get_token_account_balance(&market.svm, &victim_collateral).unwrap();
    assert!(
        victim_back * 1_000 >= victim_deposit * 999,
        "victim deposited {victim_deposit} and got back {victim_back}"
    );
}

#[test]
fn test_open_long_updates_pool() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);

    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    let pool = market.pool_state();
    assert_eq!(pool.long_size, size as u128);
    assert_eq!(pool.short_size, 0);
    // Collateral minus the 0.1% open fee is now tracked as trader collateral,
    // and the fee is split evenly between the insurance fund and the program.
    let open_fee = size / 1_000;
    assert_eq!(pool.total_collateral, collateral - open_fee);
    assert_eq!(pool.insurance_fund, open_fee / 2);
    assert_eq!(pool.program_fees, open_fee / 2);
    // Nothing is set aside from liquidity for the position.
    assert_eq!(pool.liquidity, 100_000 * ONE_USDC);
    let position = market.position_state(&trader.pubkey(), Side::Long);
    assert_eq!(position.entry_slot, market.current_slot());
    market.assert_vault_matches_ledger();
}

#[test]
fn test_close_long_in_profit() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);

    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    // Price rises 20%: a $5,000 long earns $1,000, paid once the warm-up has
    // passed.
    market.pass_warmup();
    market.set_price(dollars(120));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let profit = size / 5; // 20% of notional
    let expected_payout = net_collateral + profit - close_fee;
    let balance = get_token_account_balance(&market.svm, &trader_collateral).unwrap();
    assert_eq!(balance, expected_payout);
}

#[test]
fn test_close_long_in_loss() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);

    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    let liquidity_before = market.pool_state().liquidity;

    // Price falls 10%: a $5,000 long loses $500.
    market.set_price(dollars(90));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let loss = size / 10; // 10% of notional
    let expected_payout = net_collateral - loss - close_fee;
    let balance = get_token_account_balance(&market.svm, &trader_collateral).unwrap();
    assert_eq!(balance, expected_payout);

    // The trader's loss accrued to the liquidity providers.
    assert_eq!(market.pool_state().liquidity, liquidity_before + loss);
}

#[test]
fn test_close_short_in_profit() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);

    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Short, collateral, size, 0)
        .unwrap();

    // Price falls 10%: a $5,000 short earns $500.
    market.pass_warmup();
    market.set_price(dollars(90));
    market
        .close_position(&trader, trader_collateral, Side::Short, 0)
        .unwrap();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let profit = size / 10;
    let expected_payout = net_collateral + profit - close_fee;
    let balance = get_token_account_balance(&market.svm, &trader_collateral).unwrap();
    assert_eq!(balance, expected_payout);
}

#[test]
fn test_open_rejects_zero_amounts() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let (trader, trader_collateral) = market.funded_trader(1_000 * ONE_USDC);

    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            0,
            5_000 * ONE_USDC,
            0,
        ),
        PerpError::ZeroAmount,
    );
    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            1_000 * ONE_USDC,
            0,
            0,
        ),
        PerpError::ZeroAmount,
    );
}

#[test]
fn test_open_rejects_position_below_initial_margin() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let (trader, trader_collateral) = market.funded_trader(2_000 * ONE_USDC);

    // The initial margin is 10% of notional. 1,000 USDC of collateral less
    // the 11 USDC open fee leaves 989 USDC, short of the 1,100 USDC an 11,000
    // USDC position needs.
    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            1_000 * ONE_USDC,
            11_000 * ONE_USDC,
            0,
        ),
        PerpError::InitialMarginNotMet,
    );

    // A 10,000 USDC position needs 1,000 USDC net of its 10 USDC open fee.
    // One minor unit short of 1,010 USDC is refused, and exactly 1,010 USDC
    // opens at 10x.
    let size = 10_000 * ONE_USDC;
    let exact_collateral = 1_010 * ONE_USDC;
    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            exact_collateral - 1,
            size,
            0,
        ),
        PerpError::InitialMarginNotMet,
    );
    market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            exact_collateral,
            size,
            0,
        )
        .unwrap();
    assert_eq!(market.pool_state().total_collateral, size / 10);
}

#[test]
fn test_open_long_slippage_guard() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    // Current price is $100 (10^10 in scale 8). A long willing to pay at most
    // $99 must be rejected.
    let acceptable_price = (dollars(99)) as u64;
    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            acceptable_price,
        ),
        PerpError::SlippageExceeded,
    );
}

#[test]
fn test_stale_price_rejected() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    // Move far past the staleness window without refreshing the feed. Warp
    // relative to the current slot: LiteSVM starts the clock at a mainnet-like
    // slot, not at zero, so an absolute target could move time backwards.
    let opened_at = market.current_slot();
    market.warp(opened_at + 10_000);
    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            0,
        ),
        PerpError::StalePrice,
    );
}

/// A cluster restart passes hours of wall-clock time in zero slots, so a
/// price published before the halt can still look fresh by slot count. With
/// leverage a stale price is amplified into a market-wide equity error, so
/// the pool must refuse it until the publisher posts again.
#[test]
fn test_open_rejects_price_from_before_a_restart() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    // Simulate a halt: the cluster restarts a few slots after the price was
    // published, well inside the staleness window, so only the restart check
    // can catch the pre-halt price.
    market.set_price(dollars(100));
    let published_at = market.current_slot();
    market.warp(published_at + 5);
    market.set_last_restart_slot(published_at + 3);

    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            u64::MAX,
        ),
        PerpError::PricePredatesRestart,
    );

    // Publishing after the restart reopens the pool. Warp first: the retry is
    // otherwise byte-identical to the rejected open, so it would carry the same
    // signature and be dropped as already processed.
    market.warp(published_at + 6);
    market.set_price(dollars(100));
    market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            u64::MAX,
        )
        .expect("a freshly published price must be accepted after a restart");
}

/// The pool records the program that owns its feed at creation and refuses a
/// price from a feed account owned by any other program, however well its
/// bytes decode. The feed is swapped for a byte-identical copy owned by an
/// unrelated program, and the refusal is by the owner alone: the same bytes
/// owned by the mock oracle program again are accepted.
#[test]
fn test_open_rejects_price_feed_from_another_program() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    market.set_feed_owner(Pubkey::new_unique());
    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            0,
        ),
        PerpError::PriceFeedNotFromOracle,
    );

    // The retry is otherwise byte-identical to the rejected open, so it would
    // carry the same signature and be dropped as already processed.
    market.svm.expire_blockhash();
    market.set_feed_owner(mock_price_feed::id());
    market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            0,
        )
        .expect("the same feed owned by the recorded oracle program must be accepted");
}

#[test]
fn test_wide_oracle_confidence_rejected() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    // The pool tolerates a 1% confidence band (max_confidence_bps = 100). Widen
    // the feed's band to 2% of the price and the open must be rejected.
    market.set_price_with_confidence(dollars(100), dollars(2) as u64);
    assert_fails_with(
        market.open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            0,
        ),
        PerpError::OracleConfidenceTooWide,
    );
}

#[test]
fn test_funding_charged_to_long() {
    // Funding on: longs are the only side, so they pay funding to the pool.
    let mut market = Market::new(dollars(100), MAX_FUNDING_RATE_PER_SECOND);
    market.seed_liquidity(100_000 * ONE_USDC);

    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    let liquidity_before = market.pool_state().liquidity;

    // Let funding accrue, then refresh the feed so the price is fresh again and
    // close at the same price (no profit/loss).
    let seconds_open = 2_000;
    market.pass_seconds(seconds_open);
    market.set_price(dollars(100));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let payout = get_token_account_balance(&market.svm, &trader_collateral).unwrap();

    // The trader's shortfall against collateral-minus-fees is exactly the
    // funding for the seconds the position was open, and it went to the
    // liquidity providers.
    let funding_paid = (net_collateral - close_fee) - payout;
    let expected = size * MAX_FUNDING_RATE_PER_SECOND * seconds_open as u64 / FUNDING_PRECISION;
    assert_eq!(funding_paid, expected);
    assert_eq!(
        market.pool_state().liquidity,
        liquidity_before + funding_paid
    );
}

/// Funding held open for a window, closed at an unchanged price: returns the
/// funding the trader paid. `between` runs halfway through.
fn funding_paid_over(rate: u64, window: i64, between: impl Fn(&mut Market)) -> u64 {
    let mut market = Market::new(dollars(100), rate);
    market.seed_liquidity(100_000 * ONE_USDC);

    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    market.pass_seconds(window);
    between(&mut market);
    market.pass_seconds(window);
    market.set_price(dollars(100));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let fee = size / 1_000;
    let payout = get_token_account_balance(&market.svm, &trader_collateral).unwrap();
    (collateral - fee - fee) - payout
}

/// Funding is quoted per second of wall-clock time, so slots passing without
/// the clock moving charge nothing. A million extra slots halfway through the
/// window, as a much shorter slot would produce, leave the funding unchanged.
#[test]
fn test_funding_follows_seconds_not_slots() {
    let rate = MAX_FUNDING_RATE_PER_SECOND;
    let window = 2_000;
    let flat = funding_paid_over(rate, window, |_| {});
    let with_extra_slots = funding_paid_over(rate, window, |market| {
        let slot = market.current_slot();
        market.warp(slot + 1_000_000);
    });
    assert!(flat > 0);
    assert_eq!(with_extra_slots, flat);
}

#[test]
fn test_initialize_pool_rejects_funding_rate_above_the_maximum() {
    // The rate is fixed at creation, so this is the only place it is checked.
    assert_fails_with(
        Market::try_new(
            dollars(100),
            default_parameters(MAX_FUNDING_RATE_PER_SECOND + 1),
        ),
        PerpError::InvalidParameter,
    );
    assert!(Market::try_new(
        dollars(100),
        default_parameters(MAX_FUNDING_RATE_PER_SECOND)
    )
    .is_ok());
}

/// The pool operator trading against their own pool. The lighter side of open
/// interest is paid funding out of `liquidity`, so an operator who could raise
/// the rate at will could open a small position on the lighter side, raise the
/// rate, and close it to take the liquidity providers' deposits. The rate is
/// fixed when the pool is created and capped, so a wallet the operator
/// controls earns exactly what any trader on that side would: at most the
/// maximum rate, here just under 0.1% of the position's size over an hour.
#[test]
fn test_operator_on_the_lighter_side_earns_only_the_fixed_rate() {
    let mut market = Market::new(dollars(100), MAX_FUNDING_RATE_PER_SECOND);
    market.seed_liquidity(100_000 * ONE_USDC);

    // Longs are the heavier side, so they pay and shorts are paid.
    let (trader, trader_collateral) = market.funded_trader(2_000 * ONE_USDC);
    market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            2_000 * ONE_USDC,
            10_000 * ONE_USDC,
            0,
        )
        .unwrap();

    let collateral = 200 * ONE_USDC;
    let size = 1_000 * ONE_USDC;
    let (operator_wallet, operator_collateral) = market.funded_trader(collateral);
    market
        .open_position(
            &operator_wallet,
            operator_collateral,
            Side::Short,
            collateral,
            size,
            0,
        )
        .unwrap();
    let liquidity_before = market.pool_state().liquidity;

    let one_hour = 3_600;
    market.pass_seconds(one_hour);
    market.set_price(dollars(100));
    market
        .close_position(&operator_wallet, operator_collateral, Side::Short, 0)
        .unwrap();

    let fees = 2 * (size / 1_000); // open and close, 0.1% of notional each
    let payout = get_token_account_balance(&market.svm, &operator_collateral).unwrap();
    let funding_received = payout - (collateral - fees);
    let expected = size * MAX_FUNDING_RATE_PER_SECOND * one_hour as u64 / FUNDING_PRECISION;
    assert_eq!(funding_received, expected);
    assert!(
        funding_received * 1_000 < size,
        "under 0.1% of size in an hour"
    );
    assert_eq!(
        market.pool_state().liquidity,
        liquidity_before - funding_received
    );
}

#[test]
fn test_liquidation_of_underwater_long() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);

    // High leverage: a ~9x long, so a small adverse move erodes the margin.
    // Collateral leaves room above the notional after the open fee (10,000 of
    // notional needs at least 1,000 of net collateral at 10x).
    let collateral = 1_100 * ONE_USDC;
    let size = 10_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    // Price falls 9%: a $10,000 long loses $900, dropping equity below the 5%
    // maintenance margin.
    market.set_price(dollars(91));

    let liquidator = create_wallet(&mut market.svm, 100_000_000_000).unwrap();
    let liquidator_collateral = create_associated_token_account(
        &mut market.svm,
        &liquidator.pubkey(),
        &market.collateral_mint,
        &market.payer,
    )
    .unwrap();

    market
        .liquidate(&liquidator, &trader.pubkey(), trader_collateral, Side::Long)
        .unwrap();

    // The liquidator earned a fee and the position is gone.
    let reward = get_token_account_balance(&market.svm, &liquidator_collateral).unwrap();
    assert!(reward > 0);
    assert!(market
        .svm
        .get_account(&market.position_pda(&trader.pubkey(), Side::Long))
        .is_none());
    assert_eq!(market.pool_state().long_size, 0);
}

#[test]
fn test_healthy_position_cannot_be_liquidated() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);

    let collateral = 1_000 * ONE_USDC;
    let size = 2_000 * ONE_USDC; // 2x leverage, plenty of margin
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    let liquidator = create_wallet(&mut market.svm, 100_000_000_000).unwrap();
    create_associated_token_account(
        &mut market.svm,
        &liquidator.pubkey(),
        &market.collateral_mint,
        &market.payer,
    )
    .unwrap();

    // Price barely moves; the position stays healthy.
    market.set_price(dollars(99));
    assert_fails_with(
        market.liquidate(&liquidator, &trader.pubkey(), trader_collateral, Side::Long),
        PerpError::PositionHealthy,
    );
}

#[test]
fn test_collect_fees() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);

    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    let fees = market.pool_state().program_fees;
    assert!(fees > 0);

    let admin = market.admin.insecure_clone();
    let admin_collateral = create_associated_token_account(
        &mut market.svm,
        &admin.pubkey(),
        &market.collateral_mint,
        &market.payer,
    )
    .unwrap();
    market.collect_fees(&admin).unwrap();

    assert_eq!(
        get_token_account_balance(&market.svm, &admin_collateral).unwrap(),
        fees
    );
    assert_eq!(market.pool_state().program_fees, 0);

    // Nothing left to claim on a second sweep (a fresh blockhash, or the
    // identical transaction is rejected as already processed).
    market.svm.expire_blockhash();
    assert_fails_with(market.collect_fees(&admin), PerpError::NothingToClaim);
}

#[test]
fn test_collect_fees_requires_authority() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            0,
        )
        .unwrap();

    let imposter = create_wallet(&mut market.svm, 100_000_000_000).unwrap();
    create_associated_token_account(
        &mut market.svm,
        &imposter.pubkey(),
        &market.collateral_mint,
        &market.payer,
    )
    .unwrap();
    // The pool's `has_one = authority` constraint refuses the imposter, so
    // the refusal is Anchor's, not the program's.
    assert_fails_with_anchor_error(
        market.collect_fees(&imposter),
        AnchorErrorCode::ConstraintHasOne,
    );
}

/// Nothing is set aside to back a position's profit, so a position can open
/// against a pool that could not pay its full winnings: here a $10,000 long
/// against $6,000 of liquidity.
#[test]
fn test_open_allowed_without_full_backing() {
    let mut market = Market::default_market();
    market.seed_liquidity(6_000 * ONE_USDC);
    let collateral = 1_100 * ONE_USDC;
    let size = 10_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    let pool = market.pool_state();
    assert_eq!(pool.long_size, size as u128);
    assert_eq!(pool.liquidity, 6_000 * ONE_USDC);
    market.assert_vault_matches_ledger();
}

#[test]
fn test_profit_runs_uncapped_when_backed() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 2_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    // Price triples, so the long's profit is twice its size. A move this
    // large is far outside the price band, so the average has to catch up
    // before the position can close, which also passes the warm-up. The
    // $100,000 pool backs the whole $10,000 profit, so it is paid in full.
    market.set_price(dollars(300));
    market.settle_average_at(dollars(300));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let profit = 2 * size;
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        net_collateral + profit - close_fee
    );
    assert_eq!(market.pool_state().liquidity, 100_000 * ONE_USDC - profit);
    market.assert_vault_matches_ledger();
}

/// Two longs are owed $1,800 of profit between them, and the pool holds only
/// $900 to pay it with, so each is paid half of their profit: the first to
/// close is paid half of theirs, and the second, closing against what is left,
/// is paid half of theirs too.
#[test]
fn test_haircut_scales_profit_when_pool_stressed() {
    // No fee goes to the insurance fund here, so the only backing is the $900
    // of liquidity and the first close adds nothing to it.
    let mut market = Market::try_new(
        dollars(100),
        PoolParameters {
            insurance_fee_bps: 0,
            ..default_parameters(0)
        },
    )
    .unwrap();
    market.seed_liquidity(900 * ONE_USDC);

    let first_collateral = 1_000 * ONE_USDC;
    let first_size = 6_000 * ONE_USDC;
    let (first, first_account) = market.funded_trader(first_collateral);
    market
        .open_position(
            &first,
            first_account,
            Side::Long,
            first_collateral,
            first_size,
            0,
        )
        .unwrap();
    let second_collateral = 800 * ONE_USDC;
    let second_size = 4_000 * ONE_USDC;
    let (second, second_account) = market.funded_trader(second_collateral);
    market
        .open_position(
            &second,
            second_account,
            Side::Long,
            second_collateral,
            second_size,
            0,
        )
        .unwrap();

    // At $118 the first long is up $1,080 and the second $720: $1,800 owed
    // against $900 of backing, so h = 900 / 1,800 = 0.5.
    market.pass_warmup();
    market.set_price(dollars(118));
    let half = HAIRCUT_PRECISION / 2;
    let first_profit = first_size * 18 / 100;
    let second_profit = second_size * 18 / 100;

    market
        .close_position(&first, first_account, Side::Long, 0)
        .unwrap();
    let first_paid = first_profit * half / HAIRCUT_PRECISION;
    assert_eq!(first_paid, 540 * ONE_USDC);
    assert_eq!(
        get_token_account_balance(&market.svm, &first_account).unwrap(),
        first_collateral - first_size / 1_000 + first_paid - first_size / 1_000
    );
    // The $540 withheld from the first long stays with the providers.
    assert_eq!(market.pool_state().liquidity, 360 * ONE_USDC);

    // The second long is now owed $720 against $360: h is still 0.5.
    market
        .close_position(&second, second_account, Side::Long, 0)
        .unwrap();
    let second_paid = second_profit * half / HAIRCUT_PRECISION;
    assert_eq!(second_paid, 360 * ONE_USDC);
    assert_eq!(
        get_token_account_balance(&market.svm, &second_account).unwrap(),
        second_collateral - second_size / 1_000 + second_paid - second_size / 1_000
    );
    assert_eq!(market.pool_state().liquidity, 0);
    market.assert_vault_matches_ledger();
}

/// Alice's long is up $1,000 while Bob's short, still open and healthy, is
/// down $900, so traders are owed only $100 in aggregate, and the pool's
/// backing is $300. Sized against the $100 alone the haircut would be one and
/// Alice's $1,000 would exceed the backing; it is sized against her $1,000
/// instead, so she is paid exactly the $300 and the close goes through. Bob's
/// later close settles his loss into the pool in full.
#[test]
fn test_winner_offset_by_open_loser_is_paid_not_refused() {
    let mut market = Market::default_market();
    // $290.50 of liquidity plus the $9.50 the two open fees put in the
    // insurance fund is $300 of backing.
    market.seed_liquidity(290_500_000);

    let alice_collateral = 1_100 * ONE_USDC;
    let alice_size = 10_000 * ONE_USDC;
    let (alice, alice_account) = market.funded_trader(alice_collateral);
    market
        .open_position(
            &alice,
            alice_account,
            Side::Long,
            alice_collateral,
            alice_size,
            0,
        )
        .unwrap();
    let bob_collateral = 2_000 * ONE_USDC;
    let bob_size = 9_000 * ONE_USDC;
    let (bob, bob_account) = market.funded_trader(bob_collateral);
    market
        .open_position(&bob, bob_account, Side::Short, bob_collateral, bob_size, 0)
        .unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.liquidity + pool.insurance_fund, 300 * ONE_USDC);

    // At $110 Alice is up $1,000 and Bob down $900: h = 300 / 1,000 = 0.3.
    market.pass_warmup();
    market.set_price(dollars(110));
    market
        .close_position(&alice, alice_account, Side::Long, 0)
        .unwrap();
    let alice_paid = 1_000 * ONE_USDC * (3 * HAIRCUT_PRECISION / 10) / HAIRCUT_PRECISION;
    assert_eq!(alice_paid, 300 * ONE_USDC);
    let alice_fee = alice_size / 1_000;
    assert_eq!(
        get_token_account_balance(&market.svm, &alice_account).unwrap(),
        alice_collateral - alice_fee + alice_paid - alice_fee
    );
    // The whole backing was paid out; the fund then took half of Alice's
    // close fee.
    let pool = market.pool_state();
    assert_eq!(pool.liquidity, 0);
    assert_eq!(pool.insurance_fund, alice_fee / 2);
    market.assert_vault_matches_ledger();

    // Bob closes at the same price, losing $900 into the pool.
    market
        .close_position(&bob, bob_account, Side::Short, 0)
        .unwrap();
    let bob_fee = bob_size / 1_000;
    let bob_loss = 900 * ONE_USDC;
    assert_eq!(
        get_token_account_balance(&market.svm, &bob_account).unwrap(),
        bob_collateral - bob_fee - bob_loss - bob_fee
    );
    let pool = market.pool_state();
    assert_eq!(pool.liquidity, bob_loss);
    assert_eq!(pool.insurance_fund, (alice_fee + bob_fee) / 2);
    assert_eq!(pool.total_collateral, 0);
    market.assert_vault_matches_ledger();
}

/// The haircut counts the insurance fund as backing, so a profit larger than
/// `liquidity` but within `liquidity + insurance_fund` is paid in full: the
/// pool's liquidity first, the insurance fund for the rest.
#[test]
fn test_insurance_pays_profit_beyond_liquidity() {
    // A 5% open fee, half of which goes to the insurance fund.
    let mut market = Market::try_new(
        dollars(100),
        PoolParameters {
            open_fee_bps: 500,
            ..default_parameters(0)
        },
    )
    .unwrap();
    market.seed_liquidity(1_700 * ONE_USDC);

    // $500 open fee: $250 to the insurance fund, $1,100 of net collateral.
    let collateral = 1_600 * ONE_USDC;
    let size = 10_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    assert_eq!(market.pool_state().insurance_fund, 250 * ONE_USDC);

    // At $118 the long is up $1,800: more than the $1,700 of liquidity, within
    // the $1,950 of liquidity plus insurance, so h = 1.
    market.pass_warmup();
    market.set_price(dollars(118));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let profit = 1_800 * ONE_USDC;
    let close_fee = size / 1_000;
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        1_100 * ONE_USDC + profit - close_fee
    );
    let pool = market.pool_state();
    assert_eq!(pool.liquidity, 0);
    // $100 of the profit came from the insurance fund, which then took half
    // of the $10 close fee.
    assert_eq!(pool.insurance_fund, 150 * ONE_USDC + close_fee / 2);
    market.assert_vault_matches_ledger();
}

/// Shares are priced against assets-under-management, which counts a
/// trader's unrealized loss as the providers' gain, but that loss is still in
/// the trader's collateral. A withdrawal is capped at `liquidity`.
#[test]
fn test_remove_liquidity_capped_at_liquidity() {
    let mut market = Market::default_market();
    let (provider, provider_collateral) = market.seed_liquidity(10_000 * ONE_USDC);
    let (trader, trader_collateral) = market.funded_trader(1_000 * ONE_USDC);
    market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            1_000 * ONE_USDC,
            5_000 * ONE_USDC,
            0,
        )
        .unwrap();

    // At $80 the long is down $1,000, so assets-under-management is $11,000
    // against $10,000 of liquidity, and each share redeems 1.1 minor units
    // (the provider's shares plus the withheld minimum are 10,000 USDC of
    // shares). 9,090,909,092 shares would redeem 10,000,000,001, one minor
    // unit more than `liquidity`, and are refused.
    market.set_price(dollars(80));
    assert_fails_with(
        market.remove_liquidity(&provider, provider_collateral, 9_090_909_092, 0),
        PerpError::InsufficientLiquidity,
    );

    // One share fewer redeems exactly the pool's liquidity.
    market
        .remove_liquidity(&provider, provider_collateral, 9_090_909_091, 0)
        .unwrap();
    assert_eq!(
        get_token_account_balance(&market.svm, &provider_collateral).unwrap(),
        10_000 * ONE_USDC
    );
    assert_eq!(market.pool_state().liquidity, 0);
    market.assert_vault_matches_ledger();
}

/// One slot short of the warm-up, a profitable close is refused and the
/// position stays open.
#[test]
fn test_profit_blocked_before_maturation() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    let entry_slot = market
        .position_state(&trader.pubkey(), Side::Long)
        .entry_slot;

    market.warp(entry_slot + PROFIT_WARMUP_SLOTS - 1);
    market.set_price(dollars(110));
    assert_fails_with(
        market.close_position(&trader, trader_collateral, Side::Long, 0),
        PerpError::ProfitNotMatured,
    );
    assert_eq!(market.pool_state().long_size, size as u128);
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        0
    );
}

/// From exactly `entry_slot + profit_warmup_slots`, the profit is paid.
#[test]
fn test_profit_realized_after_maturation() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    let entry_slot = market
        .position_state(&trader.pubkey(), Side::Long)
        .entry_slot;

    market.warp(entry_slot + PROFIT_WARMUP_SLOTS);
    market.set_price(dollars(110));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let fee = size / 1_000;
    let profit = size / 10;
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        collateral - fee + profit - fee
    );
}

/// The warm-up holds back profit only: a losing position closes in the slot
/// it opened.
#[test]
fn test_loss_not_gated_by_maturation() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    let entry_slot = market
        .position_state(&trader.pubkey(), Side::Long)
        .entry_slot;

    // Price falls 10% within the same slot: a $500 loss.
    market.set_price(dollars(90));
    assert_eq!(market.current_slot(), entry_slot);
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let fee = size / 1_000;
    let loss = size / 10;
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        collateral - fee - loss - fee
    );
    assert_eq!(market.pool_state().liquidity, 100_000 * ONE_USDC + loss);
}

/// `insurance_fee_bps` of each open and close fee goes to the insurance fund,
/// rounded down, and the program keeps the rest, so no minor unit is lost.
#[test]
fn test_insurance_fund_funded_by_fees() {
    let mut market = Market::try_new(
        dollars(100),
        PoolParameters {
            insurance_fee_bps: 3_333,
            ..default_parameters(0)
        },
    )
    .unwrap();
    market.seed_liquidity(100_000 * ONE_USDC);

    // A size whose 0.1% fee is 1,234,567.89 minor units, rounded up to
    // 1,234,568: 3,333 basis points of that is 411,481.5, so the insurance
    // fund gets 411,481 (its cut rounds down) and the program the other 823,087.
    let size: u64 = 1_234_567_890;
    let fee = 1_234_568;
    let insurance_cut = 411_481;
    assert_eq!(size.div_ceil(1_000), fee);
    let collateral = 200 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.insurance_fund, insurance_cut);
    assert_eq!(pool.program_fees, fee - insurance_cut);

    // Closing at the open price charges the same fee again.
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.insurance_fund, 2 * insurance_cut);
    assert_eq!(pool.program_fees, 2 * (fee - insurance_cut));
    market.assert_vault_matches_ledger();
}

/// A $1,000 long with $110 of net collateral, liquidated after a 15% fall:
/// its $150 loss leaves equity at -$40.
fn open_long_and_gap_through_zero(market: &mut Market) -> (Keypair, Pubkey) {
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 160 * ONE_USDC;
    let size = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    market.set_price(dollars(85));
    (trader, trader_collateral)
}

/// A bankrupt position's deficit, its loss beyond its collateral, is paid by
/// the insurance fund when the fund holds enough.
#[test]
fn test_insurance_absorbs_bankruptcy_deficit() {
    // A 5% open fee, 90% of which goes to the insurance fund: $45 of the $50.
    let mut market = Market::try_new(
        dollars(100),
        PoolParameters {
            open_fee_bps: 500,
            insurance_fee_bps: 9_000,
            ..default_parameters(0)
        },
    )
    .unwrap();
    let (trader, trader_collateral) = open_long_and_gap_through_zero(&mut market);
    assert_eq!(market.pool_state().insurance_fund, 45 * ONE_USDC);
    let liquidity_before = market.pool_state().liquidity;

    let (liquidator, liquidator_collateral) = market.liquidator();
    market
        .liquidate(&liquidator, &trader.pubkey(), trader_collateral, Side::Long)
        .unwrap();

    // The fund pays the $40 deficit, so the providers keep the $110 of
    // collateral and are credited the full $150 loss.
    let pool = market.pool_state();
    assert_eq!(pool.insurance_fund, 5 * ONE_USDC);
    assert_eq!(pool.liquidity, liquidity_before + 150 * ONE_USDC);
    assert_eq!(
        get_token_account_balance(&market.svm, &liquidator_collateral).unwrap(),
        0
    );
    market.assert_vault_matches_ledger();
}

/// A position already below zero equity can still be liquidated by anyone.
/// Its equity cannot pay the liquidation fee, so the fee is forgiven and the
/// liquidator receives nothing. The insurance fund pays as much of the deficit
/// as it holds, and the liquidity providers bear only the rest.
#[test]
fn test_liquidation_of_bankrupt_position_charges_insurance_before_liquidity() {
    // A 5% open fee, half of which goes to the insurance fund: $25 of the $50.
    let mut market = Market::try_new(
        dollars(100),
        PoolParameters {
            open_fee_bps: 500,
            ..default_parameters(0)
        },
    )
    .unwrap();
    let (trader, trader_collateral) = open_long_and_gap_through_zero(&mut market);
    assert_eq!(market.pool_state().insurance_fund, 25 * ONE_USDC);
    let liquidity_before = market.pool_state().liquidity;

    let (liquidator, liquidator_collateral) = market.liquidator();
    market
        .liquidate(&liquidator, &trader.pubkey(), trader_collateral, Side::Long)
        .unwrap();

    // The $40 deficit: $25 from the insurance fund, $15 borne by the
    // providers, who keep the $110 of collateral plus the fund's $25.
    let pool = market.pool_state();
    assert_eq!(pool.insurance_fund, 0);
    assert_eq!(pool.liquidity, liquidity_before + 135 * ONE_USDC);
    assert_eq!(pool.long_size, 0);
    assert_eq!(pool.total_collateral, 0);
    assert_eq!(
        get_token_account_balance(&market.svm, &liquidator_collateral).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        0
    );
    assert!(market
        .svm
        .get_account(&market.position_pda(&trader.pubkey(), Side::Long))
        .is_none());
    market.assert_vault_matches_ledger();
}

/// Every fee rounds up, and so does the maintenance requirement, so none is
/// a minor unit short in the trader's favour. A position one base unit over
/// $5,000 pays $5.000001 to open and the same to close: 0.1% of it is
/// 5,000,000.001 base units, rounded up to 5,000,001. The insurance fund's
/// half of a fee rounds down, and the program takes the odd unit. The same
/// position is liquidatable at an equity of 250,000,001 base units, the
/// maintenance requirement 250,000,000.05 rounded up, where a requirement
/// rounded down would have left it one base unit too healthy, and the
/// liquidation fee is 50,000,001.
#[test]
fn test_fees_and_maintenance_requirement_round_up() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC + 1;
    let fee = 5_000_001;
    let (trader, trader_collateral) = market.funded_trader(2 * collateral);

    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.insurance_fund, 2_500_000);
    assert_eq!(pool.program_fees, 2_500_001);
    assert_eq!(pool.total_collateral, collateral - fee);
    assert_eq!(
        market
            .position_state(&trader.pubkey(), Side::Long)
            .collateral,
        collateral - fee
    );

    // Closing at the entry price settles no profit or loss, so the payout is
    // the net collateral less the close fee.
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        2 * collateral - fee - fee
    );
    let pool = market.pool_state();
    assert_eq!(pool.insurance_fund, 2 * 2_500_000);
    assert_eq!(pool.program_fees, 2 * 2_500_001);

    // The same position again, taken to an equity of exactly the rounded-up
    // maintenance requirement: $85.10000004 loses it 744,999,998 base units.
    // The open is byte-identical to the first, so it would carry the same
    // signature and be dropped as already processed without a new blockhash.
    market.svm.expire_blockhash();
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    market.set_price(8_510_000_004);
    let (liquidator, liquidator_collateral) = market.liquidator();
    market
        .liquidate(&liquidator, &trader.pubkey(), trader_collateral, Side::Long)
        .unwrap();
    assert_eq!(
        get_token_account_balance(&market.svm, &liquidator_collateral).unwrap(),
        50_000_001
    );
    // The trader is refunded the equity less the liquidation fee.
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        collateral - fee - fee + (250_000_001 - 50_000_001)
    );
    market.assert_vault_matches_ledger();
}

/// `basis_points_of` rounds up, so an amount that is not an exact multiple
/// rounds to the next base unit and the smallest non-zero amount pays a whole
/// unit; an exact multiple is unchanged. `basis_points_of_rounded_down`
/// splits a fee the pool already holds, so it rounds the other way.
#[test]
fn test_basis_points_of_rounds_up_and_the_insurance_split_rounds_down() {
    assert_eq!(basis_points_of(5_000 * ONE_USDC, 10).unwrap(), 5 * ONE_USDC);
    assert_eq!(
        basis_points_of(5_000 * ONE_USDC + 1, 10).unwrap(),
        5_000_001
    );
    assert_eq!(basis_points_of(1, 10).unwrap(), 1);
    assert_eq!(basis_points_of(0, 10).unwrap(), 0);
    // Widened to `u128`, so the largest amount does not overflow.
    assert_eq!(basis_points_of(u64::MAX, 10_000).unwrap(), u64::MAX);
    assert_eq!(
        basis_points_of_rounded_down(5_000_001, 5_000).unwrap(),
        2_500_000
    );
    assert_eq!(basis_points_of_rounded_down(1, 5_000).unwrap(), 0);
}

#[test]
fn test_initialize_pool_rejects_insurance_fee_at_or_above_full_fee() {
    let with_insurance_fee = |insurance_fee_bps| PoolParameters {
        insurance_fee_bps,
        ..default_parameters(0)
    };
    assert_fails_with(
        Market::try_new(dollars(100), with_insurance_fee(10_000)),
        PerpError::InvalidParameter,
    );
    assert!(Market::try_new(dollars(100), with_insurance_fee(9_999)).is_ok());
}

#[test]
fn test_initialize_pool_rejects_close_fee_at_or_above_maintenance_margin() {
    // A pool whose close fee reached the maintenance margin could strand a
    // position that is too healthy to liquidate but too poor to pay the fee to
    // close, so initialize_pool refuses the configuration.
    let with_close_fee = |close_fee_bps| PoolParameters {
        close_fee_bps,
        ..default_parameters(0)
    };
    assert_fails_with(
        Market::try_new(dollars(100), with_close_fee(600)),
        PerpError::InvalidParameter,
    );
    assert_fails_with(
        Market::try_new(dollars(100), with_close_fee(500)),
        PerpError::InvalidParameter,
    );
    // One basis point below the maintenance margin is accepted.
    assert!(Market::try_new(dollars(100), with_close_fee(499)).is_ok());
}

#[test]
fn test_initialize_pool_rejects_initial_margin_at_or_below_maintenance() {
    // An initial margin at or below the 5% maintenance margin would let a
    // position open already liquidatable.
    let with_initial_margin = |initial_margin_bps| PoolParameters {
        initial_margin_bps,
        ..default_parameters(0)
    };
    assert_fails_with(
        Market::try_new(dollars(100), with_initial_margin(500)),
        PerpError::InitialMarginNotAboveMaintenance,
    );
    assert_fails_with(
        Market::try_new(dollars(100), with_initial_margin(350)),
        PerpError::InitialMarginNotAboveMaintenance,
    );

    // Above 100% of notional is refused too. One basis point above the
    // maintenance margin, and exactly 100%, are accepted.
    assert_fails_with(
        Market::try_new(dollars(100), with_initial_margin(10_001)),
        PerpError::InvalidParameter,
    );
    assert!(Market::try_new(dollars(100), with_initial_margin(501)).is_ok());
    assert!(Market::try_new(dollars(100), with_initial_margin(10_000)).is_ok());
}

#[test]
fn test_initialize_pool_rejects_price_deviation_outside_range() {
    let with_deviation = |max_price_deviation_bps| PoolParameters {
        max_price_deviation_bps,
        ..default_parameters(0)
    };
    for rejected in [0, 10_000] {
        assert_fails_with(
            Market::try_new(dollars(100), with_deviation(rejected)),
            PerpError::InvalidPriceDeviation,
        );
    }
    assert!(Market::try_new(dollars(100), with_deviation(1)).is_ok());
    assert!(Market::try_new(dollars(100), with_deviation(9_999)).is_ok());
}

/// A single oracle print far from the pool's average cannot be traded at: the
/// open is refused before the price is folded into the average.
#[test]
fn test_open_rejected_when_oracle_jumps_outside_band() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    // The band is 20% around the $100 average: $125 and $79 are outside it.
    for outside_price in [dollars(125), dollars(79)] {
        market.set_price(outside_price);
        // The two refused opens are otherwise byte-identical transactions.
        market.svm.expire_blockhash();
        assert_fails_with(
            market.open_position(&trader, trader_collateral, Side::Long, collateral, size, 0),
            PerpError::PriceOutsideBand,
        );
        // The refused open folded nothing into the average.
        assert_eq!(market.pool_state().average_price, dollars(100) as u64);
    }

    // $118 is inside the band, and opens at that price.
    market.set_price(dollars(118));
    market.svm.expire_blockhash();
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();
    let position_account = market
        .svm
        .get_account(&market.position_pda(&trader.pubkey(), Side::Long))
        .unwrap();
    let position = Position::try_deserialize(&mut position_account.data.as_slice()).unwrap();
    assert_eq!(position.entry_price, dollars(118) as u64);
}

#[test]
fn test_close_rejected_when_oracle_jumps_outside_band() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    // A jump to $125 would pay the long $1,250, but $125 is 25% from the
    // $100 average, outside the 20% band.
    market.set_price(dollars(125));
    assert_fails_with(
        market.close_position(&trader, trader_collateral, Side::Long, 0),
        PerpError::PriceOutsideBand,
    );

    // At $115, inside the band and after the warm-up, the close goes through
    // and pays the 15% gain.
    market.pass_warmup();
    market.set_price(dollars(115));
    market.svm.expire_blockhash();
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();
    let fee = size / 1_000;
    let profit = size * 15 / 100;
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        collateral - fee + profit - fee
    );
}

/// Liquidation has no band check: a genuine crash is when positions go
/// underwater, so the pool has to be able to liquidate through one.
#[test]
fn test_liquidation_runs_outside_band() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_100 * ONE_USDC;
    let size = 10_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    // $75 is 25% below the $100 average, so the owner cannot close there.
    market.set_price(dollars(75));
    assert_fails_with(
        market.close_position(&trader, trader_collateral, Side::Long, 0),
        PerpError::PriceOutsideBand,
    );

    let liquidator = create_wallet(&mut market.svm, 100_000_000_000).unwrap();
    market
        .liquidate(&liquidator, &trader.pubkey(), trader_collateral, Side::Long)
        .unwrap();
    assert!(market
        .svm
        .get_account(&market.position_pda(&trader.pubkey(), Side::Long))
        .is_none());
    assert_eq!(market.pool_state().long_size, 0);
}

#[test]
fn test_liquidity_changes_rejected_when_oracle_jumps_outside_band() {
    let mut market = Market::default_market();
    let (provider, provider_collateral) = market.seed_liquidity(10_000 * ONE_USDC);
    let provider_lp = derive_ata(&provider.pubkey(), &market.lp_mint);
    let shares = get_token_account_balance(&market.svm, &provider_lp).unwrap();

    // $76 is 24% below the $100 average.
    market.set_price(dollars(76));
    let (depositor, depositor_collateral) = market.funded_trader(5_000 * ONE_USDC);
    assert_fails_with(
        market.add_liquidity(&depositor, depositor_collateral, 5_000 * ONE_USDC, 0),
        PerpError::PriceOutsideBand,
    );
    assert_fails_with(
        market.remove_liquidity(&provider, provider_collateral, shares, 0),
        PerpError::PriceOutsideBand,
    );
}

/// After a genuine move outside the band, anyone can walk the average toward
/// the new price with `update_price_average`, and trading resumes once the
/// price is back inside the band.
#[test]
fn test_price_average_catches_up_after_genuine_move() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    let keeper = create_wallet(&mut market.svm, 100_000_000_000).unwrap();

    // NVDAx reprices from $100 to $130, 30% away from the average.
    let new_price = dollars(130);
    market.set_price(new_price);
    assert_fails_with(
        market.open_position(&trader, trader_collateral, Side::Long, collateral, size, 0),
        PerpError::PriceOutsideBand,
    );

    // Every two minutes the keeper calls `update_price_average`. Each call
    // credits the two minutes since the previous read to the price that read
    // saw, a fifth of the window. The first call credits $100, the price
    // before the move, and records $130; each later call moves the average a
    // fifth of the remaining gap to $130: $100, then $106, then $110.80. $130
    // is within 20% of any average from $108.34 up, so the third update
    // reopens trading.
    let mut updates = 0;
    loop {
        market.pass_seconds(120);
        market.set_price(new_price);
        market.update_price_average(&keeper).unwrap();
        updates += 1;
        let opened =
            market.open_position(&trader, trader_collateral, Side::Long, collateral, size, 0);
        if opened.is_ok() {
            break;
        }
        assert_fails_with(opened, PerpError::PriceOutsideBand);
        assert!(updates < 10, "the average never caught up");
    }
    assert_eq!(updates, 3);
    let pool = market.pool_state();
    assert_eq!(pool.average_price, 11_080_000_000);
    assert_eq!(pool.last_oracle_price, new_price as u64);
}

#[test]
fn test_single_update_moves_average_by_elapsed_fraction() {
    let mut market = Market::default_market();
    let keeper = create_wallet(&mut market.svm, 100_000_000_000).unwrap();
    let created_at = market.pool_state().average_price_timestamp;

    // The first update after the oracle moves to $115 credits the four
    // minutes since creation to $100, the price seen at creation, so the
    // average stays at $100 and $115 is recorded for the next read.
    market.pass_seconds(240);
    market.set_price(dollars(115));
    market.update_price_average(&keeper).unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.average_price, dollars(100) as u64);
    assert_eq!(pool.last_oracle_price, dollars(115) as u64);
    assert_eq!(pool.average_price_timestamp, created_at + 240);

    // Four more minutes at $115 are 240 of the 600-second window, so the next
    // update moves the average 240/600 of the way from $100 to $115: to $106.
    market.pass_seconds(240);
    market.set_price(dollars(115));
    market.update_price_average(&keeper).unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.average_price, dollars(106) as u64);
    assert_eq!(pool.average_price_timestamp, created_at + 480);

    // Fifteen minutes is more than a full window, so the next update replaces
    // the average with $115, the price at the previous read, and records the
    // fall to $97. One more update credits $97 for a full window.
    market.pass_seconds(900);
    market.set_price(dollars(97));
    market.update_price_average(&keeper).unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.average_price, dollars(115) as u64);
    assert_eq!(pool.last_oracle_price, dollars(97) as u64);
    market.pass_seconds(900);
    market.set_price(dollars(97));
    market.update_price_average(&keeper).unwrap();
    assert_eq!(market.pool_state().average_price, dollars(97) as u64);
}

/// A pool left idle for more than a window cannot have its average set by one
/// read of a manipulated price. The read only records the price; the interval
/// before it is credited to the price seen at the read before. Once a read of
/// the real price replaces it, the manipulated price has moved the average
/// only by the seconds between the two reads.
#[test]
fn test_one_manipulated_read_after_idle_does_not_move_average() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    let attacker = create_wallet(&mut market.svm, 100_000_000_000).unwrap();

    // Fifteen idle minutes, then the oracle is pushed to $160 and the
    // attacker calls `update_price_average`. The average stays at $100.
    market.pass_seconds(900);
    market.set_price(dollars(160));
    market.update_price_average(&attacker).unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.average_price, dollars(100) as u64);
    assert_eq!(pool.last_oracle_price, dollars(160) as u64);

    // Six seconds later the oracle is back at $100 and is read again. The six
    // seconds are credited to $160: the average moves 6/600 of the $60 gap,
    // to $100.60, and $100 replaces $160 as the latest observation.
    market.pass_seconds(6);
    market.set_price(dollars(100));
    market.update_price_average(&attacker).unwrap();
    let pool = market.pool_state();
    assert_eq!(pool.average_price, 10_060_000_000);
    assert_eq!(pool.last_oracle_price, dollars(100) as u64);

    // An open at $160 is still refused.
    market.set_price(dollars(160));
    assert_fails_with(
        market.open_position(&trader, trader_collateral, Side::Long, collateral, size, 0),
        PerpError::PriceOutsideBand,
    );
}
