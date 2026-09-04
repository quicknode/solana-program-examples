use {
    anchor_lang::{
        solana_program::{clock::Clock, instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    options::{
        instructions::write_option::OptionTerms,
        state::{Market as MarketState, OptionContract, OptionKind, OptionStatus},
    },
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
    solana_signer::Signer,
};

// Both tokens have 6 decimals: the underlying is NVDAx (tokenized NVIDIA
// stock) and the quote is USDC, so one whole unit of either is 1_000_000
// minor units.
const ONE_TOKEN: u64 = 1_000_000;
const DECIMALS: u8 = 6;

// The venue charges 1% of every premium.
const FEE_BPS: u16 = 100;

// The walkthrough's call: 5 contracts, each on 1 NVDAx, strike 180 USDC,
// asking 25 USDC for the lot, expiring a week out.
const CONTRACTS: u64 = 5;
const ONE_NVDAX_PER_CONTRACT: u64 = ONE_TOKEN;
const CALL_STRIKE: u64 = 180 * ONE_TOKEN;
const CALL_PREMIUM: u64 = 25 * ONE_TOKEN;
// And the put: 5 contracts, strike 150 USDC, asking 20 USDC.
const PUT_STRIKE: u64 = 150 * ONE_TOKEN;
const PUT_PREMIUM: u64 = 20 * ONE_TOKEN;

const SECONDS_PER_DAY: i64 = 24 * 60 * 60;
const ONE_WEEK: i64 = 7 * SECONDS_PER_DAY;

// Every character starts with the standard wallet of 1,000 USDC; the story
// hands the writers 5 NVDAx.
const STANDARD_USDC: u64 = 1_000 * ONE_TOKEN;
const FIVE_NVDAX: u64 = 5 * ONE_TOKEN;

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

/// The walkthrough's call, expiring at `expiry`.
fn call_terms(expiry: i64) -> OptionTerms {
    OptionTerms {
        kind: OptionKind::Call,
        contracts: CONTRACTS,
        underlying_per_contract: ONE_NVDAX_PER_CONTRACT,
        strike_per_contract: CALL_STRIKE,
        premium: CALL_PREMIUM,
        expiry,
    }
}

/// One deployed venue plus the keys needed to drive it.
struct Venue {
    svm: LiteSVM,
    payer: Keypair,
    admin: Keypair,
    underlying_mint: Pubkey,
    quote_mint: Pubkey,
    market: Pubkey,
    market_authority: Pubkey,
    underlying_vault: Pubkey,
    quote_vault: Pubkey,
}

/// A character with a wallet and both token accounts.
struct Person {
    keypair: Keypair,
    underlying: Pubkey,
    quote: Pubkey,
}

impl Person {
    fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }
}

impl Venue {
    fn new() -> Venue {
        Venue::try_new(FEE_BPS, false).expect("market initialization should succeed")
    }

    /// Like `new`, but surfaces an `initialize_market` rejection instead of
    /// panicking, so tests can probe the parameter validation. `same_mint`
    /// passes the underlying mint as the quote mint too.
    fn try_new(fee_bps: u16, same_mint: bool) -> Result<Venue, ()> {
        let mut svm = LiteSVM::new();
        svm.add_program(
            options::id(),
            include_bytes!("../../../target/deploy/options.so"),
        )
        .unwrap();

        let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let admin = create_wallet(&mut svm, 100_000_000_000).unwrap();
        let underlying_mint = create_token_mint(&mut svm, &admin, DECIMALS, None).unwrap();
        let quote_mint = if same_mint {
            underlying_mint
        } else {
            create_token_mint(&mut svm, &admin, DECIMALS, None).unwrap()
        };

        let market = Pubkey::find_program_address(
            &[b"market", underlying_mint.as_ref(), quote_mint.as_ref()],
            &options::id(),
        )
        .0;
        let market_authority =
            Pubkey::find_program_address(&[b"authority", market.as_ref()], &options::id()).0;
        let underlying_vault =
            Pubkey::find_program_address(&[b"underlying_vault", market.as_ref()], &options::id()).0;
        let quote_vault =
            Pubkey::find_program_address(&[b"quote_vault", market.as_ref()], &options::id()).0;

        let initialize_market = Instruction::new_with_bytes(
            options::id(),
            &options::instruction::InitializeMarket { fee_bps }.data(),
            options::accounts::InitializeMarketAccountConstraints {
                admin: admin.pubkey(),
                market,
                underlying_mint,
                quote_mint,
                market_authority,
                underlying_vault,
                quote_vault,
                token_program: token_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(
            &mut svm,
            vec![initialize_market],
            &[&admin],
            &admin.pubkey(),
        )
        .map_err(|_| ())?;

        Ok(Venue {
            svm,
            payer,
            admin,
            underlying_mint,
            quote_mint,
            market,
            market_authority,
            underlying_vault,
            quote_vault,
        })
    }

    fn market_state(&self) -> MarketState {
        let account = self.svm.get_account(&self.market).unwrap();
        MarketState::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    fn option_pda(&self, writer: &Pubkey, id: u64) -> Pubkey {
        Pubkey::find_program_address(
            &[
                b"option",
                self.market.as_ref(),
                writer.as_ref(),
                &id.to_le_bytes(),
            ],
            &options::id(),
        )
        .0
    }

    fn option_state(&self, option: &Pubkey) -> OptionContract {
        let account = self.svm.get_account(option).unwrap();
        OptionContract::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    fn option_exists(&self, option: &Pubkey) -> bool {
        self.svm
            .get_account(option)
            .map(|account| !account.data.is_empty())
            .unwrap_or(false)
    }

    fn now(&self) -> i64 {
        self.svm.get_sysvar::<Clock>().unix_timestamp
    }

    /// Move the clock to `unix_timestamp`. Also expires the blockhash, so a
    /// retried instruction after the warp is not dropped as a duplicate.
    fn warp_to(&mut self, unix_timestamp: i64) {
        let mut clock: Clock = self.svm.get_sysvar();
        clock.unix_timestamp = unix_timestamp;
        self.svm.set_sysvar(&clock);
        self.svm.expire_blockhash();
    }

    /// A character holding `underlying` and `quote` minor units in their
    /// associated token accounts. Every character gets one SOL's worth of
    /// lamports and change for rent and fees.
    fn person(&mut self, underlying: u64, quote: u64) -> Person {
        let keypair = create_wallet(&mut self.svm, 10_000_000_000).unwrap();
        let underlying_account = create_associated_token_account(
            &mut self.svm,
            &keypair.pubkey(),
            &self.underlying_mint,
            &self.payer,
        )
        .unwrap();
        let quote_account = create_associated_token_account(
            &mut self.svm,
            &keypair.pubkey(),
            &self.quote_mint,
            &self.payer,
        )
        .unwrap();
        if underlying > 0 {
            mint_tokens_to_token_account(
                &mut self.svm,
                &self.underlying_mint,
                &underlying_account,
                underlying,
                &self.admin,
            )
            .unwrap();
        }
        if quote > 0 {
            mint_tokens_to_token_account(
                &mut self.svm,
                &self.quote_mint,
                &quote_account,
                quote,
                &self.admin,
            )
            .unwrap();
        }
        Person {
            keypair,
            underlying: underlying_account,
            quote: quote_account,
        }
    }

    fn balance(&self, token_account: &Pubkey) -> u64 {
        get_token_account_balance(&self.svm, token_account).unwrap()
    }

    fn send(&mut self, instruction: Instruction, signer: &Keypair) -> Result<(), ()> {
        send_transaction_from_instructions(
            &mut self.svm,
            vec![instruction],
            &[signer],
            &signer.pubkey(),
        )
        .map(|_| ())
        .map_err(|_| ())
    }

    fn write_option(&mut self, writer: &Person, id: u64, terms: OptionTerms) -> Result<Pubkey, ()> {
        let option = self.option_pda(&writer.pubkey(), id);
        let instruction = Instruction::new_with_bytes(
            options::id(),
            &options::instruction::WriteOption { id, terms }.data(),
            options::accounts::WriteOptionAccountConstraints {
                writer: writer.pubkey(),
                market: self.market,
                option,
                underlying_mint: self.underlying_mint,
                quote_mint: self.quote_mint,
                underlying_vault: self.underlying_vault,
                quote_vault: self.quote_vault,
                writer_underlying: writer.underlying,
                writer_quote: writer.quote,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        self.send(instruction, &writer.keypair).map(|_| option)
    }

    /// The walkthrough's call, written by `writer`, expiring one week out.
    fn write_call(&mut self, writer: &Person) -> Pubkey {
        let expiry = self.now() + ONE_WEEK;
        self.write_option(writer, 1, call_terms(expiry))
            .expect("writing the call should succeed")
    }

    /// The walkthrough's put, written by `writer`, expiring one week out.
    fn write_put(&mut self, writer: &Person) -> Pubkey {
        let expiry = self.now() + ONE_WEEK;
        self.write_option(
            writer,
            2,
            OptionTerms {
                kind: OptionKind::Put,
                contracts: CONTRACTS,
                underlying_per_contract: ONE_NVDAX_PER_CONTRACT,
                strike_per_contract: PUT_STRIKE,
                premium: PUT_PREMIUM,
                expiry,
            },
        )
        .expect("writing the put should succeed")
    }

    fn buy_option(&mut self, buyer: &Person, writer: &Pubkey, option: &Pubkey) -> Result<(), ()> {
        let instruction = Instruction::new_with_bytes(
            options::id(),
            &options::instruction::BuyOption {}.data(),
            options::accounts::BuyOptionAccountConstraints {
                buyer: buyer.pubkey(),
                writer: *writer,
                market: self.market,
                option: *option,
                quote_mint: self.quote_mint,
                quote_vault: self.quote_vault,
                buyer_quote: buyer.quote,
                writer_quote: derive_ata(writer, &self.quote_mint),
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        self.send(instruction, &buyer.keypair)
    }

    fn cancel_option(&mut self, writer: &Person, option: &Pubkey) -> Result<(), ()> {
        let instruction = Instruction::new_with_bytes(
            options::id(),
            &options::instruction::CancelOption {}.data(),
            options::accounts::CancelOptionAccountConstraints {
                writer: writer.pubkey(),
                market: self.market,
                option: *option,
                market_authority: self.market_authority,
                underlying_mint: self.underlying_mint,
                quote_mint: self.quote_mint,
                underlying_vault: self.underlying_vault,
                quote_vault: self.quote_vault,
                writer_underlying: writer.underlying,
                writer_quote: writer.quote,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        self.send(instruction, &writer.keypair)
    }

    fn exercise_option(
        &mut self,
        holder: &Person,
        writer: &Pubkey,
        option: &Pubkey,
    ) -> Result<(), ()> {
        let instruction = Instruction::new_with_bytes(
            options::id(),
            &options::instruction::ExerciseOption {}.data(),
            options::accounts::ExerciseOptionAccountConstraints {
                holder: holder.pubkey(),
                writer: *writer,
                market: self.market,
                option: *option,
                market_authority: self.market_authority,
                underlying_mint: self.underlying_mint,
                quote_mint: self.quote_mint,
                underlying_vault: self.underlying_vault,
                quote_vault: self.quote_vault,
                holder_underlying: holder.underlying,
                holder_quote: holder.quote,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        self.send(instruction, &holder.keypair)
    }

    fn collect_proceeds(&mut self, writer: &Person, option: &Pubkey) -> Result<(), ()> {
        let instruction = Instruction::new_with_bytes(
            options::id(),
            &options::instruction::CollectProceeds {}.data(),
            options::accounts::CollectProceedsAccountConstraints {
                writer: writer.pubkey(),
                market: self.market,
                option: *option,
                market_authority: self.market_authority,
                underlying_mint: self.underlying_mint,
                quote_mint: self.quote_mint,
                underlying_vault: self.underlying_vault,
                quote_vault: self.quote_vault,
                writer_underlying: writer.underlying,
                writer_quote: writer.quote,
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        self.send(instruction, &writer.keypair)
    }

    fn reclaim_collateral(&mut self, writer: &Person, option: &Pubkey) -> Result<(), ()> {
        let instruction = Instruction::new_with_bytes(
            options::id(),
            &options::instruction::ReclaimCollateral {}.data(),
            options::accounts::ReclaimCollateralAccountConstraints {
                writer: writer.pubkey(),
                market: self.market,
                option: *option,
                market_authority: self.market_authority,
                underlying_mint: self.underlying_mint,
                quote_mint: self.quote_mint,
                underlying_vault: self.underlying_vault,
                quote_vault: self.quote_vault,
                writer_underlying: writer.underlying,
                writer_quote: writer.quote,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        self.send(instruction, &writer.keypair)
    }

    fn collect_fees_as(&mut self, signer: &Keypair) -> Result<(), ()> {
        let instruction = Instruction::new_with_bytes(
            options::id(),
            &options::instruction::CollectFees {}.data(),
            options::accounts::CollectFeesAccountConstraints {
                admin: signer.pubkey(),
                market: self.market,
                market_authority: self.market_authority,
                quote_mint: self.quote_mint,
                underlying_vault: self.underlying_vault,
                quote_vault: self.quote_vault,
                admin_quote: derive_ata(&signer.pubkey(), &self.quote_mint),
                token_program: token_program_id(),
                associated_token_program: ata_program_id(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
        );
        self.send(instruction, signer)
    }

    fn collect_fees(&mut self) -> Result<(), ()> {
        let admin = self.admin.insecure_clone();
        self.collect_fees_as(&admin)
    }

    /// The custody invariant: each vault holds exactly what the market owes.
    /// Nothing in these tests donates to a vault, so equality holds rather
    /// than the `>=` the program enforces.
    fn assert_vaults_match_ledger(&self) {
        let market = self.market_state();
        assert_eq!(
            self.balance(&self.underlying_vault),
            market.underlying_locked,
            "underlying vault must hold exactly the locked underlying"
        );
        assert_eq!(
            self.balance(&self.quote_vault),
            market.quote_locked + market.fees_owed,
            "quote vault must hold exactly the locked quote plus the fees owed"
        );
    }
}

// ===========================================================================
// The call: write, buy, exercise, collect
// ===========================================================================

/// Alice writes 5 covered calls on her 5 NVDAx. The whole 5 NVDAx moves into
/// the vault at once; the lot is listed for a 25 USDC premium.
#[test]
fn test_write_call_locks_the_underlying() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);

    let option = venue.write_call(&alice);

    assert_eq!(venue.balance(&alice.underlying), 0);
    assert_eq!(venue.balance(&venue.underlying_vault), FIVE_NVDAX);
    let state = venue.option_state(&option);
    assert_eq!(state.writer, alice.pubkey());
    assert_eq!(state.holder, Pubkey::default());
    assert_eq!(state.kind, OptionKind::Call);
    assert_eq!(state.status, OptionStatus::Listed);
    assert_eq!(state.contracts, CONTRACTS);
    assert_eq!(state.strike_per_contract, CALL_STRIKE);
    assert_eq!(state.premium, CALL_PREMIUM);
    assert_eq!(venue.market_state().underlying_locked, FIVE_NVDAX);
    venue.assert_vaults_match_ledger();
}

/// Bob buys the lot. He pays 25 USDC: 1% (0.25 USDC) to the venue, the rest
/// straight to Alice. The 5 NVDAx do not move.
#[test]
fn test_buy_option_pays_the_premium_minus_the_fee() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);

    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();

    let fee = 250_000; // 0.25 USDC
    assert_eq!(venue.balance(&bob.quote), STANDARD_USDC - CALL_PREMIUM);
    assert_eq!(
        venue.balance(&alice.quote),
        STANDARD_USDC + CALL_PREMIUM - fee
    );
    assert_eq!(venue.balance(&venue.quote_vault), fee);
    assert_eq!(venue.balance(&venue.underlying_vault), FIVE_NVDAX);
    let state = venue.option_state(&option);
    assert_eq!(state.holder, bob.pubkey());
    assert_eq!(state.status, OptionStatus::Held);
    assert_eq!(venue.market_state().fees_owed, fee);
    venue.assert_vaults_match_ledger();
}

/// NVIDIA rallies past the strike offchain, so Bob exercises: he pays the
/// strike, 5 x 180 = 900 USDC, into the vault and takes the 5 NVDAx. The 900
/// USDC is now owed to Alice, and the underlying is no longer owed to anyone.
#[test]
fn test_exercise_call_swaps_the_strike_for_the_underlying() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();

    venue
        .exercise_option(&bob, &alice.pubkey(), &option)
        .unwrap();

    let strike_total = 900 * ONE_TOKEN;
    assert_eq!(venue.balance(&bob.underlying), FIVE_NVDAX);
    assert_eq!(
        venue.balance(&bob.quote),
        STANDARD_USDC - CALL_PREMIUM - strike_total
    );
    assert_eq!(venue.balance(&venue.underlying_vault), 0);
    assert_eq!(venue.balance(&venue.quote_vault), strike_total + 250_000);
    let market = venue.market_state();
    assert_eq!(market.underlying_locked, 0);
    assert_eq!(market.quote_locked, strike_total);
    assert_eq!(venue.option_state(&option).status, OptionStatus::Exercised);
    venue.assert_vaults_match_ledger();
}

/// Alice collects the 900 USDC Bob paid, and the option account closes with
/// its rent back to her. She sold 5 NVDAx for 900 USDC plus the 24.75 USDC
/// premium she already had.
#[test]
fn test_collect_proceeds_pays_the_writer_and_closes_the_option() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();
    venue
        .exercise_option(&bob, &alice.pubkey(), &option)
        .unwrap();
    let alice_lamports_before = venue.svm.get_balance(&alice.pubkey()).unwrap();

    venue.collect_proceeds(&alice, &option).unwrap();

    assert_eq!(
        venue.balance(&alice.quote),
        STANDARD_USDC + 900 * ONE_TOKEN + CALL_PREMIUM - 250_000
    );
    assert_eq!(venue.balance(&alice.underlying), 0);
    assert!(!venue.option_exists(&option));
    assert!(
        venue.svm.get_balance(&alice.pubkey()).unwrap() > alice_lamports_before,
        "the option account's rent must return to the writer"
    );
    let market = venue.market_state();
    assert_eq!(market.quote_locked, 0);
    assert_eq!(market.fees_owed, 250_000);
    venue.assert_vaults_match_ledger();
}

/// Maria sweeps the venue's fee. Only the 0.25 USDC of fees leaves the
/// vault; the strike payment sitting beside it stays locked to Alice.
#[test]
fn test_collect_fees_pays_only_the_fees_owed() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();
    venue
        .exercise_option(&bob, &alice.pubkey(), &option)
        .unwrap();

    venue.collect_fees().unwrap();

    let admin_quote = derive_ata(&venue.admin.pubkey(), &venue.quote_mint);
    assert_eq!(venue.balance(&admin_quote), 250_000);
    assert_eq!(venue.balance(&venue.quote_vault), 900 * ONE_TOKEN);
    assert_eq!(venue.market_state().fees_owed, 0);
    venue.assert_vaults_match_ledger();

    // Nothing left to sweep.
    assert!(venue.collect_fees().is_err());
}

// ===========================================================================
// The put, and the option that expires unexercised
// ===========================================================================

/// Carol writes 5 cash-secured puts at a 150 strike: 750 USDC of collateral.
/// Dave buys them for 20 USDC. NVIDIA falls below the strike offchain, so
/// Dave delivers his 5 NVDAx and takes the 750 USDC; Carol collects the
/// shares. Every amount is a product of two of the option's integers.
#[test]
fn test_put_lifecycle_delivers_the_underlying_for_the_strike() {
    let mut venue = Venue::new();
    let carol = venue.person(0, STANDARD_USDC);
    let dave = venue.person(FIVE_NVDAX, STANDARD_USDC);

    let option = venue.write_put(&carol);
    let collateral = 750 * ONE_TOKEN;
    assert_eq!(venue.balance(&carol.quote), STANDARD_USDC - collateral);
    assert_eq!(venue.balance(&venue.quote_vault), collateral);
    assert_eq!(venue.market_state().quote_locked, collateral);
    venue.assert_vaults_match_ledger();

    venue.buy_option(&dave, &carol.pubkey(), &option).unwrap();
    let fee = 200_000; // 1% of 20 USDC
    assert_eq!(
        venue.balance(&carol.quote),
        STANDARD_USDC - collateral + PUT_PREMIUM - fee
    );
    assert_eq!(venue.balance(&dave.quote), STANDARD_USDC - PUT_PREMIUM);
    venue.assert_vaults_match_ledger();

    venue
        .exercise_option(&dave, &carol.pubkey(), &option)
        .unwrap();
    assert_eq!(venue.balance(&dave.underlying), 0);
    assert_eq!(
        venue.balance(&dave.quote),
        STANDARD_USDC - PUT_PREMIUM + collateral
    );
    assert_eq!(venue.balance(&venue.underlying_vault), FIVE_NVDAX);
    assert_eq!(venue.balance(&venue.quote_vault), fee);
    let market = venue.market_state();
    assert_eq!(market.underlying_locked, FIVE_NVDAX);
    assert_eq!(market.quote_locked, 0);
    venue.assert_vaults_match_ledger();

    venue.collect_proceeds(&carol, &option).unwrap();
    assert_eq!(venue.balance(&carol.underlying), FIVE_NVDAX);
    assert_eq!(venue.market_state().underlying_locked, 0);
    assert!(!venue.option_exists(&option));
    venue.assert_vaults_match_ledger();
}

/// Bob never exercises. Once the expiry passes, Alice takes her 5 NVDAx back
/// and keeps the premium: the writer's whole return on an option that expires
/// out of the money. Bob is left with nothing to claim.
#[test]
fn test_reclaim_collateral_after_expiry_returns_it_to_the_writer() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();
    let expiry = venue.option_state(&option).expiry;

    venue.warp_to(expiry);
    venue.reclaim_collateral(&alice, &option).unwrap();

    assert_eq!(venue.balance(&alice.underlying), FIVE_NVDAX);
    assert_eq!(
        venue.balance(&alice.quote),
        STANDARD_USDC + CALL_PREMIUM - 250_000
    );
    assert_eq!(venue.balance(&bob.quote), STANDARD_USDC - CALL_PREMIUM);
    assert!(!venue.option_exists(&option));
    assert_eq!(venue.market_state().underlying_locked, 0);
    venue.assert_vaults_match_ledger();
}

// ===========================================================================
// The expiry boundary, from both sides
// ===========================================================================

/// The holder may exercise while now < expiry. One second before expiry the
/// exercise goes through; at expiry it is refused.
#[test]
fn test_exercise_is_allowed_up_to_but_not_at_expiry() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();
    let expiry = venue.option_state(&option).expiry;

    venue.warp_to(expiry);
    assert!(venue
        .exercise_option(&bob, &alice.pubkey(), &option)
        .is_err());

    venue.warp_to(expiry - 1);
    venue
        .exercise_option(&bob, &alice.pubkey(), &option)
        .expect("exercise one second before expiry must succeed");
}

/// The writer may reclaim once now >= expiry, and not one second earlier.
#[test]
fn test_reclaim_is_refused_before_expiry() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();
    let expiry = venue.option_state(&option).expiry;

    venue.warp_to(expiry - 1);
    assert!(venue.reclaim_collateral(&alice, &option).is_err());

    venue.warp_to(expiry);
    venue
        .reclaim_collateral(&alice, &option)
        .expect("reclaim at expiry must succeed");
}

/// An expired lot cannot be bought: nobody can pay for a right that can no
/// longer be exercised.
#[test]
fn test_buy_is_refused_after_expiry() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    let expiry = venue.option_state(&option).expiry;

    venue.warp_to(expiry);
    assert!(venue.buy_option(&bob, &alice.pubkey(), &option).is_err());
}

// ===========================================================================
// Cancel: the writer's exit from an unsold lot
// ===========================================================================

/// An unsold lot can be withdrawn at any time, collateral back, account
/// closed. Without this, a lot nobody buys would lock the writer's tokens
/// forever.
#[test]
fn test_cancel_unsold_option_returns_the_collateral() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let option = venue.write_call(&alice);

    venue.cancel_option(&alice, &option).unwrap();

    assert_eq!(venue.balance(&alice.underlying), FIVE_NVDAX);
    assert_eq!(venue.balance(&venue.underlying_vault), 0);
    assert!(!venue.option_exists(&option));
    venue.assert_vaults_match_ledger();
}

/// An unsold lot that expired is still the writer's to cancel: there is no
/// holder whose rights the cancel would cut short.
#[test]
fn test_cancel_unsold_option_works_after_expiry() {
    let mut venue = Venue::new();
    let carol = venue.person(0, STANDARD_USDC);
    let option = venue.write_put(&carol);
    let expiry = venue.option_state(&option).expiry;

    venue.warp_to(expiry + SECONDS_PER_DAY);
    venue.cancel_option(&carol, &option).unwrap();

    assert_eq!(venue.balance(&carol.quote), STANDARD_USDC);
    assert_eq!(venue.market_state().quote_locked, 0);
    venue.assert_vaults_match_ledger();
}

/// Once sold, the collateral belongs to the deal: the writer cannot pull it
/// out from under the holder.
#[test]
fn test_cancel_is_refused_once_sold() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();

    assert!(venue.cancel_option(&alice, &option).is_err());
    assert_eq!(venue.balance(&venue.underlying_vault), FIVE_NVDAX);
}

// ===========================================================================
// Who may do what
// ===========================================================================

#[test]
fn test_buy_is_refused_once_sold() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let carol = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();

    assert!(venue.buy_option(&carol, &alice.pubkey(), &option).is_err());
    assert_eq!(venue.option_state(&option).holder, bob.pubkey());
}

/// A writer cannot buy their own lot: the premium's source and destination
/// would be the same token account in two mutable slots, which the loader
/// rejects before the handler runs.
#[test]
fn test_writer_cannot_buy_their_own_option() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let option = venue.write_call(&alice);

    assert!(venue.buy_option(&alice, &alice.pubkey(), &option).is_err());
    assert_eq!(venue.option_state(&option).status, OptionStatus::Listed);
}

/// Only the holder can exercise: an unsold lot has no holder, and a stranger
/// is not the holder of a sold one.
#[test]
fn test_exercise_is_refused_for_anyone_but_the_holder() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let mallory = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);

    assert!(venue
        .exercise_option(&mallory, &alice.pubkey(), &option)
        .is_err());
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();
    assert!(venue
        .exercise_option(&mallory, &alice.pubkey(), &option)
        .is_err());
    assert_eq!(venue.balance(&venue.underlying_vault), FIVE_NVDAX);
}

/// A sold, unexercised, unexpired lot has no proceeds to collect, and after
/// exercise only the writer may collect them.
#[test]
fn test_collect_proceeds_needs_an_exercised_option_and_the_writer() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let mallory = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();

    assert!(venue.collect_proceeds(&alice, &option).is_err());

    venue
        .exercise_option(&bob, &alice.pubkey(), &option)
        .unwrap();
    assert!(venue.collect_proceeds(&mallory, &option).is_err());
    assert!(venue.collect_proceeds(&bob, &option).is_err());
    assert_eq!(venue.balance(&venue.quote_vault), 900 * ONE_TOKEN + 250_000);
}

/// An exercised lot has no collateral left to reclaim, whatever the clock
/// says: the holder took it.
#[test]
fn test_reclaim_is_refused_after_exercise() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();
    venue
        .exercise_option(&bob, &alice.pubkey(), &option)
        .unwrap();
    let expiry = venue.option_state(&option).expiry;

    venue.warp_to(expiry + 1);
    assert!(venue.reclaim_collateral(&alice, &option).is_err());
}

#[test]
fn test_collect_fees_is_refused_for_anyone_but_the_admin() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let mallory = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);
    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();

    assert!(venue.collect_fees_as(&mallory.keypair).is_err());
    assert_eq!(venue.market_state().fees_owed, 250_000);
}

// ===========================================================================
// Parameter validation
// ===========================================================================

#[test]
fn test_write_option_rejects_zero_quantities_and_a_free_premium() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let expiry = venue.now() + ONE_WEEK;

    let attempts = [
        (0, ONE_NVDAX_PER_CONTRACT, CALL_STRIKE, CALL_PREMIUM),
        (CONTRACTS, 0, CALL_STRIKE, CALL_PREMIUM),
        (CONTRACTS, ONE_NVDAX_PER_CONTRACT, 0, CALL_PREMIUM),
        (CONTRACTS, ONE_NVDAX_PER_CONTRACT, CALL_STRIKE, 0),
    ];
    for (id, (contracts, underlying_per_contract, strike_per_contract, premium)) in
        attempts.into_iter().enumerate()
    {
        let terms = OptionTerms {
            kind: OptionKind::Call,
            contracts,
            underlying_per_contract,
            strike_per_contract,
            premium,
            expiry,
        };
        assert!(
            venue.write_option(&alice, id as u64 + 10, terms).is_err(),
            "a zero in any term must be refused"
        );
    }
    assert_eq!(venue.balance(&alice.underlying), FIVE_NVDAX);
}

/// The holder may exercise while now < expiry, so an expiry at or before now
/// would be a lot nobody could ever exercise.
#[test]
fn test_write_option_rejects_an_expiry_that_has_passed() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let now = venue.now();

    for expiry in [now, now - SECONDS_PER_DAY] {
        assert!(venue.write_option(&alice, 20, call_terms(expiry)).is_err());
    }
}

/// A lot whose collateral would overflow is refused before anyone pays for
/// it, rather than failing at exercise.
#[test]
fn test_write_option_rejects_a_lot_whose_collateral_overflows() {
    let mut venue = Venue::new();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let expiry = venue.now() + ONE_WEEK;

    let terms = OptionTerms {
        contracts: u64::MAX,
        underlying_per_contract: 2,
        ..call_terms(expiry)
    };
    assert!(venue.write_option(&alice, 30, terms).is_err());
}

#[test]
fn test_initialize_market_rejects_a_full_fee() {
    assert!(Venue::try_new(10_000, false).is_err());
}

#[test]
fn test_initialize_market_rejects_the_same_mint_on_both_sides() {
    assert!(Venue::try_new(FEE_BPS, true).is_err());
}

/// A venue run at cost is a valid choice: with a zero fee the writer
/// receives the whole premium and no fee transfer is attempted.
#[test]
fn test_zero_fee_venue_pays_the_writer_the_whole_premium() {
    let mut venue = Venue::try_new(0, false).unwrap();
    let alice = venue.person(FIVE_NVDAX, STANDARD_USDC);
    let bob = venue.person(0, STANDARD_USDC);
    let option = venue.write_call(&alice);

    venue.buy_option(&bob, &alice.pubkey(), &option).unwrap();

    assert_eq!(venue.balance(&alice.quote), STANDARD_USDC + CALL_PREMIUM);
    assert_eq!(venue.market_state().fees_owed, 0);
    venue.assert_vaults_match_ledger();
}
