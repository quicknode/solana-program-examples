use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    perpetual_futures::{instructions::initialize_pool::PoolParameters, state::Pool, state::Side},
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
    solana_signer::Signer,
};

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

/// One deployed market plus the keys needed to drive it.
struct Market {
    svm: LiteSVM,
    payer: Keypair,
    admin: Keypair,
    collateral_mint: Pubkey,
    feed: Pubkey,
    pool: Pubkey,
    pool_authority: Pubkey,
    lp_mint: Pubkey,
    custody_vault: Pubkey,
}

impl Market {
    /// Stand up a market with the given starting oracle price and per-slot
    /// funding rate. The admin is both the pool authority and the oracle feed
    /// authority.
    fn new(initial_price: i128, funding_rate_per_slot: u64) -> Market {
        let parameters = PoolParameters {
            oracle_scale: ORACLE_SCALE,
            funding_rate_per_slot,
            open_fee_bps: 10,
            close_fee_bps: 10,
            max_leverage: 10,
            maintenance_margin_bps: 500,
            liquidation_fee_bps: 100,
            max_confidence_bps: 100,
        };
        Market::try_new(initial_price, parameters).expect("pool initialization should succeed")
    }

    /// Like `new`, but takes the full parameter set and surfaces an
    /// `initialize_pool` rejection instead of panicking, so tests can probe the
    /// parameter validation.
    fn try_new(initial_price: i128, parameters: PoolParameters) -> Result<Market, ()> {
        let mut svm = LiteSVM::new();
        svm.add_program(
            perpetual_futures::id(),
            include_bytes!("../../../target/deploy/perpetual_futures.so"),
        )
        .unwrap();
        // Use std::fs::read() instead of include_bytes!() for the switchboard program because
        // include_bytes!() runs at compile time, and during `anchor build` the IDL generation
        // step compiles tests before the .so files exist. Since this is a cross-program
        // dependency (not our own program), mock_switchboard.so may not be built yet at compile time.
        let switchboard_bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/deploy/mock_switchboard.so"
        ))
        .expect("mock_switchboard.so not found - run `anchor build` first");
        svm.add_program(mock_switchboard::id(), &switchboard_bytes).unwrap();

        let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let admin = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let collateral_mint = create_token_mint(&mut svm, &admin, DECIMALS, None).unwrap();

        // Create the mock oracle feed as a fresh account owned by the mock
        // program; the admin is its update authority.
        let feed_keypair = Keypair::new();
        let initialize_feed = Instruction::new_with_bytes(
            mock_switchboard::id(),
            &mock_switchboard::instruction::InitializeFeed {
                price: initial_price,
                scale: ORACLE_SCALE,
                confidence: 0,
            }
            .data(),
            mock_switchboard::accounts::InitializeFeedAccountConstraints {
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
        let pool_authority =
            Pubkey::find_program_address(&[b"authority", pool.as_ref()], &perpetual_futures::id())
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
                pool_authority,
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
        .map_err(|_| ())?;

        Ok(Market {
            svm,
            payer,
            admin,
            collateral_mint,
            feed,
            pool,
            pool_authority,
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
            mock_switchboard::id(),
            &mock_switchboard::instruction::SetPrice { price, confidence }.data(),
            mock_switchboard::accounts::SetPriceAccountConstraints {
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

    fn warp(&mut self, slot: u64) {
        self.svm.warp_to_slot(slot);
        self.svm.expire_blockhash();
    }

    fn current_slot(&self) -> u64 {
        self.svm.get_sysvar::<anchor_lang::prelude::Clock>().slot
    }

    /// Simulate a cluster restart at `slot`: prices stamped at or before it
    /// must be rejected until the publisher posts again.
    fn set_last_restart_slot(&mut self, slot: u64) {
        self.svm.set_sysvar(&solana_sysvar::last_restart_slot::LastRestartSlot {
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
    ) -> Result<(), ()> {
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
                pool_authority: self.pool_authority,
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
        .map(|_| ())
        .map_err(|_| ())
    }

    fn remove_liquidity(
        &mut self,
        provider: &Keypair,
        provider_collateral: Pubkey,
        shares: u64,
        minimum_amount_out: u64,
    ) -> Result<(), ()> {
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
                pool_authority: self.pool_authority,
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
        .map(|_| ())
        .map_err(|_| ())
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
    ) -> Result<(), ()> {
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
        .map(|_| ())
        .map_err(|_| ())
    }

    fn close_position(
        &mut self,
        trader: &Keypair,
        trader_collateral: Pubkey,
        side: Side,
        minimum_payout: u64,
    ) -> Result<(), ()> {
        let position = self.position_pda(&trader.pubkey(), side);
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::ClosePosition { minimum_payout }.data(),
            perpetual_futures::accounts::ClosePositionAccountConstraints {
                owner: trader.pubkey(),
                pool: self.pool,
                position,
                pool_authority: self.pool_authority,
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
        .map(|_| ())
        .map_err(|_| ())
    }

    fn liquidate(
        &mut self,
        liquidator: &Keypair,
        owner: &Pubkey,
        owner_collateral: Pubkey,
        side: Side,
    ) -> Result<(), ()> {
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
                pool_authority: self.pool_authority,
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
        .map(|_| ())
        .map_err(|_| ())
    }

    fn collect_fees(&mut self, authority: &Keypair) -> Result<(), ()> {
        let authority_collateral = derive_ata(&authority.pubkey(), &self.collateral_mint);
        let instruction = Instruction::new_with_bytes(
            perpetual_futures::id(),
            &perpetual_futures::instruction::CollectFees {}.data(),
            perpetual_futures::accounts::CollectFeesAccountConstraints {
                authority: authority.pubkey(),
                pool: self.pool,
                pool_authority: self.pool_authority,
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
        .map(|_| ())
        .map_err(|_| ())
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
    assert_eq!(pool.oracle_scale, ORACLE_SCALE);
    assert_eq!(pool.max_leverage, 10);
    assert_eq!(pool.liquidity, 0);
    assert_eq!(pool.total_collateral, 0);
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
    // supply before second deposit was `first - 1_000`; second shares =
    // second * supply / aum = second * (first - 1_000) / first.
    let expected = ((second as u128) * ((first - 1_000) as u128) / (first as u128)) as u64;
    assert_eq!(shares, expected);
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

    // As the only liquidity provider, they reclaim the full deposit: their
    // shares carry the whole pool, since the withheld minimum was never minted
    // to anyone else to hold it back.
    let returned = get_token_account_balance(&market.svm, &provider_collateral).unwrap();
    assert_eq!(returned, deposit);
    assert_eq!(market.pool_state().liquidity, 0);
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
    assert_eq!(pool.protocol_fees, open_fee);
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
fn test_open_rejects_excess_leverage() {
    let mut market = Market::default_market();
    market.seed_liquidity(100_000 * ONE_USDC);
    let collateral = 1_000 * ONE_USDC;
    let (trader, trader_collateral) = market.funded_trader(collateral);

    // max_leverage is 10x; 11x must be rejected.
    let size = 11_000 * ONE_USDC;
    assert!(market
        .open_position(&trader, trader_collateral, Side::Long, collateral, size, 0)
        .is_err());
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

    // Publishing after the restart reopens the pool.
    market.set_price(dollars(100));
    market
        .open_position(
            &trader,
            trader_collateral,
            Side::Long,
            collateral,
            5_000 * ONE_USDC,
            u64::MAX
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
    let mut market = Market::new(dollars(100), 5_000);
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
    market.warp(2_000);
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

    let fees = market.pool_state().protocol_fees;
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
    assert_eq!(market.pool_state().protocol_fees, 0);

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
    // profit is capped at the reserved notional (`size`).
    market.set_price(dollars(300));
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
        oracle_scale: ORACLE_SCALE,
        funding_rate_per_slot: 0,
        open_fee_bps: 10,
        close_fee_bps: 600,
        max_leverage: 10,
        maintenance_margin_bps: 500,
        liquidation_fee_bps: 100,
        max_confidence_bps: 100,
    };
    assert!(Market::try_new(dollars(100), parameters).is_err());
}
