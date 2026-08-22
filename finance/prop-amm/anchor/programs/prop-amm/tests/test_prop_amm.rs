use {
    anchor_lang::{
        solana_program::instruction::Instruction, system_program, AccountDeserialize, Address,
        InstructionData, ToAccountMetas,
    },
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
    prop_amm::{
        instructions::initialize_market::MarketParameters,
        state::{Direction, Market as MarketState},
    },
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
};

// Both tokens have 6 decimals: the base is NVDAx (tokenized NVIDIA stock) and
// the quote is USDC, so one whole unit of either is 1_000_000 minor units.
const ONE_TOKEN: u64 = 1_000_000;
const DECIMALS: u8 = 6;

// The oracle quotes prices with 8 decimals, so $165 is 165 * 10^8.
const ORACLE_SCALE: u32 = 8;

// 10 basis points each side of the oracle price.
const SPREAD_BPS: u16 = 10;
const MAX_CONFIDENCE_BPS: u16 = 100;

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

/// One deployed market plus the keys needed to drive it.
struct Market {
    svm: LiteSVM,
    payer: Keypair,
    operator: Keypair,
    operator_base: Address,
    operator_quote: Address,
    base_mint: Address,
    quote_mint: Address,
    feed: Address,
    market: Address,
    market_authority: Address,
    base_vault: Address,
    quote_vault: Address,
}

impl Market {
    /// Stand up a market at the given starting oracle price. The operator is
    /// also the oracle feed authority and holds funded inventory accounts.
    fn new(initial_price: i128) -> Market {
        let parameters = MarketParameters {
            oracle_scale: ORACLE_SCALE,
            spread_bps: SPREAD_BPS,
            max_confidence_bps: MAX_CONFIDENCE_BPS,
        };
        Market::try_new(initial_price, parameters).expect("market initialization should succeed")
    }

    /// Like `new`, but takes the full parameter set and surfaces an
    /// `initialize_market` rejection instead of panicking, so tests can probe
    /// the parameter validation.
    fn try_new(initial_price: i128, parameters: MarketParameters) -> Result<Market, ()> {
        let mut svm = anchor_v2_testing::svm();
        svm.add_program(
            prop_amm::id(),
            include_bytes!("../../../target/deploy/prop_amm.so"),
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
        svm.add_program(mock_switchboard::id(), &switchboard_bytes)
            .unwrap();

        let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let operator = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let base_mint = create_token_mint(&mut svm, &operator, DECIMALS, None).unwrap();
        let quote_mint = create_token_mint(&mut svm, &operator, DECIMALS, None).unwrap();

        // Create the mock oracle feed as a fresh account owned by the mock
        // program; the operator is its update authority.
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
                authority: operator.pubkey(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut svm,
            vec![initialize_feed],
            &[&operator, &feed_keypair],
            &operator.pubkey(),
        )
        .unwrap();
        let feed = feed_keypair.pubkey();

        let market = Address::find_program_address(
            &[b"market", base_mint.as_ref(), quote_mint.as_ref()],
            &prop_amm::id(),
        )
        .0;
        let market_authority =
            Address::find_program_address(&[b"authority", market.as_ref()], &prop_amm::id()).0;
        let base_vault =
            Address::find_program_address(&[b"base_vault", market.as_ref()], &prop_amm::id()).0;
        let quote_vault =
            Address::find_program_address(&[b"quote_vault", market.as_ref()], &prop_amm::id()).0;

        let initialize_market = Instruction::new_with_bytes(
            prop_amm::id(),
            &prop_amm::instruction::InitializeMarket { parameters }.data(),
            prop_amm::accounts::InitializeMarketAccountConstraints {
                operator: operator.pubkey(),
                market,
                base_mint,
                quote_mint,
                oracle_feed: feed,
                market_authority,
                base_vault,
                quote_vault,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut svm,
            vec![initialize_market],
            &[&operator],
            &operator.pubkey(),
        )
        .map_err(|_| ())?;

        // Fund the operator's inventory accounts.
        let operator_base =
            create_associated_token_account(&mut svm, &operator.pubkey(), &base_mint, &payer)
                .unwrap();
        let operator_quote =
            create_associated_token_account(&mut svm, &operator.pubkey(), &quote_mint, &payer)
                .unwrap();
        mint_tokens_to_token_account(
            &mut svm,
            &base_mint,
            &operator_base,
            10_000 * ONE_TOKEN,
            &operator,
        )
        .unwrap();
        mint_tokens_to_token_account(
            &mut svm,
            &quote_mint,
            &operator_quote,
            10_000_000 * ONE_TOKEN,
            &operator,
        )
        .unwrap();

        Ok(Market {
            svm,
            payer,
            operator,
            operator_base,
            operator_quote,
            base_mint,
            quote_mint,
            feed,
            market,
            market_authority,
            base_vault,
            quote_vault,
        })
    }

    /// A market at $165 stocked with 1,000 NVDAx and 200,000 USDC.
    fn default_market() -> Market {
        let mut market = Market::new(dollars(165));
        market
            .deposit_inventory(1_000 * ONE_TOKEN, 200_000 * ONE_TOKEN)
            .unwrap();
        market
    }

    fn market_state(&self) -> MarketState {
        let account = self.svm.get_account(&self.market).unwrap();
        MarketState::try_deserialize(&mut account.data.as_slice()).unwrap()
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
                authority: self.operator.pubkey(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![set_price],
            &[&self.operator],
            &self.operator.pubkey(),
        )
        .unwrap();
    }

    fn warp(&mut self, slot: u64) {
        self.svm.warp_to_slot(slot);
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

    /// Create a wallet holding `base` and `quote` minor units in associated
    /// token accounts.
    fn funded_trader(&mut self, base: u64, quote: u64) -> (Keypair, Address, Address) {
        let trader = create_wallet(&mut self.svm, 100_000_000_000).unwrap();
        let base_account = create_associated_token_account(
            &mut self.svm,
            &trader.pubkey(),
            &self.base_mint,
            &self.payer,
        )
        .unwrap();
        let quote_account = create_associated_token_account(
            &mut self.svm,
            &trader.pubkey(),
            &self.quote_mint,
            &self.payer,
        )
        .unwrap();
        if base > 0 {
            mint_tokens_to_token_account(
                &mut self.svm,
                &self.base_mint,
                &base_account,
                base,
                &self.operator,
            )
            .unwrap();
        }
        if quote > 0 {
            mint_tokens_to_token_account(
                &mut self.svm,
                &self.quote_mint,
                &quote_account,
                quote,
                &self.operator,
            )
            .unwrap();
        }
        (trader, base_account, quote_account)
    }

    /// Inventory movement signed by `signer` (the operator in honest tests, an
    /// imposter in the access-control tests).
    fn move_inventory_as(
        &mut self,
        signer: &Keypair,
        deposit: bool,
        base_amount: u64,
        quote_amount: u64,
    ) -> Result<(), ()> {
        let signer_base = derive_ata(&signer.pubkey(), &self.base_mint);
        let signer_quote = derive_ata(&signer.pubkey(), &self.quote_mint);
        let instruction = if deposit {
            Instruction::new_with_bytes(
                prop_amm::id(),
                &prop_amm::instruction::DepositInventory {
                    base_amount,
                    quote_amount,
                }
                .data(),
                prop_amm::accounts::DepositInventoryAccountConstraints {
                    operator: signer.pubkey(),
                    market: self.market,
                    base_mint: self.base_mint,
                    quote_mint: self.quote_mint,
                    base_vault: self.base_vault,
                    quote_vault: self.quote_vault,
                    operator_base: signer_base,
                    operator_quote: signer_quote,
                    token_program: token_program_id(),
                }
                .to_account_metas(None),
            )
        } else {
            Instruction::new_with_bytes(
                prop_amm::id(),
                &prop_amm::instruction::WithdrawInventory {
                    base_amount,
                    quote_amount,
                }
                .data(),
                prop_amm::accounts::WithdrawInventoryAccountConstraints {
                    operator: signer.pubkey(),
                    market: self.market,
                    market_authority: self.market_authority,
                    base_mint: self.base_mint,
                    quote_mint: self.quote_mint,
                    base_vault: self.base_vault,
                    quote_vault: self.quote_vault,
                    operator_base: signer_base,
                    operator_quote: signer_quote,
                    token_program: token_program_id(),
                }
                .to_account_metas(None),
            )
        };
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[signer],
            &signer.pubkey(),
        )
        .map(|_| ())
        .map_err(|_| ())
    }

    fn deposit_inventory(&mut self, base_amount: u64, quote_amount: u64) -> Result<(), ()> {
        let operator = self.operator.insecure_clone();
        self.move_inventory_as(&operator, true, base_amount, quote_amount)
    }

    fn withdraw_inventory(&mut self, base_amount: u64, quote_amount: u64) -> Result<(), ()> {
        let operator = self.operator.insecure_clone();
        self.move_inventory_as(&operator, false, base_amount, quote_amount)
    }

    fn set_quote_as(&mut self, signer: &Keypair, spread_bps: u16, paused: bool) -> Result<(), ()> {
        let instruction = Instruction::new_with_bytes(
            prop_amm::id(),
            &prop_amm::instruction::SetQuote { spread_bps, paused }.data(),
            prop_amm::accounts::SetQuoteAccountConstraints {
                operator: signer.pubkey(),
                market: self.market,
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[signer],
            &signer.pubkey(),
        )
        .map(|_| ())
        .map_err(|_| ())
    }

    fn set_quote(&mut self, spread_bps: u16, paused: bool) -> Result<(), ()> {
        let operator = self.operator.insecure_clone();
        self.set_quote_as(&operator, spread_bps, paused)
    }

    fn swap(
        &mut self,
        trader: &Keypair,
        direction: Direction,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<(), ()> {
        let trader_base = derive_ata(&trader.pubkey(), &self.base_mint);
        let trader_quote = derive_ata(&trader.pubkey(), &self.quote_mint);
        let instruction = Instruction::new_with_bytes(
            prop_amm::id(),
            &prop_amm::instruction::Swap {
                direction,
                amount_in,
                minimum_amount_out,
            }
            .data(),
            prop_amm::accounts::SwapAccountConstraints {
                trader: trader.pubkey(),
                market: self.market,
                market_authority: self.market_authority,
                oracle_feed: self.feed,
                base_mint: self.base_mint,
                quote_mint: self.quote_mint,
                base_vault: self.base_vault,
                quote_vault: self.quote_vault,
                trader_base,
                trader_quote,
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
        .map(|_| ())
        .map_err(|_| ())
    }

    fn balance(&self, token_account: &Address) -> u64 {
        get_token_account_balance(&self.svm, token_account).unwrap()
    }
}

// ===========================================================================
// Happy paths: exact quote math in both directions
// ===========================================================================

/// Alice buys 5 NVDAx. At $165 with a 10 bps spread the ask is $165.165, so
/// 5 NVDAx costs exactly 825.825 USDC.
#[test]
fn test_swap_buys_base_at_the_ask() {
    let mut market = Market::default_market();
    let quote_in = 825_825_000; // 825.825 USDC
    let (alice, alice_base, alice_quote) = market.funded_trader(0, quote_in);

    market
        .swap(&alice, Direction::BuyBase, quote_in, 5 * ONE_TOKEN)
        .unwrap();

    assert_eq!(market.balance(&alice_base), 5 * ONE_TOKEN);
    assert_eq!(market.balance(&alice_quote), 0);
    // Conservation: the vaults moved by exactly the two legs of the fill.
    assert_eq!(market.balance(&market.base_vault), 995 * ONE_TOKEN);
    assert_eq!(
        market.balance(&market.quote_vault),
        200_000 * ONE_TOKEN + quote_in
    );
}

/// Bob sells 5 NVDAx. At $165 with a 10 bps spread the bid is $164.835, so
/// he receives exactly 824.175 USDC.
#[test]
fn test_swap_sells_base_at_the_bid() {
    let mut market = Market::default_market();
    let (bob, bob_base, bob_quote) = market.funded_trader(5 * ONE_TOKEN, 0);

    market
        .swap(&bob, Direction::SellBase, 5 * ONE_TOKEN, 824_175_000)
        .unwrap();

    assert_eq!(market.balance(&bob_base), 0);
    assert_eq!(market.balance(&bob_quote), 824_175_000); // 824.175 USDC
    assert_eq!(market.balance(&market.base_vault), 1_005 * ONE_TOKEN);
    assert_eq!(
        market.balance(&market.quote_vault),
        200_000 * ONE_TOKEN - 824_175_000
    );
}

/// A buy immediately followed by a sell of the same 5 NVDAx costs exactly the
/// round-trip spread: 1.65 USDC on an $825 position, all of which stays in
/// the market's inventory. The spread IS the fee; there is no other one.
#[test]
fn test_round_trip_costs_exactly_the_spread() {
    let mut market = Market::default_market();
    let quote_in = 825_825_000;
    let (carol, carol_base, carol_quote) = market.funded_trader(0, quote_in);

    market
        .swap(&carol, Direction::BuyBase, quote_in, 0)
        .unwrap();
    market
        .swap(&carol, Direction::SellBase, 5 * ONE_TOKEN, 0)
        .unwrap();

    assert_eq!(market.balance(&carol_base), 0);
    // 825.825 in, 824.175 back: the market kept 1.65 USDC.
    assert_eq!(market.balance(&carol_quote), quote_in - 1_650_000);
    assert_eq!(market.balance(&market.base_vault), 1_000 * ONE_TOKEN);
    assert_eq!(
        market.balance(&market.quote_vault),
        200_000 * ONE_TOKEN + 1_650_000
    );
}

/// When the oracle reprices, the quote follows instantly — no trade has to
/// drag the price there through a curve. At $170 the ask is $170.17, so 5
/// NVDAx costs exactly 850.85 USDC.
#[test]
fn test_quote_follows_the_oracle() {
    let mut market = Market::default_market();
    market.set_price(dollars(170));

    let quote_in = 850_850_000; // 850.85 USDC
    let (alice, alice_base, _) = market.funded_trader(0, quote_in);
    market
        .swap(&alice, Direction::BuyBase, quote_in, 5 * ONE_TOKEN)
        .unwrap();

    assert_eq!(market.balance(&alice_base), 5 * ONE_TOKEN);
}

/// The operator re-quotes to a 50 bps spread; the next fill prices at
/// $165.825, so 5 NVDAx costs exactly 829.125 USDC.
#[test]
fn test_set_quote_changes_the_spread() {
    let mut market = Market::default_market();
    market.set_quote(50, false).unwrap();
    assert_eq!(market.market_state().spread_bps, 50);

    let quote_in = 829_125_000; // 829.125 USDC
    let (alice, alice_base, _) = market.funded_trader(0, quote_in);
    market
        .swap(&alice, Direction::BuyBase, quote_in, 5 * ONE_TOKEN)
        .unwrap();

    assert_eq!(market.balance(&alice_base), 5 * ONE_TOKEN);
}

// ===========================================================================
// The operator's capital: deposit, withdraw, and the full exit
// ===========================================================================

/// The operator can withdraw every token in both vaults at any time — its
/// capital, its exit. Afterwards the market still exists but cannot fill,
/// which is exactly what an empty prop AMM should do: reject, not misprice.
#[test]
fn test_operator_can_withdraw_everything_and_swaps_then_fail() {
    let mut market = Market::default_market();
    market
        .withdraw_inventory(1_000 * ONE_TOKEN, 200_000 * ONE_TOKEN)
        .unwrap();

    assert_eq!(market.balance(&market.base_vault), 0);
    assert_eq!(market.balance(&market.quote_vault), 0);
    let operator_base = market.operator_base;
    let operator_quote = market.operator_quote;
    assert_eq!(market.balance(&operator_base), 10_000 * ONE_TOKEN);
    assert_eq!(market.balance(&operator_quote), 10_000_000 * ONE_TOKEN);

    let (alice, _, _) = market.funded_trader(0, 825_825_000);
    assert!(market
        .swap(&alice, Direction::BuyBase, 825_825_000, 0)
        .is_err());
}

#[test]
fn test_withdraw_more_than_inventory_fails() {
    let mut market = Market::default_market();
    assert!(market.withdraw_inventory(1_001 * ONE_TOKEN, 0).is_err());
}

#[test]
fn test_deposit_inventory_rejects_non_operator() {
    let mut market = Market::default_market();
    let (mallory, _, _) = market.funded_trader(ONE_TOKEN, ONE_TOKEN);
    assert!(market
        .move_inventory_as(&mallory, true, ONE_TOKEN, 0)
        .is_err());
}

#[test]
fn test_withdraw_inventory_rejects_non_operator() {
    let mut market = Market::default_market();
    let (mallory, _, _) = market.funded_trader(0, 0);
    assert!(market
        .move_inventory_as(&mallory, false, ONE_TOKEN, 0)
        .is_err());
}

#[test]
fn test_set_quote_rejects_non_operator() {
    let mut market = Market::default_market();
    let (mallory, _, _) = market.funded_trader(0, 0);
    assert!(market.set_quote_as(&mallory, 500, true).is_err());
}

// ===========================================================================
// Swap rejections: every gate has a test that proves it shuts
// ===========================================================================

/// A fill below the caller's minimum is rejected, not filled worse.
#[test]
fn test_swap_rejects_slippage() {
    let mut market = Market::default_market();
    let quote_in = 825_825_000;
    let (alice, _, _) = market.funded_trader(0, quote_in);
    // The fill would be exactly 5 NVDAx; demand one minor unit more.
    assert!(market
        .swap(&alice, Direction::BuyBase, quote_in, 5 * ONE_TOKEN + 1)
        .is_err());
}

/// An oracle price older than the staleness bound cannot be traded against.
/// A lagging quote is a free option for arbitrageurs, so the market refuses to
/// quote at all rather than quote wrong.
#[test]
fn test_swap_rejects_stale_price() {
    let mut market = Market::default_market();
    let (alice, _, _) = market.funded_trader(0, 825_825_000);
    // The feed was last updated near slot 0; 200 slots later it is stale
    // (the bound is 150 slots).
    market.warp(200);
    assert!(market
        .swap(&alice, Direction::BuyBase, 825_825_000, 0)
        .is_err());
}

/// A cluster restart passes hours of wall-clock time in zero slots, so a price
/// published before the halt can still look fresh by slot count. The market
/// must refuse to quote against it until the publisher posts again.
#[test]
fn test_swap_rejects_price_from_before_a_restart() {
    let mut market = Market::default_market();
    market.set_price(dollars(165));
    let (alice, _, _) = market.funded_trader(0, 825_825_000);

    // Simulate a halt: the cluster restarts a few slots after the price was
    // published, well inside the 150-slot staleness bound, so only the
    // restart check can catch the pre-halt price.
    let published_at = market.current_slot();
    market.warp(published_at + 5);
    market.set_last_restart_slot(published_at + 3);

    assert!(market
        .swap(&alice, Direction::BuyBase, 825_825_000, 0)
        .is_err());

    // Publishing after the restart reopens the market. Warp first: the retry is
    // otherwise byte-identical to the rejected swap, so it would carry the same
    // signature and be dropped as already processed.
    market.warp(published_at + 6);
    market.set_price(dollars(165));
    market
        .swap(&alice, Direction::BuyBase, 825_825_000, 0)
        .expect("a freshly published price must be accepted after a restart");
}

/// A price the oracle itself is unsure about is rejected: the confidence band
/// (about 1.2% here) exceeds the market's 1% limit.
#[test]
fn test_swap_rejects_wide_confidence() {
    let mut market = Market::default_market();
    market.set_price_with_confidence(dollars(165), 200_000_000);
    let (alice, _, _) = market.funded_trader(0, 825_825_000);
    assert!(market
        .swap(&alice, Direction::BuyBase, 825_825_000, 0)
        .is_err());
}

/// While the operator has pulled its quotes, nobody can swap.
#[test]
fn test_swap_rejects_when_paused() {
    let mut market = Market::default_market();
    market.set_quote(SPREAD_BPS, true).unwrap();
    let (alice, _, _) = market.funded_trader(0, 825_825_000);
    assert!(market
        .swap(&alice, Direction::BuyBase, 825_825_000, 0)
        .is_err());

    // Unpausing restores the exact same quote.
    market.set_quote(SPREAD_BPS, false).unwrap();
    market
        .swap(&alice, Direction::BuyBase, 825_825_000, 5 * ONE_TOKEN)
        .unwrap();
}

#[test]
fn test_swap_rejects_zero_amount() {
    let mut market = Market::default_market();
    let (alice, _, _) = market.funded_trader(0, ONE_TOKEN);
    assert!(market.swap(&alice, Direction::BuyBase, 0, 0).is_err());
}

/// A buy bigger than the base inventory is rejected whole — a prop AMM never
/// partially fills, and never prices what it cannot deliver.
#[test]
fn test_swap_rejects_insufficient_inventory() {
    let mut market = Market::default_market();
    // 1,100 NVDAx at $165.165 ≈ 181,681.50 USDC — affordable for the trader,
    // but the vault only holds 1,000 NVDAx.
    let quote_in = 181_681_500_000;
    let (whale, _, _) = market.funded_trader(0, quote_in);
    assert!(market
        .swap(&whale, Direction::BuyBase, quote_in, 0)
        .is_err());
}

// ===========================================================================
// Parameter validation
// ===========================================================================

#[test]
fn test_initialize_market_rejects_zero_spread() {
    let parameters = MarketParameters {
        oracle_scale: ORACLE_SCALE,
        spread_bps: 0,
        max_confidence_bps: MAX_CONFIDENCE_BPS,
    };
    assert!(Market::try_new(dollars(165), parameters).is_err());
}

#[test]
fn test_initialize_market_rejects_full_spread() {
    let parameters = MarketParameters {
        oracle_scale: ORACLE_SCALE,
        spread_bps: 10_000,
        max_confidence_bps: MAX_CONFIDENCE_BPS,
    };
    assert!(Market::try_new(dollars(165), parameters).is_err());
}

#[test]
fn test_set_quote_rejects_invalid_spread() {
    let mut market = Market::default_market();
    assert!(market.set_quote(0, false).is_err());
    assert!(market.set_quote(10_000, false).is_err());
}
