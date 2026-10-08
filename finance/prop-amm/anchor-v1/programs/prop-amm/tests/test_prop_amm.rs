mod transaction_v1;

use transaction_v1::send_transaction_from_instructions;

use {
    anchor_lang::{
        error::{ErrorCode as AnchorErrorCode, ERROR_CODE_OFFSET},
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    prop_amm::{
        errors::PropAmmError,
        instructions::initialize_market::MarketParameters,
        state::{Direction, Market as MarketState},
    },
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
    },
    solana_signer::Signer,
};

// The base is NVDAx (tokenized NVIDIA stock), which has 8 decimals; the quote
// is USDC, which has 6. The program reads both from the mints, so nothing in
// the quote math assumes they match.
const NVDAX_DECIMALS: u8 = 8;
const ONE_NVDAX: u64 = 100_000_000;
const USDC_DECIMALS: u8 = 6;
const ONE_USDC: u64 = 1_000_000;

// The walkthrough trade: at $165 with a 10 bps spread the ask is $165.165 and
// the bid $164.835, so 5 NVDAx costs 825.825 USDC and sells for 824.175. Both
// are exact in USDC's six decimals because the ask and bid have only three
// decimal places of a dollar, and 5 is a whole number of NVDAx whatever the
// token's decimals.
const FIVE_NVDAX: u64 = 5 * ONE_NVDAX;
const FIVE_NVDAX_AT_THE_ASK: u64 = 825_825_000; // 825.825 USDC
const FIVE_NVDAX_AT_THE_BID: u64 = 824_175_000; // 824.175 USDC

// The oracle quotes prices with 8 decimals, so $165 is 165 * 10^8.
const ORACLE_SCALE: u32 = 8;

// 10 basis points each side of the oracle price.
const SPREAD_BPS: u16 = 10;
const MAX_CONFIDENCE_BPS: u16 = 100;

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

/// A failed transaction reports its program error as `Custom(n)`. Matching
/// `n` checks that the transaction failed for the rule under test, not for
/// some unrelated reason.
fn assert_fails_with_code<T>(result: Result<T, String>, code: u32) {
    let Err(error) = result else {
        panic!("the transaction should have failed with error code {code}");
    };
    assert!(
        error.contains(&format!("Custom({code})")),
        "expected error code {code}, got: {error}"
    );
}

/// Assert that `result` failed with the program's `expected` error. Anchor
/// numbers a program's own errors from `ERROR_CODE_OFFSET` in declaration
/// order.
fn assert_fails_with<T>(result: Result<T, String>, expected: PropAmmError) {
    assert_fails_with_code(result, ERROR_CODE_OFFSET + expected as u32);
}

/// Anchor's own constraint errors (a `has_one` mismatch, for instance) are
/// numbered below the program's and reported the same way.
fn assert_fails_with_anchor_error<T>(result: Result<T, String>, expected: AnchorErrorCode) {
    assert_fails_with_code(result, expected as u32);
}

/// One deployed market plus the keys needed to drive it.
struct Market {
    svm: LiteSVM,
    payer: Keypair,
    operator: Keypair,
    operator_base: Pubkey,
    operator_quote: Pubkey,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    feed: Pubkey,
    market: Pubkey,
    base_vault: Pubkey,
    quote_vault: Pubkey,
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
    fn try_new(initial_price: i128, parameters: MarketParameters) -> Result<Market, String> {
        let mut svm = LiteSVM::new();
        svm.add_program(
            prop_amm::id(),
            include_bytes!("../../../target/deploy/prop_amm.so"),
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
        let operator = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let base_mint = create_token_mint(&mut svm, &operator, NVDAX_DECIMALS, None).unwrap();
        let quote_mint = create_token_mint(&mut svm, &operator, USDC_DECIMALS, None).unwrap();

        // Create the mock oracle feed as a fresh account owned by the mock
        // program; the operator is its update authority.
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
                authority: operator.pubkey(),
                system_program: system_program::id(),
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

        let market = Pubkey::find_program_address(
            &[b"market", base_mint.as_ref(), quote_mint.as_ref()],
            &prop_amm::id(),
        )
        .0;
        let base_vault =
            Pubkey::find_program_address(&[b"base_vault", market.as_ref()], &prop_amm::id()).0;
        let quote_vault =
            Pubkey::find_program_address(&[b"quote_vault", market.as_ref()], &prop_amm::id()).0;

        let initialize_market = Instruction::new_with_bytes(
            prop_amm::id(),
            &prop_amm::instruction::InitializeMarket { parameters }.data(),
            prop_amm::accounts::InitializeMarketAccountConstraints {
                operator: operator.pubkey(),
                market,
                base_mint,
                quote_mint,
                oracle_feed: feed,
                base_vault,
                quote_vault,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut svm,
            vec![initialize_market],
            &[&operator],
            &operator.pubkey(),
        )
        .map_err(|error| format!("{error:?}"))?;

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
            10_000 * ONE_NVDAX,
            &operator,
        )
        .unwrap();
        mint_tokens_to_token_account(
            &mut svm,
            &quote_mint,
            &operator_quote,
            10_000_000 * ONE_USDC,
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
            base_vault,
            quote_vault,
        })
    }

    /// A market at $165 stocked with 1,000 NVDAx and 200,000 USDC.
    fn default_market() -> Market {
        let mut market = Market::new(dollars(165));
        market
            .deposit_inventory(1_000 * ONE_NVDAX, 200_000 * ONE_USDC)
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
            mock_price_feed::id(),
            &mock_price_feed::instruction::SetPrice { price, confidence }.data(),
            mock_price_feed::accounts::SetPriceAccountConstraints {
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
        self.svm.get_sysvar::<anchor_lang::prelude::Clock>().slot
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
    fn funded_trader(&mut self, base: u64, quote: u64) -> (Keypair, Pubkey, Pubkey) {
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
    ) -> Result<(), String> {
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
        .map_err(|error| format!("{error:?}"))
    }

    fn deposit_inventory(&mut self, base_amount: u64, quote_amount: u64) -> Result<(), String> {
        let operator = self.operator.insecure_clone();
        self.move_inventory_as(&operator, true, base_amount, quote_amount)
    }

    fn withdraw_inventory(&mut self, base_amount: u64, quote_amount: u64) -> Result<(), String> {
        let operator = self.operator.insecure_clone();
        self.move_inventory_as(&operator, false, base_amount, quote_amount)
    }

    fn set_quote_as(
        &mut self,
        signer: &Keypair,
        spread_bps: u16,
        paused: bool,
    ) -> Result<(), String> {
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
        .map_err(|error| format!("{error:?}"))
    }

    fn set_quote(&mut self, spread_bps: u16, paused: bool) -> Result<(), String> {
        let operator = self.operator.insecure_clone();
        self.set_quote_as(&operator, spread_bps, paused)
    }

    fn swap(
        &mut self,
        trader: &Keypair,
        direction: Direction,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<(), String> {
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
                oracle_feed: self.feed,
                base_mint: self.base_mint,
                quote_mint: self.quote_mint,
                base_vault: self.base_vault,
                quote_vault: self.quote_vault,
                trader_base,
                trader_quote,
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
        .map_err(|error| format!("{error:?}"))
    }

    /// Replace the feed account at the market's recorded feed address with a
    /// copy whose owning program is `owner`. The bytes are unchanged, so the
    /// copy still decodes as a fresh, confident price at the pinned scale.
    fn set_feed_owner(&mut self, owner: Pubkey) {
        let mut feed_account = self.svm.get_account(&self.feed).unwrap();
        feed_account.owner = owner;
        self.svm.set_account(self.feed, feed_account).unwrap();
    }

    /// Close the market signed by `signer` (the operator in honest tests, an
    /// imposter in the access-control test). The payer covers the transaction
    /// fee, so the signer's lamports move only by the rent the close returns.
    fn close_market_as(&mut self, signer: &Keypair) -> Result<(), String> {
        let instruction = Instruction::new_with_bytes(
            prop_amm::id(),
            &prop_amm::instruction::CloseMarket {}.data(),
            prop_amm::accounts::CloseMarketAccountConstraints {
                operator: signer.pubkey(),
                market: self.market,
                base_vault: self.base_vault,
                quote_vault: self.quote_vault,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        let payer = self.payer.insecure_clone();
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[&payer, signer],
            &payer.pubkey(),
        )
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
    }

    fn close_market(&mut self) -> Result<(), String> {
        let operator = self.operator.insecure_clone();
        self.close_market_as(&operator)
    }

    /// A plain SPL Token `TransferChecked` of `amount` minor units from
    /// `from_account` (owned by `sender`) straight into `vault`. Nothing in
    /// the market program runs: this is a third party donating tokens to a
    /// vault, not the operator's `deposit_inventory`. The instruction is
    /// built by hand (tag 12, amount, decimals) with the token program's
    /// account order: source, mint, destination, owner.
    fn donate_to_vault(
        &mut self,
        sender: &Keypair,
        from_account: &Pubkey,
        mint: &Pubkey,
        decimals: u8,
        vault: &Pubkey,
        amount: u64,
    ) {
        let mut data = vec![12u8];
        data.extend_from_slice(&amount.to_le_bytes());
        data.push(decimals);
        let transfer = Instruction::new_with_bytes(
            token_program_id(),
            &data,
            vec![
                anchor_lang::solana_program::instruction::AccountMeta::new(*from_account, false),
                anchor_lang::solana_program::instruction::AccountMeta::new_readonly(*mint, false),
                anchor_lang::solana_program::instruction::AccountMeta::new(*vault, false),
                anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
                    sender.pubkey(),
                    true,
                ),
            ],
        );
        send_transaction_from_instructions(
            &mut self.svm,
            vec![transfer],
            &[sender],
            &sender.pubkey(),
        )
        .expect("a plain token transfer into a vault should succeed");
    }

    fn lamports(&self, address: &Pubkey) -> u64 {
        self.svm
            .get_account(address)
            .map_or(0, |account| account.lamports)
    }

    /// Cut the feed account's data to `length` bytes, keeping its owner, so
    /// the market's owner check passes and only the layout check can refuse it.
    fn truncate_feed(&mut self, length: usize) {
        let mut feed_account = self.svm.get_account(&self.feed).unwrap();
        feed_account.data.truncate(length);
        self.svm.set_account(self.feed, feed_account).unwrap();
    }

    fn balance(&self, token_account: &Pubkey) -> u64 {
        get_token_account_balance(&self.svm, token_account).unwrap()
    }

    /// The owner field of a token account: bytes 32..64 of the SPL Token
    /// account layout, after the mint.
    fn token_account_owner(&self, token_account: &Pubkey) -> Pubkey {
        let account = self.svm.get_account(token_account).unwrap();
        Pubkey::try_from(&account.data[32..64]).unwrap()
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
    let quote_in = FIVE_NVDAX_AT_THE_ASK;
    let (alice, alice_base, alice_quote) = market.funded_trader(0, quote_in);

    market
        .swap(&alice, Direction::BuyBase, quote_in, FIVE_NVDAX)
        .unwrap();

    assert_eq!(market.balance(&alice_base), FIVE_NVDAX);
    assert_eq!(market.balance(&alice_quote), 0);
    // Conservation: the vaults moved by exactly the two legs of the fill.
    assert_eq!(market.balance(&market.base_vault), 995 * ONE_NVDAX);
    assert_eq!(
        market.balance(&market.quote_vault),
        200_000 * ONE_USDC + quote_in
    );
}

/// Bob sells 5 NVDAx. At $165 with a 10 bps spread the bid is $164.835, so
/// he receives exactly 824.175 USDC.
#[test]
fn test_swap_sells_base_at_the_bid() {
    let mut market = Market::default_market();
    let (bob, bob_base, bob_quote) = market.funded_trader(FIVE_NVDAX, 0);

    market
        .swap(&bob, Direction::SellBase, FIVE_NVDAX, FIVE_NVDAX_AT_THE_BID)
        .unwrap();

    assert_eq!(market.balance(&bob_base), 0);
    assert_eq!(market.balance(&bob_quote), FIVE_NVDAX_AT_THE_BID);
    assert_eq!(market.balance(&market.base_vault), 1_005 * ONE_NVDAX);
    assert_eq!(
        market.balance(&market.quote_vault),
        200_000 * ONE_USDC - FIVE_NVDAX_AT_THE_BID
    );
}

/// A buy immediately followed by a sell of the same 5 NVDAx costs exactly the
/// round-trip spread: 1.65 USDC on an $825 position, all of which stays in
/// the market's inventory. The spread IS the fee; there is no other one.
#[test]
fn test_round_trip_costs_exactly_the_spread() {
    let mut market = Market::default_market();
    let quote_in = FIVE_NVDAX_AT_THE_ASK;
    let (carol, carol_base, carol_quote) = market.funded_trader(0, quote_in);

    market
        .swap(&carol, Direction::BuyBase, quote_in, 0)
        .unwrap();
    market
        .swap(&carol, Direction::SellBase, FIVE_NVDAX, 0)
        .unwrap();

    assert_eq!(market.balance(&carol_base), 0);
    // 825.825 in, 824.175 back: the market kept 1.65 USDC.
    assert_eq!(market.balance(&carol_quote), quote_in - 1_650_000);
    assert_eq!(market.balance(&market.base_vault), 1_000 * ONE_NVDAX);
    assert_eq!(
        market.balance(&market.quote_vault),
        200_000 * ONE_USDC + 1_650_000
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
        .swap(&alice, Direction::BuyBase, quote_in, FIVE_NVDAX)
        .unwrap();

    assert_eq!(market.balance(&alice_base), FIVE_NVDAX);
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
        .swap(&alice, Direction::BuyBase, quote_in, FIVE_NVDAX)
        .unwrap();

    assert_eq!(market.balance(&alice_base), FIVE_NVDAX);
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
        .withdraw_inventory(1_000 * ONE_NVDAX, 200_000 * ONE_USDC)
        .unwrap();

    assert_eq!(market.balance(&market.base_vault), 0);
    assert_eq!(market.balance(&market.quote_vault), 0);
    let operator_base = market.operator_base;
    let operator_quote = market.operator_quote;
    assert_eq!(market.balance(&operator_base), 10_000 * ONE_NVDAX);
    assert_eq!(market.balance(&operator_quote), 10_000_000 * ONE_USDC);

    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::InsufficientInventory,
    );
}

/// Maria withdraws every token, then closes the market. The market account
/// and both vaults are gone, and the three rents she paid at
/// `initialize_market` come back to her to the lamport.
#[test]
fn test_close_market_returns_all_three_rents() {
    let mut market = Market::default_market();
    market
        .withdraw_inventory(1_000 * ONE_NVDAX, 200_000 * ONE_USDC)
        .unwrap();

    let operator = market.operator.pubkey();
    let operator_before = market.lamports(&operator);
    let rents = market.lamports(&market.market)
        + market.lamports(&market.base_vault)
        + market.lamports(&market.quote_vault);
    assert!(rents > 0);

    market.close_market().unwrap();

    assert_eq!(market.lamports(&operator), operator_before + rents);
    assert!(market.svm.get_account(&market.market).is_none());
    assert!(market.svm.get_account(&market.base_vault).is_none());
    assert!(market.svm.get_account(&market.quote_vault).is_none());
    // The inventory went back through `withdraw_inventory`, so the operator
    // holds every token it minted.
    let operator_base = market.operator_base;
    let operator_quote = market.operator_quote;
    assert_eq!(market.balance(&operator_base), 10_000 * ONE_NVDAX);
    assert_eq!(market.balance(&operator_quote), 10_000_000 * ONE_USDC);
}

/// The market cannot close while either vault holds a single minor unit: the
/// operator withdraws first. Each vault's check is exercised on its own.
#[test]
fn test_close_market_refuses_while_a_vault_holds_tokens() {
    let mut market = Market::default_market();
    assert_fails_with(market.close_market(), PropAmmError::InventoryNotEmpty);

    // Each retry is otherwise byte-identical to the refused close, so it would
    // carry the same signature and be dropped as already processed; a fresh
    // blockhash gives it a new one.

    // Base vault empty, quote vault still stocked.
    market.withdraw_inventory(1_000 * ONE_NVDAX, 0).unwrap();
    market.svm.expire_blockhash();
    assert_fails_with(market.close_market(), PropAmmError::InventoryNotEmpty);

    // Quote vault empty, one minor unit of base back in the base vault.
    market.withdraw_inventory(0, 200_000 * ONE_USDC).unwrap();
    market.deposit_inventory(1, 0).unwrap();
    market.svm.expire_blockhash();
    assert_fails_with(market.close_market(), PropAmmError::InventoryNotEmpty);
    assert!(market.svm.get_account(&market.market).is_some());

    market.withdraw_inventory(1, 0).unwrap();
    market.svm.expire_blockhash();
    market.close_market().expect("an empty market must close");
}

/// Nobody can wedge the close or slip tokens past it by sending them straight
/// to a vault. After Maria withdraws everything, a stranger sends one minor
/// unit of NVDAx into the base vault with a plain token transfer, not
/// `deposit_inventory`, and the close is refused; then one minor unit of USDC
/// into the quote vault, and the close is refused again. Maria withdraws each
/// donation like any other inventory and the market closes.
#[test]
fn test_close_market_refuses_tokens_sent_straight_to_a_vault() {
    let mut market = Market::default_market();
    market
        .withdraw_inventory(1_000 * ONE_NVDAX, 200_000 * ONE_USDC)
        .unwrap();
    let (stranger, stranger_base, stranger_quote) = market.funded_trader(1, 1);

    let base_mint = market.base_mint;
    let base_vault = market.base_vault;
    market.donate_to_vault(
        &stranger,
        &stranger_base,
        &base_mint,
        NVDAX_DECIMALS,
        &base_vault,
        1,
    );
    assert_eq!(market.balance(&base_vault), 1);
    assert_fails_with(market.close_market(), PropAmmError::InventoryNotEmpty);
    assert!(market.svm.get_account(&market.market).is_some());
    market.withdraw_inventory(1, 0).unwrap();

    let quote_mint = market.quote_mint;
    let quote_vault = market.quote_vault;
    market.donate_to_vault(
        &stranger,
        &stranger_quote,
        &quote_mint,
        USDC_DECIMALS,
        &quote_vault,
        1,
    );
    assert_eq!(market.balance(&quote_vault), 1);
    // The retry is otherwise byte-identical to the refused close, so a fresh
    // blockhash gives it a new signature.
    market.svm.expire_blockhash();
    assert_fails_with(market.close_market(), PropAmmError::InventoryNotEmpty);
    assert!(market.svm.get_account(&market.market).is_some());
    market.withdraw_inventory(0, 1).unwrap();

    market.svm.expire_blockhash();
    market.close_market().expect("an empty market must close");
    assert!(market.svm.get_account(&market.market).is_none());
    // The operator now holds its own inventory plus both donated units.
    let operator_base = market.operator_base;
    let operator_quote = market.operator_quote;
    assert_eq!(market.balance(&operator_base), 10_000 * ONE_NVDAX + 1);
    assert_eq!(market.balance(&operator_quote), 10_000_000 * ONE_USDC + 1);
}

/// A closed market cannot fill. Its account is gone, so a swap naming it
/// fails before any token moves, and the trader keeps every token.
#[test]
fn test_swap_against_a_closed_market_fails() {
    let mut market = Market::default_market();
    market
        .withdraw_inventory(1_000 * ONE_NVDAX, 200_000 * ONE_USDC)
        .unwrap();
    market.close_market().unwrap();

    let (alice, alice_base, alice_quote) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);
    assert_fails_with_anchor_error(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        AnchorErrorCode::AccountNotInitialized,
    );
    assert_eq!(market.balance(&alice_base), 0);
    assert_eq!(market.balance(&alice_quote), FIVE_NVDAX_AT_THE_ASK);
}

#[test]
fn test_close_market_rejects_non_operator() {
    let mut market = Market::default_market();
    market
        .withdraw_inventory(1_000 * ONE_NVDAX, 200_000 * ONE_USDC)
        .unwrap();
    let (mallory, _, _) = market.funded_trader(0, 0);
    assert_fails_with_anchor_error(
        market.close_market_as(&mallory),
        AnchorErrorCode::ConstraintHasOne,
    );
    assert!(market.svm.get_account(&market.market).is_some());
}

#[test]
fn test_withdraw_more_than_inventory_fails() {
    let mut market = Market::default_market();
    assert_fails_with(
        market.withdraw_inventory(1_001 * ONE_NVDAX, 0),
        PropAmmError::InsufficientInventory,
    );
}

#[test]
fn test_deposit_inventory_rejects_non_operator() {
    let mut market = Market::default_market();
    let (mallory, _, _) = market.funded_trader(ONE_NVDAX, ONE_USDC);
    assert_fails_with_anchor_error(
        market.move_inventory_as(&mallory, true, ONE_NVDAX, 0),
        AnchorErrorCode::ConstraintHasOne,
    );
}

#[test]
fn test_withdraw_inventory_rejects_non_operator() {
    let mut market = Market::default_market();
    let (mallory, _, _) = market.funded_trader(0, 0);
    assert_fails_with_anchor_error(
        market.move_inventory_as(&mallory, false, ONE_NVDAX, 0),
        AnchorErrorCode::ConstraintHasOne,
    );
}

#[test]
fn test_set_quote_rejects_non_operator() {
    let mut market = Market::default_market();
    let (mallory, _, _) = market.funded_trader(0, 0);
    assert_fails_with_anchor_error(
        market.set_quote_as(&mallory, 500, true),
        AnchorErrorCode::ConstraintHasOne,
    );
}

// ===========================================================================
// Swap rejections: every gate has a test that proves it shuts
// ===========================================================================

/// A fill below the caller's minimum is rejected, not filled worse.
#[test]
fn test_swap_rejects_slippage() {
    let mut market = Market::default_market();
    let quote_in = FIVE_NVDAX_AT_THE_ASK;
    let (alice, _, _) = market.funded_trader(0, quote_in);
    // The fill would be exactly 5 NVDAx; demand one minor unit more.
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, quote_in, FIVE_NVDAX + 1),
        PropAmmError::SlippageExceeded,
    );
}

/// An oracle price older than the staleness bound cannot be traded against.
/// A lagging quote is a free option for arbitrageurs, so the market refuses to
/// quote at all rather than quote wrong.
#[test]
fn test_swap_rejects_stale_price() {
    let mut market = Market::default_market();
    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);
    // The feed was last updated at the current slot; 200 slots later it is
    // stale (the bound is 150 slots). Warp relative to the current slot:
    // LiteSVM starts the clock at a mainnet-like slot, not at zero.
    let published_at = market.current_slot();
    market.warp(published_at + 200);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::StalePrice,
    );
}

/// A cluster restart passes hours of wall-clock time in zero slots, so a price
/// published before the halt can still look fresh by slot count. The market
/// must refuse to quote against it until the publisher posts again.
#[test]
fn test_swap_rejects_price_from_before_a_restart() {
    let mut market = Market::default_market();
    market.set_price(dollars(165));
    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);

    // Simulate a halt: the cluster restarts a few slots after the price was
    // published, well inside the 150-slot staleness bound, so only the
    // restart check can catch the pre-halt price.
    let published_at = market.current_slot();
    market.warp(published_at + 5);
    market.set_last_restart_slot(published_at + 3);

    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::PricePredatesRestart,
    );

    // Publishing after the restart reopens the market. Warp first: the retry is
    // otherwise byte-identical to the rejected swap, so it would carry the same
    // signature and be dropped as already processed.
    market.warp(published_at + 6);
    market.set_price(dollars(165));
    market
        .swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0)
        .expect("a freshly published price must be accepted after a restart");
}

/// The market records the program that owns its feed at creation and refuses
/// a price from a feed account owned by any other program, however well its
/// bytes decode. The feed is swapped for a byte-identical copy owned by an
/// unrelated program, and the refusal is by the owner alone: the same bytes
/// owned by the mock oracle program again are accepted.
#[test]
fn test_swap_rejects_price_feed_from_another_program() {
    let mut market = Market::default_market();
    assert_eq!(
        market.market_state().price_feed_program,
        mock_price_feed::id()
    );
    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);

    market.set_feed_owner(Pubkey::new_unique());
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::PriceFeedNotFromOracle,
    );

    // The retry is otherwise byte-identical to the rejected swap, so it would
    // carry the same signature and be dropped as already processed.
    market.svm.expire_blockhash();
    market.set_feed_owner(mock_price_feed::id());
    market
        .swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0)
        .expect("the same feed owned by the recorded oracle program must be accepted");
}

/// A price the oracle itself is unsure about is rejected: the confidence band
/// (about 1.2% here) exceeds the market's 1% limit.
#[test]
fn test_swap_rejects_wide_confidence() {
    let mut market = Market::default_market();
    market.set_price_with_confidence(dollars(165), 200_000_000);
    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::OracleConfidenceTooWide,
    );
}

/// A zero or negative oracle price is not a price. The market refuses to
/// quote against either rather than divide by it or flip the spread.
#[test]
fn test_swap_rejects_non_positive_price() {
    let mut market = Market::default_market();
    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);

    market.set_price(0);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::NonPositivePrice,
    );

    // The retry is otherwise byte-identical to the rejected swap, so it would
    // carry the same signature and be dropped as already processed.
    market.svm.expire_blockhash();
    market.set_price(-dollars(165));
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::NonPositivePrice,
    );
}

/// A market created for a feed with 6 decimals of scale refuses a feed that
/// reports 8: read at the wrong scale, $165 would be $16,500.
#[test]
fn test_swap_rejects_oracle_scale_mismatch() {
    let parameters = MarketParameters {
        oracle_scale: ORACLE_SCALE - 2,
        spread_bps: SPREAD_BPS,
        max_confidence_bps: MAX_CONFIDENCE_BPS,
    };
    let mut market = Market::try_new(dollars(165), parameters).unwrap();
    market
        .deposit_inventory(1_000 * ONE_NVDAX, 200_000 * ONE_USDC)
        .unwrap();
    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::OracleScaleMismatch,
    );
}

/// A feed account owned by the recorded oracle program but too short to hold
/// the price layout is refused before a byte of it is decoded.
#[test]
fn test_swap_rejects_oracle_data_too_short() {
    let mut market = Market::default_market();
    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);
    // The layout needs 76 bytes; keep the discriminator, authority and price.
    market.truncate_feed(56);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::OracleDataTooShort,
    );
}

/// One minor unit of USDC (0.000001) at the $165.165 ask buys 0.0000000060546
/// NVDAx, which floors to zero minor units. The market refuses rather than
/// take the trader's input for nothing.
#[test]
fn test_swap_rejects_amount_that_rounds_to_zero() {
    let mut market = Market::default_market();
    let (alice, _, alice_quote) = market.funded_trader(0, 1);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, 1, 0),
        PropAmmError::AmountRoundsToZero,
    );
    assert_eq!(market.balance(&alice_quote), 1);
}

/// While the operator has pulled its quotes, nobody can swap.
#[test]
fn test_swap_rejects_when_paused() {
    let mut market = Market::default_market();
    market.set_quote(SPREAD_BPS, true).unwrap();
    let (alice, _, _) = market.funded_trader(0, FIVE_NVDAX_AT_THE_ASK);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, FIVE_NVDAX_AT_THE_ASK, 0),
        PropAmmError::MarketPaused,
    );

    // Unpausing restores the exact same quote.
    market.set_quote(SPREAD_BPS, false).unwrap();
    market
        .swap(
            &alice,
            Direction::BuyBase,
            FIVE_NVDAX_AT_THE_ASK,
            FIVE_NVDAX,
        )
        .unwrap();
}

#[test]
fn test_swap_rejects_zero_amount() {
    let mut market = Market::default_market();
    let (alice, _, _) = market.funded_trader(0, ONE_USDC);
    assert_fails_with(
        market.swap(&alice, Direction::BuyBase, 0, 0),
        PropAmmError::ZeroAmount,
    );
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
    assert_fails_with(
        market.swap(&whale, Direction::BuyBase, quote_in, 0),
        PropAmmError::InsufficientInventory,
    );
}

// ===========================================================================
// Parameter validation
// ===========================================================================

/// The market account is the token authority of both vaults, so it can sign
/// their outgoing transfers with its own seeds.
#[test]
fn test_market_owns_both_vaults() {
    let market = Market::default_market();
    assert_eq!(
        market.token_account_owner(&market.base_vault),
        market.market
    );
    assert_eq!(
        market.token_account_owner(&market.quote_vault),
        market.market
    );
}

#[test]
fn test_initialize_market_rejects_zero_spread() {
    let parameters = MarketParameters {
        oracle_scale: ORACLE_SCALE,
        spread_bps: 0,
        max_confidence_bps: MAX_CONFIDENCE_BPS,
    };
    assert_fails_with(
        Market::try_new(dollars(165), parameters),
        PropAmmError::InvalidParameter,
    );
}

#[test]
fn test_initialize_market_rejects_full_spread() {
    let parameters = MarketParameters {
        oracle_scale: ORACLE_SCALE,
        spread_bps: 10_000,
        max_confidence_bps: MAX_CONFIDENCE_BPS,
    };
    assert_fails_with(
        Market::try_new(dollars(165), parameters),
        PropAmmError::InvalidParameter,
    );
}

#[test]
fn test_set_quote_rejects_invalid_spread() {
    let mut market = Market::default_market();
    assert_fails_with(market.set_quote(0, false), PropAmmError::InvalidParameter);
    assert_fails_with(
        market.set_quote(10_000, false),
        PropAmmError::InvalidParameter,
    );
}
