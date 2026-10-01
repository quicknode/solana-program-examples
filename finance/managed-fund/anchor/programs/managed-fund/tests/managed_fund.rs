use {
    anchor_lang::{
        solana_program::{instruction::AccountMeta, instruction::Instruction},
        system_program, AccountDeserialize, Address, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token,
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
    solana_account::Account as SolanaAccount,
    // LiteSVM's get_sysvar / set_sysvar want the host-side Clock, not pinocchio's.
    solana_clock::Clock,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
};

use managed_fund::error::FundError;

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

fn pyth_receiver_program_id() -> Address {
    "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ"
        .parse()
        .unwrap()
}

fn derive_ata(wallet: &Address, mint: &Address) -> Address {
    let (ata, _bump) = Address::find_program_address(
        &[wallet.as_ref(), token_program_id().as_ref(), mint.as_ref()],
        &ata_program_id(),
    );
    ata
}

/// Mock PriceUpdateV2 layout (see pyth-solana-receiver-sdk): price i64 at 73,
/// publish_time i64 at 93, posted_slot u64 at 125. Exponent -8.
fn build_mock_price_update_account(
    price: i64,
    exponent: i32,
    publish_time: i64,
    posted_slot: u64,
) -> Vec<u8> {
    let discriminator: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];
    let mut data = Vec::with_capacity(133);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&[0u8; 32]);
    data.push(1u8);
    data.extend_from_slice(&[0xEFu8; 32]);
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&100_000u64.to_le_bytes());
    data.extend_from_slice(&exponent.to_le_bytes());
    data.extend_from_slice(&publish_time.to_le_bytes());
    data.extend_from_slice(&(publish_time - 1).to_le_bytes());
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&120_000u64.to_le_bytes());
    data.extend_from_slice(&posted_slot.to_le_bytes());
    data
}

fn set_price_feed(svm: &mut LiteSVM, key: Address, price: i64) {
    set_price_feed_posted_at(svm, key, price, 1);
}

/// Write a Pyth feed as if Pyth posted it in `posted_slot`.
fn set_price_feed_posted_at(svm: &mut LiteSVM, key: Address, price: i64, posted_slot: u64) {
    write_price_feed(svm, key, price, -8, posted_slot);
}

/// Write a Pyth feed with its own exponent: Pyth's US equity feeds use -5.
fn write_price_feed(svm: &mut LiteSVM, key: Address, price: i64, exponent: i32, posted_slot: u64) {
    let data = build_mock_price_update_account(price, exponent, PUBLISH_TIME, posted_slot);
    let rent = svm.minimum_balance_for_rent_exemption(data.len());
    svm.set_account(
        key,
        SolanaAccount {
            lamports: rent,
            data,
            owner: pyth_receiver_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

const PUBLISH_TIME: i64 = 1_700_000_000;
const TOKEN_DECIMALS: u8 = 6;
const SECONDS_PER_YEAR: i64 = 31_536_000;

const TSLA_PRICE: i64 = 25_000_000_000; // $250
const NVDA_PRICE: i64 = 18_000_000_000; // $180
const TSLA_RATE: u64 = 250_000_000; // router USDC minor units per whole token
const NVDA_RATE: u64 = 180_000_000;

const FEE_BPS: u16 = 100; // 1%
const SLIPPAGE_BPS: u16 = 100; // 1%
const REBALANCE_THRESHOLD_BPS: u16 = 200; // two percentage points
const FUND_INDEX: u64 = 0; // fund PDA seed: "fund" + 0

struct TestContext {
    svm: LiteSVM,
    fund_program_id: Address,
    router_program_id: Address,
    manager: Keypair,
    payer: Keypair,
    usdc_mint: Address,
    tsla_mint: Address,
    nvda_mint: Address,
    fund_pda: Address,
    share_mint_pda: Address,
    registry_pda: Address,
    approved_tsla: Address,
    approved_nvda: Address,
    router_config_pda: Address,
    tsla_rate_pda: Address,
    nvda_rate_pda: Address,
    vault_usdc: Address,
    vault_tsla: Address,
    vault_nvda: Address,
    router_usdc_treasury: Address,
    price_feed_tsla: Address,
    price_feed_nvda: Address,
}

impl TestContext {
    fn asset_config(&self, index: u8) -> Address {
        Address::find_program_address(
            &[b"asset", self.fund_pda.as_ref(), &[index]],
            &self.fund_program_id,
        )
        .0
    }
}

/// Mints, router (config + rates + treasury), Pyth feeds, a registry with TSLAx
/// and NVDAx approved, and all derived PDAs. Does not create the fund.
fn setup_full() -> TestContext {
    setup_with_tsla_decimals(TOKEN_DECIMALS)
}

/// `setup_full`, with TSLAx minted at `tsla_decimals` instead of six.
fn setup_with_tsla_decimals(tsla_decimals: u8) -> TestContext {
    let fund_program_id = managed_fund::id();
    let router_program_id = mock_swap_router::id();

    let mut svm = anchor_v2_testing::svm();
    svm.add_program(
        fund_program_id,
        include_bytes!("../../../target/deploy/managed_fund.so"),
    )
    .unwrap();
    // Use std::fs::read() instead of include_bytes!() for the router program because
    // include_bytes!() runs at compile time, and during `anchor build` the IDL generation
    // step compiles tests before the .so files exist. Since this is a cross-program
    // dependency (not our own program), mock_swap_router.so may not be built yet at compile time.
    let router_bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/deploy/mock_swap_router.so"
    ))
    .expect("mock_swap_router.so not found - run `anchor build` first");
    svm.add_program(router_program_id, &router_bytes).unwrap();

    svm.set_sysvar(&Clock {
        slot: 1,
        epoch_start_timestamp: PUBLISH_TIME,
        epoch: 0,
        leader_schedule_epoch: 0,
        unix_timestamp: PUBLISH_TIME,
    });

    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
    let manager = create_wallet(&mut svm, 10_000_000_000).unwrap();

    let usdc_mint = create_token_mint(&mut svm, &payer, TOKEN_DECIMALS, None).unwrap();
    let tsla_mint = create_token_mint(&mut svm, &payer, tsla_decimals, None).unwrap();
    let nvda_mint = create_token_mint(&mut svm, &payer, TOKEN_DECIMALS, None).unwrap();

    let (router_config_pda, _) =
        Address::find_program_address(&[b"router_config"], &router_program_id);

    // The router mints basket assets on swap, so its config account, which signs
    // the router's token CPIs, must hold their mint authority.
    for basket_mint in [&tsla_mint, &nvda_mint] {
        let ix = spl_token::instruction::set_authority(
            &spl_token::ID,
            basket_mint,
            Some(&router_config_pda),
            spl_token::instruction::AuthorityType::MintTokens,
            &payer.pubkey(),
            &[],
        )
        .unwrap();
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey()).unwrap();
    }

    let (fund_pda, _) = Address::find_program_address(
        &[b"fund", FUND_INDEX.to_le_bytes().as_ref()],
        &fund_program_id,
    );
    let (share_mint_pda, _) =
        Address::find_program_address(&[b"share_mint", fund_pda.as_ref()], &fund_program_id);
    let (registry_pda, _) =
        Address::find_program_address(&[b"registry", payer.pubkey().as_ref()], &fund_program_id);
    let (approved_tsla, _) = Address::find_program_address(
        &[b"approved_asset", registry_pda.as_ref(), tsla_mint.as_ref()],
        &fund_program_id,
    );
    let (approved_nvda, _) = Address::find_program_address(
        &[b"approved_asset", registry_pda.as_ref(), nvda_mint.as_ref()],
        &fund_program_id,
    );
    let (tsla_rate_pda, _) =
        Address::find_program_address(&[b"rate", tsla_mint.as_ref()], &router_program_id);
    let (nvda_rate_pda, _) =
        Address::find_program_address(&[b"rate", nvda_mint.as_ref()], &router_program_id);

    let vault_usdc = derive_ata(&fund_pda, &usdc_mint);
    let vault_tsla = derive_ata(&fund_pda, &tsla_mint);
    let vault_nvda = derive_ata(&fund_pda, &nvda_mint);
    let router_usdc_treasury = derive_ata(&router_config_pda, &usdc_mint);

    let price_feed_tsla = Keypair::new().pubkey();
    let price_feed_nvda = Keypair::new().pubkey();
    set_price_feed(&mut svm, price_feed_tsla, TSLA_PRICE);
    set_price_feed(&mut svm, price_feed_nvda, NVDA_PRICE);

    // Router: init, rates, treasury.
    let init_router_ix = Instruction::new_with_bytes(
        router_program_id,
        &mock_swap_router::instruction::InitializeRouter { usdc_mint }.data(),
        mock_swap_router::accounts::InitializeRouterAccountConstraints {
            authority: payer.pubkey(),
            usdc_mint,
            router_config: router_config_pda,
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![init_router_ix], &[&payer], &payer.pubkey())
        .unwrap();

    for (mint, rate, rate_pda) in [
        (tsla_mint, TSLA_RATE, tsla_rate_pda),
        (nvda_mint, NVDA_RATE, nvda_rate_pda),
    ] {
        let ix = Instruction::new_with_bytes(
            router_program_id,
            &mock_swap_router::instruction::SetRate {
                mint,
                usdc_per_token: rate,
            }
            .data(),
            mock_swap_router::accounts::SetRateAccountConstraints {
                authority: payer.pubkey(),
                router_config: router_config_pda,
                asset_mint: mint,
                usdc_mint,
                asset_rate: rate_pda,
                router_usdc_treasury,
                associated_token_program: ata_program_id(),
                token_program: token_program_id(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey()).unwrap();
    }

    mint_tokens_to_token_account(
        &mut svm,
        &usdc_mint,
        &router_usdc_treasury,
        10_000_000_000u64,
        &payer,
    )
    .unwrap();

    // Registry with both basket assets approved, bound to their feeds.
    let init_registry_ix = Instruction::new_with_bytes(
        fund_program_id,
        &managed_fund::instruction::InitializeRegistry {}.data(),
        managed_fund::accounts::InitializeRegistryAccountConstraints {
            authority: payer.pubkey(),
            registry: registry_pda,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut svm,
        vec![init_registry_ix],
        &[&payer],
        &payer.pubkey(),
    )
    .unwrap();

    for (mint, feed, entry) in [
        (tsla_mint, price_feed_tsla, approved_tsla),
        (nvda_mint, price_feed_nvda, approved_nvda),
    ] {
        let ix = Instruction::new_with_bytes(
            fund_program_id,
            &managed_fund::instruction::ApproveAsset { price_feed: feed }.data(),
            managed_fund::accounts::ApproveAssetAccountConstraints {
                authority: payer.pubkey(),
                registry: registry_pda,
                asset_mint: mint,
                approved_asset: entry,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey()).unwrap();
    }

    TestContext {
        svm,
        fund_program_id,
        router_program_id,
        manager,
        payer,
        usdc_mint,
        tsla_mint,
        nvda_mint,
        fund_pda,
        share_mint_pda,
        registry_pda,
        approved_tsla,
        approved_nvda,
        router_config_pda,
        tsla_rate_pda,
        nvda_rate_pda,
        vault_usdc,
        vault_tsla,
        vault_nvda,
        router_usdc_treasury,
        price_feed_tsla,
        price_feed_nvda,
    }
}

fn initialize_fund_instruction(
    ctx: &TestContext,
    fee_bps: u16,
    max_slippage_bps: u16,
    rebalance_threshold_bps: u16,
    router: Address,
) -> Instruction {
    Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::InitializeFund {
            index: FUND_INDEX,
            fee_bps,
            max_slippage_bps,
            rebalance_threshold_bps,
            swap_router: router,
        }
        .data(),
        managed_fund::accounts::InitializeFundAccountConstraints {
            manager: ctx.manager.pubkey(),
            usdc_mint: ctx.usdc_mint,
            registry: ctx.registry_pda,
            fund: ctx.fund_pda,
            share_mint: ctx.share_mint_pda,
            vault_usdc: ctx.vault_usdc,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn init_fund(ctx: &mut TestContext, fee_bps: u16, slippage_bps: u16, router: Address) {
    let ix =
        initialize_fund_instruction(ctx, fee_bps, slippage_bps, REBALANCE_THRESHOLD_BPS, router);
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    )
    .unwrap();
}

fn add_asset(
    ctx: &mut TestContext,
    index: u8,
    mint: Address,
    approved_asset: Address,
    vault: Address,
    weight_bps: u16,
) -> Result<(), solana_kite::SolanaKiteError> {
    let asset_config = ctx.asset_config(index);
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::AddAsset { weight_bps }.data(),
        managed_fund::accounts::AddAssetAccountConstraints {
            manager: ctx.manager.pubkey(),
            fund: ctx.fund_pda,
            registry: ctx.registry_pda,
            asset_mint: mint,
            approved_asset,
            asset_config,
            vault_asset: vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    )
}

/// init fund + add TSLAx (index 0, 40%) + NVDAx (index 1, 60%).
fn standard_fund(ctx: &mut TestContext) {
    let router = ctx.router_program_id;
    init_fund(ctx, FEE_BPS, SLIPPAGE_BPS, router);
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.approved_tsla, ctx.vault_tsla);
    add_asset(ctx, 0, tm, wt, vt, 4000).unwrap();
    let (nm, wn, vn) = (ctx.nvda_mint, ctx.approved_nvda, ctx.vault_nvda);
    add_asset(ctx, 1, nm, wn, vn, 6000).unwrap();
}

/// One asset's deposit remaining_accounts, in the order the handler reads:
/// [asset_config, vault, mint, rate, price_feed]. Deposit deploys into the asset,
/// so vault and mint must be writable.
fn asset_deposit_metas(
    config: Address,
    vault: Address,
    mint: Address,
    rate: Address,
    feed: Address,
) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new_readonly(config, false),
        AccountMeta::new(vault, false),
        AccountMeta::new(mint, false),
        AccountMeta::new_readonly(rate, false),
        AccountMeta::new_readonly(feed, false),
    ]
}

fn deposit_remaining_tsla(ctx: &TestContext) -> Vec<AccountMeta> {
    asset_deposit_metas(
        ctx.asset_config(0),
        ctx.vault_tsla,
        ctx.tsla_mint,
        ctx.tsla_rate_pda,
        ctx.price_feed_tsla,
    )
}

/// remaining_accounts for a deposit into the two-asset standard fund.
fn deposit_remaining(ctx: &TestContext) -> Vec<AccountMeta> {
    let mut metas = deposit_remaining_tsla(ctx);
    metas.extend(asset_deposit_metas(
        ctx.asset_config(1),
        ctx.vault_nvda,
        ctx.nvda_mint,
        ctx.nvda_rate_pda,
        ctx.price_feed_nvda,
    ));
    metas
}

/// Named accounts for a deposit (everything except per-asset remaining_accounts).
fn deposit_named_metas(ctx: &TestContext, user: &Keypair) -> Vec<AccountMeta> {
    managed_fund::accounts::DepositAccountConstraints {
        depositor: user.pubkey(),
        fund: ctx.fund_pda,
        share_mint: ctx.share_mint_pda,
        usdc_mint: ctx.usdc_mint,
        depositor_usdc_account: derive_ata(&user.pubkey(), &ctx.usdc_mint),
        depositor_share_account: derive_ata(&user.pubkey(), &ctx.share_mint_pda),
        vault_usdc: ctx.vault_usdc,
        router_config: ctx.router_config_pda,
        router_usdc_treasury: ctx.router_usdc_treasury,
        swap_router_program: ctx.router_program_id,
        associated_token_program: ata_program_id(),
        token_program: token_program_id(),
        system_program: system_program::ID,
    }
    .to_account_metas(None)
}

fn deposit_instruction(
    ctx: &TestContext,
    user: &Keypair,
    usdc_amount: u64,
    minimum_shares: u64,
    remaining: Vec<AccountMeta>,
) -> Instruction {
    let mut metas = deposit_named_metas(ctx, user);
    metas.extend(remaining);
    Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::Deposit {
            usdc_amount,
            minimum_shares,
        }
        .data(),
        metas,
    )
}

/// Deposit into the two-asset standard fund, auto-deploying at the target weights.
fn do_deposit(
    ctx: &mut TestContext,
    user: &Keypair,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Address {
    let remaining = deposit_remaining(ctx);
    let ix = deposit_instruction(ctx, user, usdc_amount, minimum_shares, remaining);
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[user], &user.pubkey()).unwrap();
    derive_ata(&user.pubkey(), &ctx.share_mint_pda)
}

/// Deposit into a TSLAx-only fund.
fn do_deposit_tsla_only(
    ctx: &mut TestContext,
    user: &Keypair,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Address {
    let remaining = deposit_remaining_tsla(ctx);
    let ix = deposit_instruction(ctx, user, usdc_amount, minimum_shares, remaining);
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[user], &user.pubkey()).unwrap();
    derive_ata(&user.pubkey(), &ctx.share_mint_pda)
}

/// Update the router's exchange rate for a mint (and its Pyth feed stays the caller's
/// job). Used to keep the router quote in step with a price move.
fn set_router_rate(ctx: &mut TestContext, mint: Address, rate: u64, rate_pda: Address) {
    let ix = Instruction::new_with_bytes(
        ctx.router_program_id,
        &mock_swap_router::instruction::SetRate {
            mint,
            usdc_per_token: rate,
        }
        .data(),
        mock_swap_router::accounts::SetRateAccountConstraints {
            authority: ctx.payer.pubkey(),
            router_config: ctx.router_config_pda,
            asset_mint: mint,
            usdc_mint: ctx.usdc_mint,
            asset_rate: rate_pda,
            router_usdc_treasury: ctx.router_usdc_treasury,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&ctx.payer], &ctx.payer.pubkey())
        .unwrap();
}

/// Move NVDAx's price: rewrite its Pyth feed and update the router rate to match.
fn set_nvda_price(ctx: &mut TestContext, price: i64, rate: u64) {
    set_price_feed(&mut ctx.svm, ctx.price_feed_nvda, price);
    let nvda_mint = ctx.nvda_mint;
    let nvda_rate_pda = ctx.nvda_rate_pda;
    set_router_rate(ctx, nvda_mint, rate, nvda_rate_pda);
}

fn set_weight(
    ctx: &mut TestContext,
    index: u8,
    weight_bps: u16,
) -> Result<(), solana_kite::SolanaKiteError> {
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::SetWeight { weight_bps }.data(),
        managed_fund::accounts::SetWeightAccountConstraints {
            manager: ctx.manager.pubkey(),
            fund: ctx.fund_pda,
            asset_config: ctx.asset_config(index),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    )
}

/// init fund + add only TSLAx at 40%, so total weight is 4000: the fund is
/// under-allocated and rejects deposits until its weights reach 100%.
fn tsla_only_fund(ctx: &mut TestContext) {
    let router = ctx.router_program_id;
    init_fund(ctx, FEE_BPS, SLIPPAGE_BPS, router);
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.approved_tsla, ctx.vault_tsla);
    add_asset(ctx, 0, tm, wt, vt, 4000).unwrap();
}

fn read_fund(ctx: &TestContext) -> managed_fund::state::Fund {
    let account = ctx.svm.get_account(&ctx.fund_pda).unwrap();
    managed_fund::state::Fund::try_deserialize(&mut &account.data[..]).unwrap()
}

/// How a failed transaction reports one of the program's errors: Anchor numbers
/// them from 6000 in declaration order.
fn program_error(error: FundError) -> String {
    format!("Custom({})", 6000 + error as u32)
}

/// Recorded holdings must equal the vaults' token balances whenever nothing has
/// been donated: a mismatch means a handler moved tokens without recording it.
fn assert_holdings_match_vaults(ctx: &TestContext) {
    let fund = read_fund(ctx);
    assert_eq!(
        fund.usdc_holdings,
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        "recorded USDC matches the USDC vault"
    );
    assert_eq!(
        fund.asset_holdings[0],
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        "recorded TSLAx matches the TSLAx vault"
    );
    assert_eq!(
        fund.asset_holdings[1],
        get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap(),
        "recorded NVDAx matches the NVDAx vault"
    );
}

/// Transfer USDC straight into the fund's USDC vault with an ordinary token
/// transfer, never calling the deposit handler.
fn donate_usdc(ctx: &mut TestContext, donor: &Keypair, amount: u64) {
    let ix = spl_token::instruction::transfer(
        &spl_token::ID,
        &derive_ata(&donor.pubkey(), &ctx.usdc_mint),
        &ctx.vault_usdc,
        &donor.pubkey(),
        &[],
        amount,
    )
    .unwrap();
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[donor], &donor.pubkey()).unwrap();
}

/// A holder's position valued in USDC minor units at the test's starting prices
/// (TSLAx $250, NVDAx $180; both assets and USDC have six decimals).
fn value_in_usdc(ctx: &TestContext, owner: &Address) -> u64 {
    let balance =
        |mint: &Address| get_token_account_balance(&ctx.svm, &derive_ata(owner, mint)).unwrap_or(0);
    balance(&ctx.usdc_mint)
        + balance(&ctx.tsla_mint) * (TSLA_PRICE as u64 / 100_000_000)
        + balance(&ctx.nvda_mint) * (NVDA_PRICE as u64 / 100_000_000)
}

fn read_asset_config(ctx: &TestContext, index: u8) -> managed_fund::state::AssetConfig {
    let account = ctx.svm.get_account(&ctx.asset_config(index)).unwrap();
    managed_fund::state::AssetConfig::try_deserialize(&mut &account.data[..]).unwrap()
}

/// A rebalance of the standard fund, signed by `caller`: the program computes
/// the trade, so the caller names only the pair. The remaining accounts are the
/// same five per asset that a deposit takes.
fn rebalance_instruction(
    ctx: &TestContext,
    caller: &Keypair,
    sell_index: u8,
    buy_index: u8,
) -> Instruction {
    rebalance_instruction_with(ctx, caller, sell_index, buy_index, deposit_remaining(ctx))
}

fn rebalance_instruction_with(
    ctx: &TestContext,
    caller: &Keypair,
    sell_index: u8,
    buy_index: u8,
    remaining: Vec<AccountMeta>,
) -> Instruction {
    let mut metas = managed_fund::accounts::RebalanceAccountConstraints {
        caller: caller.pubkey(),
        fund: ctx.fund_pda,
        usdc_mint: ctx.usdc_mint,
        vault_usdc: ctx.vault_usdc,
        router_config: ctx.router_config_pda,
        router_usdc_treasury: ctx.router_usdc_treasury,
        swap_router_program: ctx.router_program_id,
        associated_token_program: ata_program_id(),
        token_program: token_program_id(),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    metas.extend(remaining);
    Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::Rebalance {
            sell_index,
            buy_index,
        }
        .data(),
        metas,
    )
}

/// Rebalance the standard fund, signed by a fresh wallet that is neither the
/// manager nor a depositor.
fn try_rebalance(
    ctx: &mut TestContext,
    sell_index: u8,
    buy_index: u8,
) -> Result<(), solana_kite::SolanaKiteError> {
    let stranger = create_wallet(&mut ctx.svm, 1_000_000_000).unwrap();
    let ix = rebalance_instruction(ctx, &stranger, sell_index, buy_index);
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&stranger], &stranger.pubkey())
}

fn do_rebalance(ctx: &mut TestContext, sell_index: u8, buy_index: u8) {
    try_rebalance(ctx, sell_index, buy_index).unwrap();
}

/// Assert that a transaction failed with one of the program's errors.
fn assert_program_error(
    result: Result<(), solana_kite::SolanaKiteError>,
    error: FundError,
    why: &str,
) {
    let err = format!("{:?}", result.expect_err(why));
    assert!(err.contains(&program_error(error)), "{why}: {err}");
}

fn advance_one_year(ctx: &mut TestContext) {
    let clock = ctx.svm.get_sysvar::<Clock>();
    ctx.svm.set_sysvar(&Clock {
        slot: clock.slot + 1_000_000,
        epoch_start_timestamp: clock.epoch_start_timestamp,
        epoch: clock.epoch,
        leader_schedule_epoch: clock.leader_schedule_epoch,
        unix_timestamp: PUBLISH_TIME + SECONDS_PER_YEAR,
    });
}

fn do_collect_fees(ctx: &mut TestContext) -> Address {
    let manager_share = derive_ata(&ctx.manager.pubkey(), &ctx.share_mint_pda);
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::CollectFees {}.data(),
        managed_fund::accounts::CollectFeesAccountConstraints {
            manager: ctx.manager.pubkey(),
            fund: ctx.fund_pda,
            share_mint: ctx.share_mint_pda,
            manager_share_account: manager_share,
            payer: ctx.payer.pubkey(),
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&ctx.payer], &ctx.payer.pubkey())
        .unwrap();
    manager_share
}

fn fund_user(ctx: &mut TestContext, usdc_amount: u64) -> Keypair {
    let user = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(
        &mut ctx.svm,
        &ctx.usdc_mint,
        &user_usdc,
        usdc_amount,
        &ctx.payer,
    )
    .unwrap();
    user
}

// ----------------------------------------------------------------------------

#[test]
fn test_initialize_and_add_assets() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    let account = ctx.svm.get_account(&ctx.fund_pda).unwrap();
    let fund = managed_fund::state::Fund::try_deserialize(&mut &account.data[..]).unwrap();
    assert_eq!(fund.asset_count, 2);
    assert_eq!(fund.total_weight_bps, 10_000);
    assert_eq!(fund.fee_bps, FEE_BPS);
    assert_eq!(fund.max_slippage_bps, SLIPPAGE_BPS);
    assert_eq!(fund.registry, ctx.registry_pda);

    let cfg0 = ctx.svm.get_account(&ctx.asset_config(0)).unwrap();
    let asset0 = managed_fund::state::AssetConfig::try_deserialize(&mut &cfg0.data[..]).unwrap();
    assert_eq!(asset0.mint, ctx.tsla_mint);
    assert_eq!(asset0.price_feed, ctx.price_feed_tsla);
    assert_eq!(asset0.vault, ctx.vault_tsla);
    assert_eq!(asset0.weight_bps, 4000);
}

#[test]
fn test_add_asset_rejects_unapproved() {
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    init_fund(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);

    // A mint that was never approved: its approved_asset PDA does not exist.
    let rogue_mint = create_token_mint(&mut ctx.svm, &ctx.payer, TOKEN_DECIMALS, None).unwrap();
    let (rogue_entry, _) = Address::find_program_address(
        &[
            b"approved_asset",
            ctx.registry_pda.as_ref(),
            rogue_mint.as_ref(),
        ],
        &ctx.fund_program_id,
    );
    let rogue_vault = derive_ata(&ctx.fund_pda, &rogue_mint);

    let result = add_asset(&mut ctx, 0, rogue_mint, rogue_entry, rogue_vault, 5000);
    assert!(result.is_err(), "adding an unapproved mint must fail");
}

#[test]
fn test_add_asset_rejects_weight_overflow() {
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    init_fund(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.approved_tsla, ctx.vault_tsla);
    add_asset(&mut ctx, 0, tm, wt, vt, 6000).unwrap();
    let (nm, wn, vn) = (ctx.nvda_mint, ctx.approved_nvda, ctx.vault_nvda);
    let result = add_asset(&mut ctx, 1, nm, wn, vn, 6000);
    assert!(result.is_err(), "weights over 10000 bps must fail");
}

/// Create a fresh mint and approve it in the registry. The bound price feed is an
/// arbitrary pubkey: callers that never value this asset (e.g. the cap boundary test)
/// do not need a real feed account.
fn create_and_approve_mint(ctx: &mut TestContext) -> (Address, Address) {
    let mint = create_token_mint(&mut ctx.svm, &ctx.payer, TOKEN_DECIMALS, None).unwrap();
    let (entry, _) = Address::find_program_address(
        &[b"approved_asset", ctx.registry_pda.as_ref(), mint.as_ref()],
        &ctx.fund_program_id,
    );
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::ApproveAsset {
            price_feed: Keypair::new().pubkey(),
        }
        .data(),
        managed_fund::accounts::ApproveAssetAccountConstraints {
            authority: ctx.payer.pubkey(),
            registry: ctx.registry_pda,
            asset_mint: mint,
            approved_asset: entry,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&ctx.payer], &ctx.payer.pubkey())
        .unwrap();
    (mint, entry)
}

#[test]
fn test_add_asset_enforces_max_assets() {
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    init_fund(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);

    // Fill the basket to the cap: 16 assets at 625 bps each = 10000.
    for index in 0..16u8 {
        let (mint, entry) = create_and_approve_mint(&mut ctx);
        let vault = derive_ata(&ctx.fund_pda, &mint);
        add_asset(&mut ctx, index, mint, entry, vault, 625).unwrap();
    }
    let fund = read_fund(&ctx);
    assert_eq!(fund.asset_count, 16);
    assert_eq!(fund.total_weight_bps, 10_000);

    // The 17th asset must be rejected. Weight 0 so the cap, not the weight sum, trips.
    let (mint, entry) = create_and_approve_mint(&mut ctx);
    let vault = derive_ata(&ctx.fund_pda, &mint);
    let result = add_asset(&mut ctx, 16, mint, entry, vault, 0);
    assert!(result.is_err(), "adding beyond MAX_ASSETS must revert");
}

#[test]
fn test_initialize_rejects_excessive_fee() {
    let mut ctx = setup_full();
    let excessive = managed_fund::instructions::initialize_fund::MAX_FEE_BPS + 1;
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::InitializeFund {
            index: FUND_INDEX,
            fee_bps: excessive,
            max_slippage_bps: SLIPPAGE_BPS,
            rebalance_threshold_bps: REBALANCE_THRESHOLD_BPS,
            swap_router: ctx.router_program_id,
        }
        .data(),
        managed_fund::accounts::InitializeFundAccountConstraints {
            manager: ctx.manager.pubkey(),
            usdc_mint: ctx.usdc_mint,
            registry: ctx.registry_pda,
            fund: ctx.fund_pda,
            share_mint: ctx.share_mint_pda,
            vault_usdc: ctx.vault_usdc,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    );
    assert!(r.is_err(), "fee above MAX_FEE_BPS must be rejected");
}

#[test]
fn test_initialize_rejects_excessive_slippage() {
    let mut ctx = setup_full();
    let excessive = managed_fund::instructions::initialize_fund::MAX_SLIPPAGE_BPS + 1;
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::InitializeFund {
            index: FUND_INDEX,
            fee_bps: FEE_BPS,
            max_slippage_bps: excessive,
            rebalance_threshold_bps: REBALANCE_THRESHOLD_BPS,
            swap_router: ctx.router_program_id,
        }
        .data(),
        managed_fund::accounts::InitializeFundAccountConstraints {
            manager: ctx.manager.pubkey(),
            usdc_mint: ctx.usdc_mint,
            registry: ctx.registry_pda,
            fund: ctx.fund_pda,
            share_mint: ctx.share_mint_pda,
            vault_usdc: ctx.vault_usdc,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    );
    assert!(
        r.is_err(),
        "slippage above MAX_SLIPPAGE_BPS must be rejected"
    );
}

#[test]
fn test_deposit_first() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    let amount = 1_000_000u64; // 1 USDC
    let user = fund_user(&mut ctx, amount);
    let user_share = do_deposit(&mut ctx, &user, amount, amount);

    // First deposit is 1:1, then deployed at 40/60: 0.4 USDC -> TSLAx, 0.6 -> NVDAx,
    // leaving no idle USDC.
    assert_eq!(
        get_token_account_balance(&ctx.svm, &user_share).unwrap(),
        amount
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        0
    );
    // 400000 USDC / 250 = 1600 TSLAx; 600000 USDC / 180 = 3333 NVDAx (floor).
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        1_600
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap(),
        3_333
    );
}

#[test]
fn test_deposit_rejects_underallocated() {
    let mut ctx = setup_full();
    // TSLAx at 40% only: total weight is 4000, so the fund is not investable yet.
    tsla_only_fund(&mut ctx);

    let user = fund_user(&mut ctx, 10_000_000);
    let ix = deposit_instruction(&ctx, &user, 10_000_000, 1, deposit_remaining_tsla(&ctx));
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(
        r.is_err(),
        "deposit into an under-allocated fund must revert"
    );

    // Bring TSLAx to 100%; the deposit now succeeds and deploys fully into TSLAx.
    // Fresh blockhash so the retry is not byte-identical to the reverted attempt.
    set_weight(&mut ctx, 0, 10_000).unwrap();
    ctx.svm.expire_blockhash();
    do_deposit_tsla_only(&mut ctx, &user, 10_000_000, 1);

    // 10 USDC / 250 = 40000 TSLAx, with no idle USDC left.
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        40_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        0
    );
}

#[test]
fn test_deposit_rejects_slippage() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    // Router rate for TSLAx far worse than the oracle: a deposit's TSLAx deploy leg
    // must revert, taking the whole deposit with it.
    let (tsla_mint, tsla_rate_pda) = (ctx.tsla_mint, ctx.tsla_rate_pda);
    set_router_rate(&mut ctx, tsla_mint, 300_000_000, tsla_rate_pda);

    let user = fund_user(&mut ctx, 10_000_000);
    let ix = deposit_instruction(&ctx, &user, 10_000_000, 1, deposit_remaining(&ctx));
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(
        r.is_err(),
        "deposit deploy leg worse than oracle must revert the deposit"
    );
}

#[test]
fn test_deposit_rejects_unregistered_router() {
    let mut ctx = setup_full();
    // Register a different router than the deployed mock, then fully allocate 40/60.
    let bogus_router = Address::new_unique();
    init_fund(&mut ctx, FEE_BPS, SLIPPAGE_BPS, bogus_router);
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.approved_tsla, ctx.vault_tsla);
    add_asset(&mut ctx, 0, tm, wt, vt, 4000).unwrap();
    let (nm, wn, vn) = (ctx.nvda_mint, ctx.approved_nvda, ctx.vault_nvda);
    add_asset(&mut ctx, 1, nm, wn, vn, 6000).unwrap();

    let user = fund_user(&mut ctx, 10_000_000);
    let ix = deposit_instruction(&ctx, &user, 10_000_000, 1, deposit_remaining(&ctx));
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(
        r.is_err(),
        "deposit deploying through an unregistered router must fail"
    );
}

/// Simulate a cluster restart at `slot`: prices posted at or before it must be
/// rejected until Pyth posts again.
fn set_last_restart_slot(ctx: &mut TestContext, slot: u64) {
    ctx.svm
        .set_sysvar(&solana_sysvar::last_restart_slot::LastRestartSlot {
            last_restart_slot: slot,
        });
}

/// Under Alpenglow the Clock's unix_timestamp may only advance by up to twice
/// the slot time elapsed since the parent block, so after a halt it trails real
/// time and a price published just before the halt still passes the 60-second
/// staleness check. The fund must reject any price posted at or before the
/// restart slot until Pyth posts again.
#[test]
fn test_deposit_rejects_price_from_before_restart() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);
    let user = fund_user(&mut ctx, 10_000_000);

    // Both feeds were posted at slot 1. Simulate a halt: the cluster restarts
    // at slot 3, and the timestamp has barely moved, so the pre-halt prices are
    // still well inside the 60-second window and only the restart check can
    // catch them.
    let clock = ctx.svm.get_sysvar::<Clock>();
    ctx.svm.set_sysvar(&Clock {
        slot: 5,
        unix_timestamp: PUBLISH_TIME + 2,
        ..clock
    });
    set_last_restart_slot(&mut ctx, 3);

    let ix = deposit_instruction(&ctx, &user, 10_000_000, 1, deposit_remaining(&ctx));
    let message =
        send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey())
            .expect_err("a deposit priced before the restart must fail");
    let code = managed_fund::error::FundError::PricePredatesRestart as u32 + 6000;
    assert!(
        message.to_string().contains(&format!("Custom({code})")),
        "expected PricePredatesRestart (Custom({code})), got: {message}"
    );

    // Pyth posting again after the restart reopens the fund. Fresh blockhash
    // so the retry is not byte-identical to the rejected deposit.
    set_price_feed_posted_at(&mut ctx.svm, ctx.price_feed_tsla, TSLA_PRICE, 4);
    set_price_feed_posted_at(&mut ctx.svm, ctx.price_feed_nvda, NVDA_PRICE, 4);
    ctx.svm.expire_blockhash();
    do_deposit(&mut ctx, &user, 10_000_000, 1);
    assert!(get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap() > 0);
}

#[test]
fn test_deposit_fair_pricing() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    // Alice deposits 900 USDC (first deposit 1:1 -> 900,000,000 shares), auto-deployed
    // 40/60: 1.44 TSLAx + 3.0 NVDAx. NAV = 900 USDC.
    let alice = fund_user(&mut ctx, 900_000_000);
    let alice_share = do_deposit(&mut ctx, &alice, 900_000_000, 1);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &alice_share).unwrap(),
        900_000_000
    );

    // NVDAx rises 180 -> 200. NAV rises to 0 + 1.44*250 + 3.0*200 = 960 USDC.
    set_nvda_price(&mut ctx, 20_000_000_000, 200_000_000);

    // Bob deposits 480 USDC at the higher NAV: shares = 480 * 900 / 960 = 450,000,000.
    // He pays today's price, so he does not dilute Alice's gain.
    let bob = fund_user(&mut ctx, 480_000_000);
    let bob_share = do_deposit(&mut ctx, &bob, 480_000_000, 1);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &bob_share).unwrap(),
        450_000_000
    );

    // Alice's shares are untouched; supply is the two deposits combined.
    assert_eq!(
        get_token_account_balance(&ctx.svm, &alice_share).unwrap(),
        900_000_000
    );
    let fund = read_fund(&ctx);
    assert_eq!(fund.total_shares, 1_350_000_000);
}

#[test]
fn test_rebalance() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    // Alice deposits 900 USDC, auto-deployed to 1.44 TSLAx + 3.0 NVDAx (exactly 40/60).
    let alice = fund_user(&mut ctx, 900_000_000);
    do_deposit(&mut ctx, &alice, 900_000_000, 1);

    // NVDAx rises 180 -> 200, pushing the basket to 37.5 / 62.5 by value.
    set_nvda_price(&mut ctx, 20_000_000_000, 200_000_000);

    // A stranger, neither the manager nor a depositor, rebalances. The program
    // computes the trade back to 40/60: NVDAx is $24 over its $576 target and
    // TSLAx $24 under its $384 target, so it sells 0.12 NVDAx for 24 USDC and
    // buys 0.096 TSLAx with it.
    do_rebalance(&mut ctx, 1, 0);

    // 1.44 + 0.096 = 1.536 TSLAx; 3.0 - 0.12 = 2.88 NVDAx. Now 384 / 576 = 40 / 60.
    // The USDC vault nets to zero across the two legs.
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        1_536_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap(),
        2_880_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        0
    );
}

#[test]
fn test_collect_fees() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    let user = fund_user(&mut ctx, 1_000_000_000); // 1000 USDC
    do_deposit(&mut ctx, &user, 1_000_000_000, 1);

    advance_one_year(&mut ctx);
    let manager_share = do_collect_fees(&mut ctx);

    // 1% of 1,000,000,000 = 10,000,000 fee shares.
    assert_eq!(
        get_token_account_balance(&ctx.svm, &manager_share).unwrap(),
        10_000_000
    );
}

fn withdraw_remaining(ctx: &TestContext, user: &Address) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new_readonly(ctx.asset_config(0), false),
        AccountMeta::new(ctx.vault_tsla, false),
        AccountMeta::new_readonly(ctx.tsla_mint, false),
        AccountMeta::new(derive_ata(user, &ctx.tsla_mint), false),
        AccountMeta::new_readonly(ctx.asset_config(1), false),
        AccountMeta::new(ctx.vault_nvda, false),
        AccountMeta::new_readonly(ctx.nvda_mint, false),
        AccountMeta::new(derive_ata(user, &ctx.nvda_mint), false),
    ]
}

#[test]
fn test_withdraw() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    let user = fund_user(&mut ctx, 10_000_000);
    // Deposit auto-deploys 4 USDC -> 16000 TSLAx and 6 USDC -> 33333 NVDAx, no idle USDC.
    let user_share = do_deposit(&mut ctx, &user, 10_000_000, 1);
    let shares = get_token_account_balance(&ctx.svm, &user_share).unwrap();

    // User needs token accounts for each asset paid in kind.
    let user_usdc = derive_ata(&user.pubkey(), &ctx.usdc_mint);
    create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.tsla_mint, &ctx.payer)
        .unwrap();
    create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.nvda_mint, &ctx.payer)
        .unwrap();

    let mut metas = managed_fund::accounts::WithdrawAccountConstraints {
        user: user.pubkey(),
        fund: ctx.fund_pda,
        share_mint: ctx.share_mint_pda,
        usdc_mint: ctx.usdc_mint,
        user_share_account: user_share,
        user_usdc_account: user_usdc,
        vault_usdc: ctx.vault_usdc,
        associated_token_program: ata_program_id(),
        token_program: token_program_id(),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    metas.extend(withdraw_remaining(&ctx, &user.pubkey()));

    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::Withdraw {
            shares_to_burn: shares,
            min_usdc_out: 0,
        }
        .data(),
        metas,
    );
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey()).unwrap();

    // Sole holder withdraws everything in kind: all 16000 TSLAx + 33333 NVDAx, no USDC.
    assert_eq!(get_token_account_balance(&ctx.svm, &user_usdc).unwrap(), 0);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &derive_ata(&user.pubkey(), &ctx.tsla_mint)).unwrap(),
        16_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &derive_ata(&user.pubkey(), &ctx.nvda_mint)).unwrap(),
        33_333
    );
}

#[test]
fn test_withdraw_rejects_slippage() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    let user = fund_user(&mut ctx, 10_000_000);
    let user_share = do_deposit(&mut ctx, &user, 10_000_000, 1);
    let shares = get_token_account_balance(&ctx.svm, &user_share).unwrap();

    let user_usdc = derive_ata(&user.pubkey(), &ctx.usdc_mint);
    create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.tsla_mint, &ctx.payer)
        .unwrap();
    create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.nvda_mint, &ctx.payer)
        .unwrap();

    let mut metas = managed_fund::accounts::WithdrawAccountConstraints {
        user: user.pubkey(),
        fund: ctx.fund_pda,
        share_mint: ctx.share_mint_pda,
        usdc_mint: ctx.usdc_mint,
        user_share_account: user_share,
        user_usdc_account: user_usdc,
        vault_usdc: ctx.vault_usdc,
        associated_token_program: ata_program_id(),
        token_program: token_program_id(),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    metas.extend(withdraw_remaining(&ctx, &user.pubkey()));

    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::Withdraw {
            shares_to_burn: shares,
            // The deposit was fully deployed, so the USDC payout is 0; demanding any
            // USDC back must revert.
            min_usdc_out: 1,
        }
        .data(),
        metas,
    );
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(r.is_err(), "min_usdc_out above payout must revert");
}

#[test]
fn test_deposit_rejects_incomplete_assets() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    let amount = 1_000_000u64;
    let user = fund_user(&mut ctx, amount);

    // Only one asset's accounts supplied (5) for a two-asset fund (needs 10).
    let ix = deposit_instruction(&ctx, &user, amount, 1, deposit_remaining_tsla(&ctx));
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(r.is_err(), "incomplete asset accounts must revert");
}

fn do_withdraw(ctx: &mut TestContext, user: &Keypair, shares: u64, min_usdc_out: u64) {
    create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.tsla_mint, &ctx.payer)
        .unwrap();
    create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.nvda_mint, &ctx.payer)
        .unwrap();
    let user_usdc = derive_ata(&user.pubkey(), &ctx.usdc_mint);
    let user_share = derive_ata(&user.pubkey(), &ctx.share_mint_pda);
    let mut metas = managed_fund::accounts::WithdrawAccountConstraints {
        user: user.pubkey(),
        fund: ctx.fund_pda,
        share_mint: ctx.share_mint_pda,
        usdc_mint: ctx.usdc_mint,
        user_share_account: user_share,
        user_usdc_account: user_usdc,
        vault_usdc: ctx.vault_usdc,
        associated_token_program: ata_program_id(),
        token_program: token_program_id(),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    metas.extend(withdraw_remaining(ctx, &user.pubkey()));
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::Withdraw {
            shares_to_burn: shares,
            min_usdc_out,
        }
        .data(),
        metas,
    );
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[user], &user.pubkey()).unwrap();
}

#[test]
fn test_set_weight_retire() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    // Retire NVDAx by setting its target weight to zero. Total drops to 4000, so the
    // fund is under-allocated and stops accepting deposits.
    set_weight(&mut ctx, 1, 0).unwrap();
    let fund = read_fund(&ctx);
    assert_eq!(fund.total_weight_bps, 4000);
    assert_eq!(read_asset_config(&ctx, 1).weight_bps, 0);

    let user = fund_user(&mut ctx, 100_000_000);
    let ix = deposit_instruction(&ctx, &user, 100_000_000, 1, deposit_remaining(&ctx));
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(
        r.is_err(),
        "an under-allocated (retired) fund must reject deposits"
    );

    // Reassign the freed weight to TSLAx (back to 100%); deposits reopen and now deploy
    // entirely into TSLAx, never touching the retired NVDAx vault. Fresh blockhash so
    // the retry is not byte-identical to the reverted attempt.
    set_weight(&mut ctx, 0, 10_000).unwrap();
    ctx.svm.expire_blockhash();
    do_deposit(&mut ctx, &user, 100_000_000, 1);
    // 100 USDC / 250 = 400000 TSLAx, nothing to NVDAx, no idle USDC.
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        400_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        0
    );
}

#[test]
fn test_set_weight_rejects_overflow() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);
    // TSLAx 4000 + NVDAx 6000 = 10000. Raising TSLAx to 6000 would total 12000.
    let r = set_weight(&mut ctx, 0, 6000);
    assert!(
        r.is_err(),
        "weight change pushing total over 10000 must revert"
    );
}

#[test]
fn test_set_weight_rejects_non_manager() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    let intruder = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::SetWeight { weight_bps: 0 }.data(),
        managed_fund::accounts::SetWeightAccountConstraints {
            manager: intruder.pubkey(),
            fund: ctx.fund_pda,
            asset_config: ctx.asset_config(1),
        }
        .to_account_metas(None),
    );
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&intruder],
        &intruder.pubkey(),
    );
    assert!(r.is_err(), "only the manager may set weights");
}

/// The whole lifecycle with the exact figures the book's Managed Fund chapter narrates: deposit and
/// auto-deploy, a price move, a rebalance back to target, a second depositor priced at
/// the new NAV, a year's fee, and an in-kind withdrawal.
#[test]
fn test_full_lifecycle() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    // Alice deposits 900 USDC -> 900,000,000 shares, deployed to 1.44 TSLAx + 3.0 NVDAx.
    let alice = fund_user(&mut ctx, 900_000_000);
    let alice_share = do_deposit(&mut ctx, &alice, 900_000_000, 1);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &alice_share).unwrap(),
        900_000_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        1_440_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap(),
        3_000_000
    );

    assert_holdings_match_vaults(&ctx);

    // NVDAx 180 -> 200; basket drifts to 37.5 / 62.5. Rebalance back to 40/60.
    set_nvda_price(&mut ctx, 20_000_000_000, 200_000_000);
    do_rebalance(&mut ctx, 1, 0);
    assert_holdings_match_vaults(&ctx);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        1_536_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap(),
        2_880_000
    );

    // Bob deposits 480 USDC at NAV 960 -> 450,000,000 shares, deployed 40/60.
    let bob = fund_user(&mut ctx, 480_000_000);
    let bob_share = do_deposit(&mut ctx, &bob, 480_000_000, 1);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &bob_share).unwrap(),
        450_000_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        2_304_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap(),
        4_320_000
    );

    assert_holdings_match_vaults(&ctx);

    // A year passes; the manager collects 1% of the 1,350,000,000 supply = 13,500,000.
    advance_one_year(&mut ctx);
    let manager_share = do_collect_fees(&mut ctx);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &manager_share).unwrap(),
        13_500_000
    );
    assert_eq!(read_fund(&ctx).total_shares, 1_363_500_000);

    // Alice withdraws all 900,000,000 shares in kind: her 900/1363.5 slice of each vault.
    do_withdraw(&mut ctx, &alice, 900_000_000, 0);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &derive_ata(&alice.pubkey(), &ctx.tsla_mint)).unwrap(),
        1_520_792
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &derive_ata(&alice.pubkey(), &ctx.nvda_mint)).unwrap(),
        2_851_485
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &derive_ata(&alice.pubkey(), &ctx.usdc_mint)).unwrap(),
        0
    );
    assert_eq!(read_fund(&ctx).total_shares, 463_500_000);
    assert_holdings_match_vaults(&ctx);
}

/// The first-depositor inflation attack: a dust deposit, then a donation straight
/// into the fund's USDC vault, then a 1,000 USDC deposit with no
/// `minimum_shares` floor. The program prices shares from the holdings it has
/// recorded, so the donation changes the vault's balance and nothing the
/// handler reads: the victim gets exactly the shares they would have got without
/// it, and the donated USDC stays in the vault outside the fund. Modeled on the
/// lending example's `raw_token_donation_does_not_inflate_exchange_rate`.
#[test]
fn test_donation_does_not_inflate_share_price() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);

    let donation = 1_000_000_000u64; // 1,000 USDC
    let victim_deposit = 1_000_000_000u64; // 1,000 USDC

    // The attacker deposits one minor unit. Both 40/60 deploy legs round down to
    // zero USDC and are skipped, so the minor unit stays in the USDC vault,
    // recorded, and the empty-fund deposit mints one share per minor unit.
    let attacker = fund_user(&mut ctx, 1 + donation);
    let attacker_share = do_deposit(&mut ctx, &attacker, 1, 0);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &attacker_share).unwrap(),
        1
    );
    assert_eq!(read_fund(&ctx).usdc_holdings, 1);

    // The donation lands in the vault without going through the deposit handler.
    donate_usdc(&mut ctx, &attacker, donation);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        1 + donation
    );
    assert_eq!(
        read_fund(&ctx).usdc_holdings,
        1,
        "a donation is not recorded"
    );

    // The victim deposits 1,000 USDC with no floor. Priced off the recorded NAV
    // of one minor unit against one share: 1,000,000,000 * 1 / 1 shares, the same
    // as with no donation at all. Read off the vault balance it would have been
    // 1,000,000,000 * 1 / 1,000,000,001 = 0.
    let victim = fund_user(&mut ctx, victim_deposit);
    let victim_share = do_deposit(&mut ctx, &victim, victim_deposit, 0);
    let victim_shares = get_token_account_balance(&ctx.svm, &victim_share).unwrap();
    assert_eq!(victim_shares, victim_deposit);

    // The victim redeems everything in kind: all but rounding dust of the 1,000
    // USDC they put in.
    do_withdraw(&mut ctx, &victim, victim_shares, 0);
    let victim_value = value_in_usdc(&ctx, &victim.pubkey());
    assert!(
        victim_value + 1_000 >= victim_deposit,
        "victim got back {victim_value} of {victim_deposit}"
    );

    // The attacker redeems their one share for the recorded holdings left: their
    // own minor unit plus the victim's rounding dust, under a tenth of a cent.
    // The donation is never paid out.
    do_withdraw(&mut ctx, &attacker, 1, 0);
    let attacker_value = value_in_usdc(&ctx, &attacker.pubkey());
    assert!(
        attacker_value < 1_000,
        "attacker got back {attacker_value} minor units"
    );
    assert_eq!(read_fund(&ctx).total_shares, 0);
    assert_eq!(read_fund(&ctx).usdc_holdings, 0);
    assert!(get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap() >= donation);
}

/// A deposit leg that spends USDC and buys none of its asset would mint shares
/// against no recorded value, and every later deposit would then divide by a
/// zero NAV. With TSLAx at 100% and $250, a one-minor-unit deposit swaps its
/// minor unit for zero TSLAx, so the deposit must be refused.
#[test]
fn test_deposit_rejects_leg_that_buys_nothing() {
    let mut ctx = setup_full();
    tsla_only_fund(&mut ctx);
    set_weight(&mut ctx, 0, 10_000).unwrap();

    let attacker = fund_user(&mut ctx, 1);
    let ix = deposit_instruction(&ctx, &attacker, 1, 0, deposit_remaining_tsla(&ctx));
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&attacker],
        &attacker.pubkey(),
    );
    let err = format!(
        "{:?}",
        r.expect_err("a leg that buys nothing must revert the deposit")
    );
    assert!(
        err.contains(&program_error(FundError::DepositTooSmall)),
        "{err}"
    );
    assert_eq!(read_fund(&ctx).total_shares, 0);

    // A deposit large enough to buy some TSLAx goes through.
    let user = fund_user(&mut ctx, 1_000_000);
    do_deposit_tsla_only(&mut ctx, &user, 1_000_000, 1);
    assert_eq!(read_fund(&ctx).asset_holdings[0], 4_000);
}

/// Transfer `amount` of `mint` from the donor's token account straight into a
/// fund vault, never calling a handler.
fn donate_token(
    ctx: &mut TestContext,
    donor: &Keypair,
    mint: &Address,
    vault: &Address,
    amount: u64,
) {
    let ix = spl_token::instruction::transfer(
        &spl_token::ID,
        &derive_ata(&donor.pubkey(), mint),
        vault,
        &donor.pubkey(),
        &[],
        amount,
    )
    .unwrap();
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[donor], &donor.pubkey()).unwrap();
}

/// A fund sitting at its target weights has nothing to rebalance, in either
/// direction, so nobody can trade it.
#[test]
fn test_rebalance_refuses_fund_at_target() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);
    let alice = fund_user(&mut ctx, 900_000_000);
    do_deposit(&mut ctx, &alice, 900_000_000, 1);

    assert_program_error(
        try_rebalance(&mut ctx, 1, 0),
        FundError::DriftBelowThreshold,
        "a fund at target must not sell NVDAx",
    );
    assert_program_error(
        try_rebalance(&mut ctx, 0, 1),
        FundError::DriftBelowThreshold,
        "a fund at target must not sell TSLAx",
    );
}

/// Drift smaller than the fund's threshold is not worth the slippage of a
/// trade, so the rebalance is refused.
#[test]
fn test_rebalance_refuses_drift_below_threshold() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);
    let alice = fund_user(&mut ctx, 900_000_000);
    do_deposit(&mut ctx, &alice, 900_000_000, 1);

    // NVDAx 180 -> 185: 555 of a 915 fund is 60.66%, 0.66 points over its 60%
    // target and under the two-point threshold.
    set_nvda_price(&mut ctx, 18_500_000_000, 185_000_000);
    assert_program_error(
        try_rebalance(&mut ctx, 1, 0),
        FundError::DriftBelowThreshold,
        "drift under the threshold must not trade",
    );
    assert_eq!(read_fund(&ctx).asset_holdings[1], 3_000_000);
}

/// Churn: once a rebalance has restored the weights, calling it again, in
/// either direction, finds nothing to do. Nobody can trade the fund back and
/// forth to bleed it through slippage.
#[test]
fn test_rebalance_cannot_churn() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);
    let alice = fund_user(&mut ctx, 900_000_000);
    do_deposit(&mut ctx, &alice, 900_000_000, 1);

    set_nvda_price(&mut ctx, 20_000_000_000, 200_000_000);
    do_rebalance(&mut ctx, 1, 0);
    let after_first = read_fund(&ctx);

    ctx.svm.expire_blockhash();
    assert_program_error(
        try_rebalance(&mut ctx, 1, 0),
        FundError::DriftBelowThreshold,
        "a second rebalance must find nothing to sell",
    );
    assert_program_error(
        try_rebalance(&mut ctx, 0, 1),
        FundError::DriftBelowThreshold,
        "rebalancing back the other way must be refused",
    );
    let after_retries = read_fund(&ctx);
    assert_eq!(after_retries.asset_holdings, after_first.asset_holdings);
    assert_eq!(after_retries.usdc_holdings, after_first.usdc_holdings);
}

/// A third basket asset the router can trade: its own mint (the router holds
/// the mint authority), router rate, Pyth feed, and registry approval.
struct ExtraAsset {
    mint: Address,
    approved: Address,
    vault: Address,
    rate_pda: Address,
    feed: Address,
}

fn create_routable_asset(ctx: &mut TestContext, price: i64, rate: u64) -> ExtraAsset {
    let mint = create_token_mint(&mut ctx.svm, &ctx.payer, TOKEN_DECIMALS, None).unwrap();
    let ix = spl_token::instruction::set_authority(
        &spl_token::ID,
        &mint,
        Some(&ctx.router_config_pda),
        spl_token::instruction::AuthorityType::MintTokens,
        &ctx.payer.pubkey(),
        &[],
    )
    .unwrap();
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&ctx.payer], &ctx.payer.pubkey())
        .unwrap();
    let (rate_pda, _) =
        Address::find_program_address(&[b"rate", mint.as_ref()], &ctx.router_program_id);
    set_router_rate(ctx, mint, rate, rate_pda);
    let feed = Keypair::new().pubkey();
    set_price_feed(&mut ctx.svm, feed, price);
    let (approved, _) = Address::find_program_address(
        &[b"approved_asset", ctx.registry_pda.as_ref(), mint.as_ref()],
        &ctx.fund_program_id,
    );
    let ix = Instruction::new_with_bytes(
        ctx.fund_program_id,
        &managed_fund::instruction::ApproveAsset { price_feed: feed }.data(),
        managed_fund::accounts::ApproveAssetAccountConstraints {
            authority: ctx.payer.pubkey(),
            registry: ctx.registry_pda,
            asset_mint: mint,
            approved_asset: approved,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&ctx.payer], &ctx.payer.pubkey())
        .unwrap();
    let vault = derive_ata(&ctx.fund_pda, &mint);
    ExtraAsset {
        mint,
        approved,
        vault,
        rate_pda,
        feed,
    }
}

/// The buy side must be under its target: a rebalance cannot pour the sale into
/// an asset already at or over its weight. Two assets cannot show this, since
/// when one is over its target the other is under by the same amount, so the
/// fund here holds a third, at 40/40/20.
#[test]
fn test_rebalance_refuses_buying_overweight_asset() {
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    init_fund(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);
    let third = create_routable_asset(&mut ctx, 10_000_000_000, 100_000_000); // $100
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.approved_tsla, ctx.vault_tsla);
    add_asset(&mut ctx, 0, tm, wt, vt, 4000).unwrap();
    let (nm, wn, vn) = (ctx.nvda_mint, ctx.approved_nvda, ctx.vault_nvda);
    add_asset(&mut ctx, 1, nm, wn, vn, 4000).unwrap();
    add_asset(&mut ctx, 2, third.mint, third.approved, third.vault, 2000).unwrap();

    let mut remaining = deposit_remaining(&ctx);
    remaining.extend(asset_deposit_metas(
        ctx.asset_config(2),
        third.vault,
        third.mint,
        third.rate_pda,
        third.feed,
    ));
    let alice = fund_user(&mut ctx, 1_000_000_000);
    let ix = deposit_instruction(&ctx, &alice, 1_000_000_000, 1, remaining.clone());
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&alice], &alice.pubkey()).unwrap();

    // NVDAx and the third asset both rise 30%. NVDAx is about $520 of $1,180
    // against a $472 target; the third asset $260 against $236; TSLAx $400
    // against $472. NVDAx is over by more than the threshold, the third asset
    // is over too, and only TSLAx is under.
    set_nvda_price(&mut ctx, 23_400_000_000, 234_000_000);
    set_price_feed(&mut ctx.svm, third.feed, 13_000_000_000);
    set_router_rate(&mut ctx, third.mint, 130_000_000, third.rate_pda);

    let stranger = create_wallet(&mut ctx.svm, 1_000_000_000).unwrap();
    let into_third = rebalance_instruction_with(&ctx, &stranger, 1, 2, remaining.clone());
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![into_third],
        &[&stranger],
        &stranger.pubkey(),
    );
    assert_program_error(
        r,
        FundError::NotUnderweight,
        "an over-weight asset must not be bought",
    );

    // Naming the same asset on both sides is refused outright.
    let into_itself = rebalance_instruction_with(&ctx, &stranger, 1, 1, remaining.clone());
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![into_itself],
        &[&stranger],
        &stranger.pubkey(),
    );
    assert_program_error(
        r,
        FundError::SameMint,
        "an asset must not be sold into itself",
    );

    // Selling NVDAx into TSLAx, the asset that is under, goes through.
    let into_tsla = rebalance_instruction_with(&ctx, &stranger, 1, 0, remaining);
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![into_tsla],
        &[&stranger],
        &stranger.pubkey(),
    )
    .unwrap();
    assert!(read_fund(&ctx).asset_holdings[0] > 1_600_000);
}

/// A rebalance values every asset, so it needs every asset's accounts, as a
/// deposit does.
#[test]
fn test_rebalance_rejects_incomplete_assets() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);
    let alice = fund_user(&mut ctx, 900_000_000);
    do_deposit(&mut ctx, &alice, 900_000_000, 1);
    set_nvda_price(&mut ctx, 20_000_000_000, 200_000_000);

    let stranger = create_wallet(&mut ctx.svm, 1_000_000_000).unwrap();
    let mut ix = rebalance_instruction(&ctx, &stranger, 1, 0);
    ix.accounts.truncate(ix.accounts.len() - 5);
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&stranger],
        &stranger.pubkey(),
    );
    assert_program_error(
        r,
        FundError::IncompleteAssetAccounts,
        "a rebalance missing an asset must revert",
    );
}

/// Setting a weight to zero retires an asset, and a rebalance then sells all of
/// it, however little is left: a retired asset needs no threshold, because
/// selling it to zero is a trade that can happen only once.
#[test]
fn test_rebalance_sells_retired_asset() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);
    let alice = fund_user(&mut ctx, 900_000_000);
    do_deposit(&mut ctx, &alice, 900_000_000, 1);

    // Maria retires NVDAx and moves its weight to TSLAx.
    set_weight(&mut ctx, 1, 0).unwrap();
    set_weight(&mut ctx, 0, 10_000).unwrap();

    // NVDAx falls to $1, so the 3 NVDAx left are $3 of a $363 fund: under one
    // percentage point, below the two-point threshold.
    set_nvda_price(&mut ctx, 100_000_000, 1_000_000);
    do_rebalance(&mut ctx, 1, 0);

    // All 3 NVDAx sold for 3 USDC, which bought 0.012 TSLAx.
    let fund = read_fund(&ctx);
    assert_eq!(fund.asset_holdings[1], 0);
    assert_eq!(fund.asset_holdings[0], 1_452_000);
    assert_eq!(fund.usdc_holdings, 0);
    assert_holdings_match_vaults(&ctx);

    ctx.svm.expire_blockhash();
    assert_program_error(
        try_rebalance(&mut ctx, 1, 0),
        FundError::DriftBelowThreshold,
        "a retired asset already sold has nothing left to sell",
    );
}

/// Donations can neither force a rebalance nor pay for one. Donated NVDAx is not
/// recorded, so it cannot push NVDAx over its target; donated USDC is never
/// spent, because the buy leg invests only what the sale brought in.
#[test]
fn test_rebalance_ignores_donations() {
    let mut ctx = setup_full();
    standard_fund(&mut ctx);
    let alice = fund_user(&mut ctx, 900_000_000);
    do_deposit(&mut ctx, &alice, 900_000_000, 1);

    // The donor gets NVDAx the way anyone can: deposit, then withdraw in kind.
    let donor = fund_user(&mut ctx, 200_000_000);
    let donor_share = do_deposit(&mut ctx, &donor, 100_000_000, 1);
    let donor_shares = get_token_account_balance(&ctx.svm, &donor_share).unwrap();
    do_withdraw(&mut ctx, &donor, donor_shares, 0);
    let donated_nvda =
        get_token_account_balance(&ctx.svm, &derive_ata(&donor.pubkey(), &ctx.nvda_mint)).unwrap();
    assert!(donated_nvda > 0);
    let (nvda_mint, vault_nvda, usdc_mint, vault_usdc) =
        (ctx.nvda_mint, ctx.vault_nvda, ctx.usdc_mint, ctx.vault_usdc);
    donate_token(&mut ctx, &donor, &nvda_mint, &vault_nvda, donated_nvda);
    donate_token(&mut ctx, &donor, &usdc_mint, &vault_usdc, 100_000_000);

    // Counted, the donated NVDAx would put NVDAx far over its target. Recorded
    // holdings are still 40/60, so there is nothing to rebalance.
    assert_program_error(
        try_rebalance(&mut ctx, 1, 0),
        FundError::DriftBelowThreshold,
        "donated NVDAx must not force a trade",
    );

    // A real price move does drift the fund, and the rebalance spends only
    // what its sale brought in: the donated USDC is still in the vault, outside
    // the recorded holdings.
    set_nvda_price(&mut ctx, 20_000_000_000, 200_000_000);
    ctx.svm.expire_blockhash();
    do_rebalance(&mut ctx, 1, 0);
    let fund = read_fund(&ctx);
    assert_eq!(fund.usdc_holdings, 0);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        100_000_000
    );
    assert_eq!(
        fund.asset_holdings[1] + donated_nvda,
        get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap()
    );
}

/// The threshold is fixed at creation, within bounds: a manager cannot set it
/// near zero, where the fund would trade on every small move, or so high that
/// the target weights stop describing the fund.
#[test]
fn test_initialize_rejects_threshold_out_of_range() {
    use managed_fund::instructions::initialize_fund::{
        MAX_REBALANCE_THRESHOLD_BPS, MIN_REBALANCE_THRESHOLD_BPS,
    };
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    for threshold in [
        0,
        MIN_REBALANCE_THRESHOLD_BPS - 1,
        MAX_REBALANCE_THRESHOLD_BPS + 1,
    ] {
        let ix = initialize_fund_instruction(&ctx, FEE_BPS, SLIPPAGE_BPS, threshold, router);
        let r = send_transaction_from_instructions(
            &mut ctx.svm,
            vec![ix],
            &[&ctx.manager],
            &ctx.manager.pubkey(),
        );
        assert_program_error(
            r,
            FundError::RebalanceThresholdOutOfRange,
            "a threshold outside the bounds must be rejected",
        );
        ctx.svm.expire_blockhash();
    }
    init_fund(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);
    assert_eq!(
        read_fund(&ctx).rebalance_threshold_bps,
        REBALANCE_THRESHOLD_BPS
    );
}

/// Valuation scales by each asset's decimals and each feed's exponent. TSLAx
/// here has eight decimals and a Pyth equity feed with exponent -5, while USDC
/// and NVDAx keep six decimals and NVDAx's feed keeps -8. Assuming six decimals
/// and -8 would value Alice's 1.44 TSLAx at $360 * 100 * 1,000, and Bob's
/// deposit would buy almost no shares. Scaled correctly, every figure matches
/// the six-decimal story.
#[test]
fn test_valuation_scales_by_decimals_and_exponent() {
    let mut ctx = setup_with_tsla_decimals(8);
    write_price_feed(&mut ctx.svm, ctx.price_feed_tsla, 25_000_000, -5, 1); // $250
    standard_fund(&mut ctx);
    assert_eq!(read_asset_config(&ctx, 0).decimals, 8);
    assert_eq!(read_asset_config(&ctx, 1).decimals, 6);

    // Alice's 900 USDC deploys to 1.44 TSLAx (eight decimals) and 3 NVDAx.
    let alice = fund_user(&mut ctx, 900_000_000);
    let alice_share = do_deposit(&mut ctx, &alice, 900_000_000, 1);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &alice_share).unwrap(),
        900_000_000
    );
    assert_eq!(read_fund(&ctx).asset_holdings[0], 144_000_000);
    assert_eq!(read_fund(&ctx).asset_holdings[1], 3_000_000);

    // NVDAx to $200. The rebalance computes its trade in the same units: 0.12
    // NVDAx sold for 24 USDC, which buys 0.096 TSLAx, back to 40/60.
    set_nvda_price(&mut ctx, 20_000_000_000, 200_000_000);
    do_rebalance(&mut ctx, 1, 0);
    assert_eq!(read_fund(&ctx).asset_holdings[0], 153_600_000);
    assert_eq!(read_fund(&ctx).asset_holdings[1], 2_880_000);

    // The fund is worth $960, so Bob's 480 USDC buys 450 shares.
    let bob = fund_user(&mut ctx, 480_000_000);
    let bob_share = do_deposit(&mut ctx, &bob, 480_000_000, 1);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &bob_share).unwrap(),
        450_000_000
    );
    assert_eq!(read_fund(&ctx).asset_holdings[0], 230_400_000);
    assert_eq!(read_fund(&ctx).asset_holdings[1], 4_320_000);
    assert_holdings_match_vaults(&ctx);
}
