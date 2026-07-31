#![allow(dead_code)]
//! Shared LiteSVM harness for the lending program tests.
//!
//! Sets up a lending market with reserves, funds users, and exposes one method
//! per protocol action. Actions that read value (deposit/redeem/borrow/withdraw/
//! liquidate) bundle the required `refresh_reserve` / `refresh_obligation`
//! instructions into the same transaction, exactly as a real client must.

use anchor_lang::{
    solana_program::{
        instruction::{AccountMeta, Instruction},
        system_program,
    },
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use anchor_spl::token::ID as TOKEN_PROGRAM_ID;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_kite::{
    create_associated_token_account, create_token_mint, create_wallet, get_token_account_balance,
    mint_tokens_to_token_account, send_transaction_from_instructions,
};
use solana_signer::Signer;

use lending::constants::{
    LENDING_MARKET_SEED, LIQUIDITY_VAULT_SEED, OBLIGATION_SEED, OBLIGATION_SHARE_VAULT_SEED,
    PRICE_FEED_SEED, RESERVE_SEED, SHARE_MINT_SEED,
};
use lending::state::{Obligation, Reserve, ReserveConfig};

pub use anchor_lang::prelude::Pubkey;

/// A FIXED_POINT_SCALE-scaled price exponent: prices are passed as
/// `mantissa * 10^-18`, matching a Switchboard On-Demand feed's 1e18 result.
pub const PRICE_EXPONENT: i32 = -18;

pub fn dollars(whole: u64) -> i128 {
    // price mantissa for `whole` dollars at exponent -18.
    (whole as i128) * 1_000_000_000_000_000_000
}

pub fn cents(amount: u64) -> i128 {
    (amount as i128) * 10_000_000_000_000_000
}

pub fn ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    let ata_program: Pubkey = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap();
    Pubkey::find_program_address(
        &[owner.as_ref(), TOKEN_PROGRAM_ID.as_ref(), mint.as_ref()],
        &ata_program,
    )
    .0
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &lending::id()).0
}

/// Map kite's transaction result to a String so tests can assert on the program
/// error message embedded in failed-transaction logs.
fn send(
    svm: &mut LiteSVM,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
    payer: &Pubkey,
) -> Result<(), String> {
    send_transaction_from_instructions(svm, instructions, signers, payer)
        .map_err(|thrown| format!("{thrown:?}"))
}

/// Handle to one reserve and its associated PDAs.
#[derive(Clone, Copy)]
pub struct ReserveHandle {
    pub mint: Pubkey,
    pub decimals: u8,
    pub reserve: Pubkey,
    pub share_mint: Pubkey,
    pub liquidity_vault: Pubkey,
    pub price_feed: Pubkey,
}

pub struct Env {
    pub svm: LiteSVM,
    /// Market owner; also the mint authority for every test mint and the price
    /// feed authority.
    pub owner: Keypair,
    pub market: Pubkey,
}

impl Env {
    pub fn new() -> Self {
        let mut svm = LiteSVM::new();
        let program_bytes = include_bytes!("../../../../target/deploy/lending.so");
        svm.add_program(lending::id(), program_bytes).unwrap();

        let owner = create_wallet(&mut svm, 1_000_000_000_000).unwrap();
        let quote_mint = create_token_mint(&mut svm, &owner, 6, None).unwrap();
        // The market is seeded by its market_id index alone (no owner). Market 0.
        let market_id: u64 = 0;
        let market = pda(&[LENDING_MARKET_SEED, &market_id.to_le_bytes()]);

        let instruction = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::InitializeLendingMarket {
                lending_market: market,
                owner: owner.pubkey(),
                quote_currency_mint: quote_mint,
                system_program: system_program::id(),
            }
            .to_account_metas(None),
            data: lending::instruction::InitializeLendingMarket { market_id }.data(),
        };
        send(&mut svm, vec![instruction], &[&owner], &owner.pubkey()).unwrap();

        Env { svm, owner, market }
    }

    pub fn current_slot(&self) -> u64 {
        self.svm.get_sysvar::<anchor_lang::solana_program::clock::Clock>().slot
    }

    /// Create a second lending market owned by `market_owner`, for tests that
    /// exercise cross-market isolation.
    pub fn init_market_for(&mut self, market_owner: &Keypair) -> Pubkey {
        let env_owner = self.owner.insecure_clone();
        let quote_mint = create_token_mint(&mut self.svm, &env_owner, 6, None).unwrap();
        // A distinct id from the env's market 0, since the id is the market's
        // global identifier (the owner is not part of the seed).
        let market_id: u64 = 1;
        let market = pda(&[LENDING_MARKET_SEED, &market_id.to_le_bytes()]);
        let instruction = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::InitializeLendingMarket {
                lending_market: market,
                owner: market_owner.pubkey(),
                quote_currency_mint: quote_mint,
                system_program: system_program::id(),
            }
            .to_account_metas(None),
            data: lending::instruction::InitializeLendingMarket { market_id }.data(),
        };
        send(&mut self.svm, vec![instruction], &[market_owner], &market_owner.pubkey()).unwrap();
        market
    }

    /// Add a reserve to a market other than the default one. The mint and price
    /// feed are still created/written by the env owner (a reserve trusts
    /// whichever feed its market owner registers; the writer need not match).
    pub fn add_reserve_to(
        &mut self,
        market_owner: &Keypair,
        market: Pubkey,
        decimals: u8,
        price_mantissa: i128,
        config: ReserveConfig,
    ) -> ReserveHandle {
        let env_owner = self.owner.insecure_clone();
        let mint = create_token_mint(&mut self.svm, &env_owner, decimals, None).unwrap();
        self.set_price_for(market_owner, market, mint, price_mantissa);

        let reserve = pda(&[RESERVE_SEED, market.as_ref(), mint.as_ref()]);
        let share_mint = pda(&[SHARE_MINT_SEED, reserve.as_ref()]);
        let liquidity_vault = pda(&[LIQUIDITY_VAULT_SEED, reserve.as_ref()]);
        let price_feed = self.price_feed_address(market, mint);

        let instruction = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::InitializeReserve {
                lending_market: market,
                owner: market_owner.pubkey(),
                reserve,
                liquidity_mint: mint,
                liquidity_vault,
                share_mint,
                price_feed,
                token_program: TOKEN_PROGRAM_ID,
                system_program: system_program::id(),
            }
            .to_account_metas(None),
            data: lending::instruction::InitializeReserve { config }.data(),
        };
        send(&mut self.svm, vec![instruction], &[market_owner], &market_owner.pubkey()).unwrap();

        ReserveHandle {
            mint,
            decimals,
            reserve,
            share_mint,
            liquidity_vault,
            price_feed,
        }
    }

    /// Advance time so interest accrues and blockhashes differ.
    pub fn warp_slots(&mut self, slots: u64) {
        let target = self.current_slot() + slots;
        self.svm.warp_to_slot(target);
        self.svm.expire_blockhash();
    }

    /// The feed PDA the market owner writes for `mint`: seeded by the owner's
    /// key, so it is the feed `add_reserve` registers reserves against.
    /// The feed PDA for a given market and mint (seeds `["price_feed", market, mint]`).
    pub fn price_feed_address(&self, market: Pubkey, mint: Pubkey) -> Pubkey {
        pda(&[PRICE_FEED_SEED, market.as_ref(), mint.as_ref()])
    }

    pub fn set_price(&mut self, mint: Pubkey, price_mantissa: i128) {
        let owner = self.owner.insecure_clone();
        let market = self.market;
        self.set_price_for(&owner, market, mint, price_mantissa);
    }

    /// Publish a price for `mint` in `market`, signed by that market's `owner`.
    pub fn set_price_for(
        &mut self,
        owner: &Keypair,
        market: Pubkey,
        mint: Pubkey,
        price_mantissa: i128,
    ) {
        let price_feed = self.price_feed_address(market, mint);
        let instruction = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::SetPrice {
                lending_market: market,
                owner: owner.pubkey(),
                price_feed,
                mint,
                system_program: system_program::id(),
            }
            .to_account_metas(None),
            data: lending::instruction::SetPrice {
                price_mantissa,
                exponent: PRICE_EXPONENT,
            }
            .data(),
        };
        send(&mut self.svm, vec![instruction], &[owner], &owner.pubkey()).unwrap();
    }

    pub fn add_reserve(
        &mut self,
        decimals: u8,
        price_mantissa: i128,
        config: ReserveConfig,
    ) -> ReserveHandle {
        let owner = self.owner.insecure_clone();
        let market = self.market;
        self.add_reserve_to(&owner, market, decimals, price_mantissa, config)
    }

    pub fn try_update_config(
        &mut self,
        handle: &ReserveHandle,
        config: ReserveConfig,
    ) -> Result<(), String> {
        let owner = self.owner.insecure_clone();
        let instruction = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::UpdateReserveConfig {
                lending_market: self.market,
                owner: owner.pubkey(),
                reserve: handle.reserve,
            }
            .to_account_metas(None),
            data: lending::instruction::UpdateReserveConfig { config }.data(),
        };
        send(&mut self.svm, vec![instruction], &[&owner], &owner.pubkey())
    }

    pub fn create_user(&mut self) -> Keypair {
        create_wallet(&mut self.svm, 1_000_000_000_000).unwrap()
    }

    /// Create the user's token account for a mint and mint `amount` into it.
    pub fn fund(&mut self, user: &Keypair, mint: Pubkey, amount: u64) -> Pubkey {
        let owner = self.owner.insecure_clone();
        let token_account =
            create_associated_token_account(&mut self.svm, &user.pubkey(), &mint, user).unwrap();
        if amount > 0 {
            mint_tokens_to_token_account(&mut self.svm, &mint, &token_account, amount, &owner)
                .unwrap();
        }
        token_account
    }

    fn refresh_reserve_ix(&self, handle: &ReserveHandle) -> Instruction {
        Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::RefreshReserve {
                reserve: handle.reserve,
            }
            .to_account_metas(None),
            data: lending::instruction::RefreshReserve {}.data(),
        }
    }

    /// Supply liquidity to a reserve, receiving share tokens. Returns the user's
    /// share-token account.
    pub fn try_supply(
        &mut self,
        user: &Keypair,
        handle: &ReserveHandle,
        amount: u64,
    ) -> Result<Pubkey, String> {
        let user_liquidity = ata(&user.pubkey(), &handle.mint);
        let user_share = create_associated_token_account(
            &mut self.svm,
            &user.pubkey(),
            &handle.share_mint,
            user,
        )
        .unwrap();

        let deposit = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::DepositReserveLiquidity {
                reserve: handle.reserve,
                liquidity_mint: handle.mint,
                liquidity_vault: handle.liquidity_vault,
                share_mint: handle.share_mint,
                user_liquidity,
                user_share,
                owner: user.pubkey(),
                token_program: TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: lending::instruction::DepositReserveLiquidity {
                liquidity_amount: amount,
            }
            .data(),
        };
        let refresh = self.refresh_reserve_ix(handle);
        send(&mut self.svm, vec![refresh, deposit], &[user], &user.pubkey())?;
        Ok(user_share)
    }

    pub fn supply(&mut self, user: &Keypair, handle: &ReserveHandle, amount: u64) -> Pubkey {
        self.try_supply(user, handle, amount).unwrap()
    }

    pub fn try_redeem(
        &mut self,
        user: &Keypair,
        handle: &ReserveHandle,
        share_amount: u64,
    ) -> Result<(), String> {
        let user_liquidity = ata(&user.pubkey(), &handle.mint);
        let user_share = ata(&user.pubkey(), &handle.share_mint);
        let redeem = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::RedeemReserveCollateral {
                reserve: handle.reserve,
                liquidity_mint: handle.mint,
                liquidity_vault: handle.liquidity_vault,
                share_mint: handle.share_mint,
                user_liquidity,
                user_share,
                owner: user.pubkey(),
                token_program: TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: lending::instruction::RedeemReserveCollateral { share_amount }.data(),
        };
        let refresh = self.refresh_reserve_ix(handle);
        send(&mut self.svm, vec![refresh, redeem], &[user], &user.pubkey())
    }

    pub fn initialize_obligation(&mut self, user: &Keypair) -> Pubkey {
        let obligation = pda(&[OBLIGATION_SEED, self.market.as_ref(), user.pubkey().as_ref()]);
        let instruction = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::InitializeObligation {
                lending_market: self.market,
                obligation,
                owner: user.pubkey(),
                system_program: system_program::id(),
            }
            .to_account_metas(None),
            data: lending::instruction::InitializeObligation {}.data(),
        };
        send(&mut self.svm, vec![instruction], &[user], &user.pubkey()).unwrap();
        obligation
    }

    pub fn obligation_share_vault(&self, handle: &ReserveHandle, obligation: Pubkey) -> Pubkey {
        pda(&[
            OBLIGATION_SHARE_VAULT_SEED,
            handle.reserve.as_ref(),
            obligation.as_ref(),
        ])
    }

    pub fn try_post_collateral(
        &mut self,
        user: &Keypair,
        obligation: Pubkey,
        handle: &ReserveHandle,
        share_amount: u64,
    ) -> Result<(), String> {
        let user_share = ata(&user.pubkey(), &handle.share_mint);
        let vault = self.obligation_share_vault(handle, obligation);
        let instruction = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::DepositObligationCollateral {
                obligation,
                owner: user.pubkey(),
                reserve: handle.reserve,
                share_mint: handle.share_mint,
                obligation_share_vault: vault,
                user_share,
                token_program: TOKEN_PROGRAM_ID,
                system_program: system_program::id(),
            }
            .to_account_metas(None),
            data: lending::instruction::DepositObligationCollateral { share_amount }.data(),
        };
        send(&mut self.svm, vec![instruction], &[user], &user.pubkey())
    }

    pub fn post_collateral(
        &mut self,
        user: &Keypair,
        obligation: Pubkey,
        handle: &ReserveHandle,
        share_amount: u64,
    ) {
        self.try_post_collateral(user, obligation, handle, share_amount)
            .unwrap()
    }

    fn refresh_obligation_ix(
        &self,
        obligation: Pubkey,
        deposit_reserves: &[&ReserveHandle],
        borrow_reserves: &[&ReserveHandle],
    ) -> Instruction {
        let mut accounts = lending::accounts::RefreshObligation { obligation }.to_account_metas(None);
        for handle in deposit_reserves.iter().chain(borrow_reserves.iter()) {
            accounts.push(AccountMeta::new_readonly(handle.reserve, false));
            accounts.push(AccountMeta::new_readonly(handle.price_feed, false));
        }
        Instruction {
            program_id: lending::id(),
            accounts,
            data: lending::instruction::RefreshObligation {}.data(),
        }
    }

    /// All reserves an obligation touches must be refreshed before
    /// refresh_obligation; this collects the de-duplicated refresh instructions.
    fn refresh_all_ix(&self, reserves: &[&ReserveHandle]) -> Vec<Instruction> {
        let mut seen: Vec<Pubkey> = Vec::new();
        let mut instructions = Vec::new();
        for handle in reserves {
            if !seen.contains(&handle.reserve) {
                seen.push(handle.reserve);
                instructions.push(self.refresh_reserve_ix(handle));
            }
        }
        instructions
    }

    /// `existing_deposits` / `existing_borrows` must list the obligation's
    /// CURRENT positions (what `refresh_obligation` will value). The reserve
    /// being borrowed is refreshed too, but is only added to `refresh_obligation`
    /// once it actually has a borrow entry — so the first borrow of a new reserve
    /// passes it only via `borrow`, not via `existing_borrows`.
    #[allow(clippy::too_many_arguments)]
    pub fn try_borrow(
        &mut self,
        user: &Keypair,
        obligation: Pubkey,
        existing_deposits: &[&ReserveHandle],
        existing_borrows: &[&ReserveHandle],
        borrow: &ReserveHandle,
        amount: u64,
    ) -> Result<(), String> {
        let mut refresh_set: Vec<&ReserveHandle> = existing_deposits.to_vec();
        refresh_set.extend_from_slice(existing_borrows);
        refresh_set.push(borrow);

        let mut instructions = self.refresh_all_ix(&refresh_set);
        instructions.push(self.refresh_obligation_ix(obligation, existing_deposits, existing_borrows));
        instructions.push(self.borrow_ix(user, obligation, borrow, amount));
        send(&mut self.svm, instructions, &[user], &user.pubkey())
    }

    fn borrow_ix(
        &self,
        user: &Keypair,
        obligation: Pubkey,
        borrow: &ReserveHandle,
        amount: u64,
    ) -> Instruction {
        let user_liquidity = ata(&user.pubkey(), &borrow.mint);
        Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::BorrowObligationLiquidity {
                obligation,
                owner: user.pubkey(),
                reserve: borrow.reserve,
                price_feed: borrow.price_feed,
                liquidity_mint: borrow.mint,
                liquidity_vault: borrow.liquidity_vault,
                user_liquidity,
                token_program: TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: lending::instruction::BorrowObligationLiquidity {
                liquidity_amount: amount,
            }
            .data(),
        }
    }

    /// Borrow while deliberately skipping the `refresh_obligation` instruction,
    /// to exercise the `ObligationStale` guard.
    pub fn try_borrow_skip_obligation_refresh(
        &mut self,
        user: &Keypair,
        obligation: Pubkey,
        all_reserves: &[&ReserveHandle],
        borrow: &ReserveHandle,
        amount: u64,
    ) -> Result<(), String> {
        let mut instructions = self.refresh_all_ix(all_reserves);
        instructions.push(self.borrow_ix(user, obligation, borrow, amount));
        send(&mut self.svm, instructions, &[user], &user.pubkey())
    }

    pub fn repay(
        &mut self,
        user: &Keypair,
        obligation: Pubkey,
        borrow: &ReserveHandle,
        amount: u64,
    ) {
        let user_liquidity = ata(&user.pubkey(), &borrow.mint);
        let instructions = vec![
            self.refresh_reserve_ix(borrow),
            Instruction {
                program_id: lending::id(),
                accounts: lending::accounts::RepayObligationLiquidity {
                    obligation,
                    reserve: borrow.reserve,
                    liquidity_mint: borrow.mint,
                    liquidity_vault: borrow.liquidity_vault,
                    user_liquidity,
                    repayer: user.pubkey(),
                    token_program: TOKEN_PROGRAM_ID,
                }
                .to_account_metas(None),
                data: lending::instruction::RepayObligationLiquidity {
                    liquidity_amount: amount,
                }
                .data(),
            },
        ];
        send(&mut self.svm, instructions, &[user], &user.pubkey()).unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_withdraw_collateral(
        &mut self,
        user: &Keypair,
        obligation: Pubkey,
        deposit_reserves: &[&ReserveHandle],
        borrow_reserves: &[&ReserveHandle],
        collateral: &ReserveHandle,
        share_amount: u64,
    ) -> Result<(), String> {
        let user_share = ata(&user.pubkey(), &collateral.share_mint);
        let vault = self.obligation_share_vault(collateral, obligation);
        let mut all: Vec<&ReserveHandle> = deposit_reserves.to_vec();
        all.extend_from_slice(borrow_reserves);

        let mut instructions = self.refresh_all_ix(&all);
        instructions.push(self.refresh_obligation_ix(obligation, deposit_reserves, borrow_reserves));
        instructions.push(Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::WithdrawObligationCollateral {
                obligation,
                owner: user.pubkey(),
                reserve: collateral.reserve,
                price_feed: collateral.price_feed,
                share_mint: collateral.share_mint,
                obligation_share_vault: vault,
                user_share,
                token_program: TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: lending::instruction::WithdrawObligationCollateral { share_amount }.data(),
        });
        send(&mut self.svm, instructions, &[user], &user.pubkey())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_liquidate(
        &mut self,
        liquidator: &Keypair,
        obligation: Pubkey,
        deposit_reserves: &[&ReserveHandle],
        borrow_reserves: &[&ReserveHandle],
        repay: &ReserveHandle,
        collateral: &ReserveHandle,
        amount: u64,
    ) -> Result<(), String> {
        let repay_source = ata(&liquidator.pubkey(), &repay.mint);
        // Create the destination ATA only on the first call, so a test can
        // attempt several liquidations.
        let collateral_dest = ata(&liquidator.pubkey(), &collateral.share_mint);
        if self.svm.get_account(&collateral_dest).is_none() {
            create_associated_token_account(
                &mut self.svm,
                &liquidator.pubkey(),
                &collateral.share_mint,
                liquidator,
            )
            .unwrap();
        }
        let vault = self.obligation_share_vault(collateral, obligation);

        let mut all: Vec<&ReserveHandle> = deposit_reserves.to_vec();
        all.extend_from_slice(borrow_reserves);
        let mut instructions = self.refresh_all_ix(&all);
        instructions.push(self.refresh_obligation_ix(obligation, deposit_reserves, borrow_reserves));
        instructions.push(Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::LiquidateObligation {
                obligation,
                liquidator: liquidator.pubkey(),
                repay_reserve: repay.reserve,
                collateral_reserve: collateral.reserve,
                repay_price_feed: repay.price_feed,
                collateral_price_feed: collateral.price_feed,
                repay_liquidity_mint: repay.mint,
                collateral_share_mint: collateral.share_mint,
                repay_liquidity_vault: repay.liquidity_vault,
                obligation_collateral_vault: vault,
                liquidator_repay_source: repay_source,
                liquidator_collateral_dest: collateral_dest,
                token_program: TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: lending::instruction::LiquidateObligation {
                liquidity_amount: amount,
            }
            .data(),
        });
        send(&mut self.svm, instructions, &[liquidator], &liquidator.pubkey())
    }

    /// Send a lone `refresh_reserve` so accrued interest lands in the index.
    pub fn refresh_reserve_only(&mut self, payer: &Keypair, handle: &ReserveHandle) {
        let instruction = self.refresh_reserve_ix(handle);
        send(&mut self.svm, vec![instruction], &[payer], &payer.pubkey()).unwrap();
    }

    /// Refresh the listed reserves and then the obligation, recomputing its values.
    pub fn refresh_obligation_only(
        &mut self,
        payer: &Keypair,
        obligation: Pubkey,
        deposits: &[&ReserveHandle],
        borrows: &[&ReserveHandle],
    ) {
        let mut all: Vec<&ReserveHandle> = deposits.to_vec();
        all.extend_from_slice(borrows);
        let mut instructions = self.refresh_all_ix(&all);
        instructions.push(self.refresh_obligation_ix(obligation, deposits, borrows));
        send(&mut self.svm, instructions, &[payer], &payer.pubkey()).unwrap();
    }

    /// Market owner collects accrued protocol fees from a reserve to their own
    /// token account. Bundles `refresh_reserve` so fees are current. Returns the
    /// owner's fee-receiving token account.
    pub fn collect_protocol_fees(&mut self, handle: &ReserveHandle) -> Pubkey {
        let owner = self.owner.insecure_clone();
        let owner_liquidity = ata(&owner.pubkey(), &handle.mint);
        if self.svm.get_account(&owner_liquidity).is_none() {
            create_associated_token_account(&mut self.svm, &owner.pubkey(), &handle.mint, &owner)
                .unwrap();
        }
        let refresh = self.refresh_reserve_ix(handle);
        let collect = Instruction {
            program_id: lending::id(),
            accounts: lending::accounts::CollectProtocolFees {
                lending_market: self.market,
                owner: owner.pubkey(),
                reserve: handle.reserve,
                liquidity_mint: handle.mint,
                liquidity_vault: handle.liquidity_vault,
                owner_liquidity,
                token_program: TOKEN_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: lending::instruction::CollectProtocolFees {}.data(),
        };
        send(&mut self.svm, vec![refresh, collect], &[&owner], &owner.pubkey()).unwrap();
        owner_liquidity
    }

    // --- state readers ---

    pub fn reserve(&self, handle: &ReserveHandle) -> Reserve {
        let account = self.svm.get_account(&handle.reserve).unwrap();
        Reserve::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn obligation(&self, obligation: Pubkey) -> Obligation {
        let account = self.svm.get_account(&obligation).unwrap();
        Obligation::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn token_balance(&self, token_account: Pubkey) -> u64 {
        get_token_account_balance(&self.svm, &token_account).unwrap()
    }
}

/// A reasonable default reserve config: 75% LTV, 80% liquidation threshold,
/// 5% bonus, 50% close factor, 10% reserve factor (protocol's cut of interest),
/// kink at 80% utilization, 2%/20%/150% APR curve.
pub fn default_config() -> ReserveConfig {
    ReserveConfig {
        loan_to_value_bps: 7_500,
        liquidation_threshold_bps: 8_000,
        liquidation_bonus_bps: 500,
        close_factor_bps: 5_000,
        reserve_factor_bps: 1_000,
        optimal_utilization_bps: 8_000,
        min_borrow_rate_bps: 200,
        optimal_borrow_rate_bps: 2_000,
        max_borrow_rate_bps: 15_000,
    }
}
