use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program, AccountDeserialize, Address,
        InstructionData, ToAccountMetas,
    },
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
    perpetual_futures::{
        errors::PerpError,
        instructions::initialize_pool::PoolParameters,
        state::{Pool, Position, Side},
    },
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
};

// Matches `MAX_FUNDING_RATE_PER_SECOND` in the program's constants: the
// steepest funding rate `initialize_pool` accepts.
const MAX_FUNDING_RATE_PER_SECOND: u64 = 277;
// Matches `PRICE_AVERAGE_WINDOW_SECONDS`: one fold after this many seconds
// replaces the pool's average price with the oracle price.
const PRICE_AVERAGE_WINDOW_SECONDS: i64 = 600;
// Ten years, in seconds.
const TEN_YEARS: i64 = 315_360_000;
// Collateral token has 6 decimals (like USDC), so one whole unit is 1_000_000
// base units.
const ONE_USDC: u64 = 1_000_000;
const DECIMALS: u8 = 6;

// The oracle quotes prices with 8 decimals, so $100 is 100 * 10^8.
const ORACLE_SCALE: u32 = 8;

fn token_program_id() -> Address {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

fn ata_program_id() -> Address {
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap()
}

fn derive_ata(wallet: &Address, mint: &Address) -> Address {
    Address::find_program_address(
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
/// open and close fees, a 10% initial margin (10x leverage), a 5% maintenance
/// margin, a 1% liquidation fee, a 1% maximum confidence band, and a 20% price
/// band around the pool's average price.
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
    }
}

/// Assert that `result` failed with the program's `expected` error. Anchor
/// reports a program error as `Custom(6000 + the variant's index)`.
fn assert_fails_with<T>(result: Result<T, String>, expected: PerpError) {
    let code = expected as u32 + 6000;
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
    collateral_mint: Address,
    feed: Address,
    pool: Address,
    lp_mint: Address,
    custody_vault: Address,
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
        let mut svm = anchor_v2_testing::svm();
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
                system_program: system_program::ID,
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

        let pool = Address::find_program_address(
            &[b"pool", collateral_mint.as_ref(), feed.as_ref()],
            &perpetual_futures::id(),
        )
        .0;
        let lp_mint =
            Address::find_program_address(&[b"lp_mint", pool.as_ref()], &perpetual_futures::id()).0;
        let custody_vault =
            Address::find_program_address(&[b"vault", pool.as_ref()], &perpetual_futures::id()).0;

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
                system_program: system_program::ID,
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
        let mut clock = self.svm.get_sysvar::<solana_clock::Clock>();
        clock.unix_timestamp += seconds;
        self.svm.set_sysvar(&clock);
        self.svm.expire_blockhash();
    }

    fn current_slot(&self) -> u64 {
        self.svm.get_sysvar::<solana_clock::Clock>().slot
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
    fn funded_trader(&mut self, amount: u64) -> (Keypair, Address) {
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
        provider_collateral: Address,
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
                system_program: system_program::ID,
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
        provider_collateral: Address,
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
                system_program: system_program::ID,
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

    fn position_pda(&self, owner: &Address, side: Side) -> Address {
        let side_seed: &[u8] = match side {
            Side::Long => b"long",
            Side::Short => b"short",
        };
        Address::find_program_address(
            &[b"position", self.pool.as_ref(), owner.as_ref(), side_seed],
            &perpetual_futures::id(),
        )
        .0
    }

    fn open_position(
        &mut self,
        trader: &Keypair,
        trader_collateral: Address,
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
                system_program: system_program::ID,
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
        trader_collateral: Address,
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
                system_program: system_program::ID,
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
        owner: &Address,
        owner_collateral: Address,
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
                system_program: system_program::ID,
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
                system_program: system_program::ID,
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
    fn seed_liquidity(&mut self, amount: u64) -> (Keypair, Address) {
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
    assert_eq!(pool.oracle_scale, ORACLE_SCALE);
    assert_eq!(pool.initial_margin_bps, 1_000);
    assert_eq!(pool.max_price_deviation_bps, 2_000);
    // The average starts at the oracle price the pool was created against.
    assert_eq!(pool.average_price, dollars(100) as u64);
    assert_eq!(
        pool.average_price_timestamp,
        market
            .svm
            .get_sysvar::<solana_clock::Clock>()
            .unix_timestamp
    );
    assert_eq!(pool.liquidity, 0);
    assert_eq!(pool.total_collateral, 0);

    // The pool account itself owns the custody vault and is the LP mint's
    // authority; there is no separate signing PDA. A token account keeps its
    // owner at bytes 32..64, and a mint keeps its authority at bytes 4..36
    // behind a four-byte `COption` tag.
    let vault = market.svm.get_account(&market.custody_vault).unwrap();
    let vault_owner = Address::new_from_array(vault.data[32..64].try_into().unwrap());
    assert_eq!(vault_owner, market.pool);
    let lp_mint = market.svm.get_account(&market.lp_mint).unwrap();
    let mint_authority = Address::new_from_array(lp_mint.data[4..36].try_into().unwrap());
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

#[test]
fn test_first_deposit_below_minimum_fails() {
    let mut market = Market::default_market();
    let (provider, provider_collateral) = market.funded_trader(10_000);
    // 500 base units is below the 1_000 locked minimum.
    assert!(market
        .add_liquidity(&provider, provider_collateral, 500, 0)
        .is_err());
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
    // The steepest rate a pool may have, held for ten years. The position is
    // tiny because a pool holding 1_001 can back only 1_001 of notional.
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

    // The pool holds 1_001, so it can back a position of up to 1_001 notional.
    // Heavy collateral keeps the position far from liquidation while funding
    // drains it into `liquidity`.
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
    // Collateral minus the 0.1% open fee is now tracked as trader collateral.
    let open_fee = size / 1_000;
    assert_eq!(pool.total_collateral, collateral - open_fee);
    assert_eq!(pool.program_fees, open_fee);
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

    // Price rises 20%: a $5,000 long earns $1,000.
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

    assert!(market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            0,
            5_000 * ONE_USDC,
            0
        )
        .is_err());
    assert!(market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            1_000 * ONE_USDC,
            0,
            0
        )
        .is_err());
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
    assert!(market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            acceptable_price
        )
        .is_err());
}

#[test]
fn test_stale_price_rejected() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    // Move far past the staleness window without refreshing the feed.
    market.warp(10_000);
    assert!(market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            0
        )
        .is_err());
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

    assert!(market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            u64::MAX
        )
        .is_err());

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

#[test]
fn test_wide_oracle_confidence_rejected() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    // The pool tolerates a 1% confidence band (max_confidence_bps = 100). Widen
    // the feed's band to 2% of the price and the open must be rejected.
    market.set_price_with_confidence(dollars(100), dollars(2) as u64);
    assert!(market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            0
        )
        .is_err());
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
    market.pass_seconds(2_000);
    market.set_price(dollars(100));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let payout = get_token_account_balance(&market.svm, &trader_collateral).unwrap();

    // The trader received less than collateral-minus-close-fee; the shortfall
    // is the funding they paid, which went to the liquidity providers.
    assert!(payout < net_collateral - close_fee);
    let funding_paid = (net_collateral - close_fee) - payout;
    assert!(funding_paid > 0);
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
    let expected = size * MAX_FUNDING_RATE_PER_SECOND * one_hour as u64 / 1_000_000_000;
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
    assert!(market
        .liquidate(&liquidator, &trader.pubkey(), trader_collateral, Side::Long)
        .is_err());
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

    // Nothing left to claim on a second sweep.
    assert!(market.collect_fees(&admin).is_err());
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
    assert!(market.collect_fees(&imposter).is_err());
}

#[test]
fn test_open_rejects_when_pool_cannot_back_it() {
    let mut market = Market::default_market();
    // Only 3,000 of liquidity, but a 5,000 position must reserve 5,000.
    market.seed_liquidity(3_000 * ONE_USDC);
    let (trader, trader_collateral) = market.funded_trader(1_000 * ONE_USDC);
    assert!(market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            1_000 * ONE_USDC,
            5_000 * ONE_USDC,
            0
        )
        .is_err());
}

#[test]
fn test_profit_capped_at_reserved_notional() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 2_000 * ONE_USDC;
    let size = 5_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);
    market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .unwrap();

    // Price triples: uncapped profit would be 2x the notional, but recoverable
    // profit is capped at the reserved notional (`size`). A move this large is
    // far outside the price band, so the average has to catch up before the
    // position can close.
    market.set_price(dollars(300));
    market.settle_average_at(dollars(300));
    market
        .close_position(&trader, trader_collateral, Side::Long, 0)
        .unwrap();

    let open_fee = size / 1_000;
    let close_fee = size / 1_000;
    let net_collateral = collateral - open_fee;
    let expected = net_collateral + size - close_fee;
    assert_eq!(
        get_token_account_balance(&market.svm, &trader_collateral).unwrap(),
        expected
    );
}

#[test]
fn test_remove_liquidity_blocked_by_reserved() {
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

    // 5,000 of the 10,000 liquidity is now reserved. Pulling everything fails,
    // but withdrawing within the free half succeeds.
    let provider_lp = derive_ata(&provider.pubkey(), &market.lp_mint);
    let shares = get_token_account_balance(&market.svm, &provider_lp).unwrap();
    assert!(market
        .remove_liquidity(&provider, provider_collateral, shares, 0)
        .is_err());
    assert!(market
        .remove_liquidity(&provider, provider_collateral, shares / 2, 0)
        .is_ok());
}

#[test]
fn test_initialize_pool_rejects_close_fee_at_or_above_maintenance_margin() {
    // A pool whose close fee reached the maintenance margin could strand a
    // position that is too healthy to liquidate but too poor to pay the fee to
    // close, so initialize_pool refuses the configuration.
    let parameters = PoolParameters {
        close_fee_bps: 600,
        ..default_parameters(0)
    };
    assert_fails_with(
        Market::try_new(dollars(100), parameters),
        PerpError::InvalidParameter,
    );
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

    // At $115, inside the band, the close goes through and pays the 15% gain.
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
