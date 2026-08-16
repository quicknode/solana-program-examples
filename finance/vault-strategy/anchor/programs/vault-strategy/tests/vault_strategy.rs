use {
    anchor_lang::{
        solana_program::{instruction::AccountMeta, instruction::Instruction},
        system_program, AccountDeserialize, Address, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
    solana_signer::Signer,
    solana_account::Account as SolanaAccount,
};

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
/// publish_time i64 at 93. Exponent -8.
fn build_mock_price_update_account(price: i64, exponent: i32, publish_time: i64) -> Vec<u8> {
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
    data.extend_from_slice(&1u64.to_le_bytes());
    data
}

fn set_price_feed(svm: &mut LiteSVM, key: Address, price: i64) {
    let data = build_mock_price_update_account(price, -8, PUBLISH_TIME);
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
const TSLA_RATE: u64 = 250; // router usdc per token
const NVDA_RATE: u64 = 180;

const FEE_BPS: u16 = 100; // 1%
const SLIPPAGE_BPS: u16 = 100; // 1%
const STRATEGY_INDEX: u64 = 0; // strategy PDA seed: "strategy" + 0

struct TestContext {
    svm: LiteSVM,
    vault_program_id: Address,
    router_program_id: Address,
    manager: Keypair,
    payer: Keypair,
    usdc_mint: Address,
    tsla_mint: Address,
    nvda_mint: Address,
    strategy_pda: Address,
    share_mint_pda: Address,
    registry_pda: Address,
    approved_tsla: Address,
    approved_nvda: Address,
    router_config_pda: Address,
    router_authority_pda: Address,
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
            &[b"asset", self.strategy_pda.as_ref(), &[index]],
            &self.vault_program_id,
        )
        .0
    }
}

/// Mints, router (config + rates + treasury), Pyth feeds, a registry with TSLAx
/// and NVDAx approved, and all derived PDAs. Does not create the strategy.
fn setup_full() -> TestContext {
    let vault_program_id = vault_strategy::id();
    let router_program_id = mock_swap_router::id();

    let mut svm = LiteSVM::new();
    svm.add_program(
        vault_program_id,
        include_bytes!("../../../target/deploy/vault_strategy.so"),
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
    let tsla_mint = create_token_mint(&mut svm, &payer, TOKEN_DECIMALS, None).unwrap();
    let nvda_mint = create_token_mint(&mut svm, &payer, TOKEN_DECIMALS, None).unwrap();

    let (router_authority_pda, _) =
        Address::find_program_address(&[b"router_authority"], &router_program_id);

    // The router mints basket assets on swap, so it must hold their mint authority.
    for basket_mint in [&tsla_mint, &nvda_mint] {
        let ix = spl_token::instruction::set_authority(
            &spl_token::ID,
            basket_mint,
            Some(&router_authority_pda),
            spl_token::instruction::AuthorityType::MintTokens,
            &payer.pubkey(),
            &[],
        )
        .unwrap();
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey()).unwrap();
    }

    let (strategy_pda, _) = Address::find_program_address(
        &[b"strategy", STRATEGY_INDEX.to_le_bytes().as_ref()],
        &vault_program_id,
    );
    let (share_mint_pda, _) =
        Address::find_program_address(&[b"share_mint", strategy_pda.as_ref()], &vault_program_id);
    let (registry_pda, _) =
        Address::find_program_address(&[b"registry", payer.pubkey().as_ref()], &vault_program_id);
    let (approved_tsla, _) = Address::find_program_address(
        &[b"approved_asset", registry_pda.as_ref(), tsla_mint.as_ref()],
        &vault_program_id,
    );
    let (approved_nvda, _) = Address::find_program_address(
        &[b"approved_asset", registry_pda.as_ref(), nvda_mint.as_ref()],
        &vault_program_id,
    );
    let (router_config_pda, _) =
        Address::find_program_address(&[b"router_config"], &router_program_id);
    let (tsla_rate_pda, _) =
        Address::find_program_address(&[b"rate", tsla_mint.as_ref()], &router_program_id);
    let (nvda_rate_pda, _) =
        Address::find_program_address(&[b"rate", nvda_mint.as_ref()], &router_program_id);

    let vault_usdc = derive_ata(&strategy_pda, &usdc_mint);
    let vault_tsla = derive_ata(&strategy_pda, &tsla_mint);
    let vault_nvda = derive_ata(&strategy_pda, &nvda_mint);
    let router_usdc_treasury = derive_ata(&router_authority_pda, &usdc_mint);

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
            router_authority: router_authority_pda,
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
                router_authority: router_authority_pda,
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
        vault_program_id,
        &vault_strategy::instruction::InitializeRegistry {}.data(),
        vault_strategy::accounts::InitializeRegistryAccountConstraints {
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
            vault_program_id,
            &vault_strategy::instruction::ApproveAsset { price_feed: feed }.data(),
            vault_strategy::accounts::ApproveAssetAccountConstraints {
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
        vault_program_id,
        router_program_id,
        manager,
        payer,
        usdc_mint,
        tsla_mint,
        nvda_mint,
        strategy_pda,
        share_mint_pda,
        registry_pda,
        approved_tsla,
        approved_nvda,
        router_config_pda,
        router_authority_pda,
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

fn init_strategy(ctx: &mut TestContext, fee_bps: u16, slippage_bps: u16, router: Address) {
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::InitializeStrategy {
            index: STRATEGY_INDEX,
            fee_bps,
            max_slippage_bps: slippage_bps,
            swap_router: router,
        }
        .data(),
        vault_strategy::accounts::InitializeStrategyAccountConstraints {
            manager: ctx.manager.pubkey(),
            usdc_mint: ctx.usdc_mint,
            registry: ctx.registry_pda,
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            vault_usdc: ctx.vault_usdc,
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
        ctx.vault_program_id,
        &vault_strategy::instruction::AddAsset { weight_bps }.data(),
        vault_strategy::accounts::AddAssetAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
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

/// init strategy + add TSLAx (index 0, 40%) + NVDAx (index 1, 60%).
fn standard_strategy(ctx: &mut TestContext) {
    let router = ctx.router_program_id;
    init_strategy(ctx, FEE_BPS, SLIPPAGE_BPS, router);
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

/// remaining_accounts for a deposit into the two-asset standard strategy.
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
    vault_strategy::accounts::DepositAccountConstraints {
        depositor: user.pubkey(),
        strategy: ctx.strategy_pda,
        share_mint: ctx.share_mint_pda,
        usdc_mint: ctx.usdc_mint,
        depositor_usdc_account: derive_ata(&user.pubkey(), &ctx.usdc_mint),
        depositor_share_account: derive_ata(&user.pubkey(), &ctx.share_mint_pda),
        vault_usdc: ctx.vault_usdc,
        router_config: ctx.router_config_pda,
        router_usdc_treasury: ctx.router_usdc_treasury,
        router_authority: ctx.router_authority_pda,
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
        ctx.vault_program_id,
        &vault_strategy::instruction::Deposit {
            usdc_amount,
            minimum_shares,
        }
        .data(),
        metas,
    )
}

/// Deposit into the two-asset standard strategy, auto-deploying at the target weights.
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

/// Deposit into a TSLAx-only strategy.
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
            router_authority: ctx.router_authority_pda,
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
        ctx.vault_program_id,
        &vault_strategy::instruction::SetWeight { weight_bps }.data(),
        vault_strategy::accounts::SetWeightAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
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

/// init strategy + add only TSLAx at 40%, so total weight is 4000: the strategy is
/// under-allocated and rejects deposits until its weights reach 100%.
fn tsla_only_strategy(ctx: &mut TestContext) {
    let router = ctx.router_program_id;
    init_strategy(ctx, FEE_BPS, SLIPPAGE_BPS, router);
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.approved_tsla, ctx.vault_tsla);
    add_asset(ctx, 0, tm, wt, vt, 4000).unwrap();
}

fn read_strategy(ctx: &TestContext) -> vault_strategy::state::Strategy {
    let account = ctx.svm.get_account(&ctx.strategy_pda).unwrap();
    vault_strategy::state::Strategy::try_deserialize(&mut &account.data[..]).unwrap()
}

fn read_asset_config(ctx: &TestContext, index: u8) -> vault_strategy::state::AssetConfig {
    let account = ctx.svm.get_account(&ctx.asset_config(index)).unwrap();
    vault_strategy::state::AssetConfig::try_deserialize(&mut &account.data[..]).unwrap()
}

/// (mint, asset_config, price_feed, vault, rate_pda) for an asset in the two-asset
/// standard strategy: index 0 is TSLAx, index 1 is NVDAx.
fn asset_accounts(ctx: &TestContext, index: u8) -> (Address, Address, Address, Address, Address) {
    match index {
        0 => (
            ctx.tsla_mint,
            ctx.asset_config(0),
            ctx.price_feed_tsla,
            ctx.vault_tsla,
            ctx.tsla_rate_pda,
        ),
        1 => (
            ctx.nvda_mint,
            ctx.asset_config(1),
            ctx.price_feed_nvda,
            ctx.vault_nvda,
            ctx.nvda_rate_pda,
        ),
        _ => panic!("unknown asset index {index}"),
    }
}

fn do_rebalance(
    ctx: &mut TestContext,
    sell_index: u8,
    buy_index: u8,
    sell_amount: u64,
    usdc_to_invest: u64,
) {
    let (sell_mint, sell_config, sell_feed, vault_sell, sell_rate) =
        asset_accounts(ctx, sell_index);
    let (buy_mint, buy_config, buy_feed, vault_buy, buy_rate) = asset_accounts(ctx, buy_index);
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Rebalance {
            sell_amount,
            usdc_to_invest,
        }
        .data(),
        vault_strategy::accounts::RebalanceAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            sell_mint,
            buy_mint,
            sell_config,
            buy_config,
            sell_price_feed: sell_feed,
            buy_price_feed: buy_feed,
            vault_sell,
            vault_buy,
            vault_usdc: ctx.vault_usdc,
            sell_rate,
            buy_rate,
            router_config: ctx.router_config_pda,
            router_usdc_treasury: ctx.router_usdc_treasury,
            router_authority: ctx.router_authority_pda,
            swap_router_program: ctx.router_program_id,
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
    .unwrap();
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
        ctx.vault_program_id,
        &vault_strategy::instruction::CollectFees {}.data(),
        vault_strategy::accounts::CollectFeesAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
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
    standard_strategy(&mut ctx);

    let account = ctx.svm.get_account(&ctx.strategy_pda).unwrap();
    let strategy =
        vault_strategy::state::Strategy::try_deserialize(&mut &account.data[..]).unwrap();
    assert_eq!(strategy.asset_count, 2);
    assert_eq!(strategy.total_weight_bps, 10_000);
    assert_eq!(strategy.fee_bps, FEE_BPS);
    assert_eq!(strategy.max_slippage_bps, SLIPPAGE_BPS);
    assert_eq!(strategy.registry, ctx.registry_pda);

    let cfg0 = ctx.svm.get_account(&ctx.asset_config(0)).unwrap();
    let asset0 = vault_strategy::state::AssetConfig::try_deserialize(&mut &cfg0.data[..]).unwrap();
    assert_eq!(asset0.mint, ctx.tsla_mint);
    assert_eq!(asset0.price_feed, ctx.price_feed_tsla);
    assert_eq!(asset0.vault, ctx.vault_tsla);
    assert_eq!(asset0.weight_bps, 4000);
}

#[test]
fn test_add_asset_rejects_unapproved() {
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    init_strategy(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);

    // A mint that was never approved: its approved_asset PDA does not exist.
    let rogue_mint = create_token_mint(&mut ctx.svm, &ctx.payer, TOKEN_DECIMALS, None).unwrap();
    let (rogue_entry, _) = Address::find_program_address(
        &[
            b"approved_asset",
            ctx.registry_pda.as_ref(),
            rogue_mint.as_ref(),
        ],
        &ctx.vault_program_id,
    );
    let rogue_vault = derive_ata(&ctx.strategy_pda, &rogue_mint);

    let result = add_asset(&mut ctx, 0, rogue_mint, rogue_entry, rogue_vault, 5000);
    assert!(result.is_err(), "adding an unapproved mint must fail");
}

#[test]
fn test_add_asset_rejects_weight_overflow() {
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    init_strategy(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);
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
        &ctx.vault_program_id,
    );
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::ApproveAsset {
            price_feed: Keypair::new().pubkey(),
        }
        .data(),
        vault_strategy::accounts::ApproveAssetAccountConstraints {
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
    init_strategy(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);

    // Fill the basket to the cap: 16 assets at 625 bps each = 10000.
    for index in 0..16u8 {
        let (mint, entry) = create_and_approve_mint(&mut ctx);
        let vault = derive_ata(&ctx.strategy_pda, &mint);
        add_asset(&mut ctx, index, mint, entry, vault, 625).unwrap();
    }
    let strategy = read_strategy(&ctx);
    assert_eq!(strategy.asset_count, 16);
    assert_eq!(strategy.total_weight_bps, 10_000);

    // The 17th asset must be rejected. Weight 0 so the cap, not the weight sum, trips.
    let (mint, entry) = create_and_approve_mint(&mut ctx);
    let vault = derive_ata(&ctx.strategy_pda, &mint);
    let result = add_asset(&mut ctx, 16, mint, entry, vault, 0);
    assert!(result.is_err(), "adding beyond MAX_ASSETS must revert");
}

#[test]
fn test_initialize_rejects_excessive_fee() {
    let mut ctx = setup_full();
    let excessive = vault_strategy::instructions::initialize_strategy::MAX_FEE_BPS + 1;
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::InitializeStrategy {
            index: STRATEGY_INDEX,
            fee_bps: excessive,
            max_slippage_bps: SLIPPAGE_BPS,
            swap_router: ctx.router_program_id,
        }
        .data(),
        vault_strategy::accounts::InitializeStrategyAccountConstraints {
            manager: ctx.manager.pubkey(),
            usdc_mint: ctx.usdc_mint,
            registry: ctx.registry_pda,
            strategy: ctx.strategy_pda,
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
    let excessive = vault_strategy::instructions::initialize_strategy::MAX_SLIPPAGE_BPS + 1;
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::InitializeStrategy {
            index: STRATEGY_INDEX,
            fee_bps: FEE_BPS,
            max_slippage_bps: excessive,
            swap_router: ctx.router_program_id,
        }
        .data(),
        vault_strategy::accounts::InitializeStrategyAccountConstraints {
            manager: ctx.manager.pubkey(),
            usdc_mint: ctx.usdc_mint,
            registry: ctx.registry_pda,
            strategy: ctx.strategy_pda,
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
    standard_strategy(&mut ctx);

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
    // TSLAx at 40% only: total weight is 4000, so the strategy is not investable yet.
    tsla_only_strategy(&mut ctx);

    let user = fund_user(&mut ctx, 10_000_000);
    let ix = deposit_instruction(&ctx, &user, 10_000_000, 1, deposit_remaining_tsla(&ctx));
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(
        r.is_err(),
        "deposit into an under-allocated strategy must revert"
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
    standard_strategy(&mut ctx);

    // Router rate for TSLAx far worse than the oracle: a deposit's TSLAx deploy leg
    // must revert, taking the whole deposit with it.
    let (tsla_mint, tsla_rate_pda) = (ctx.tsla_mint, ctx.tsla_rate_pda);
    set_router_rate(&mut ctx, tsla_mint, 300, tsla_rate_pda);

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
    init_strategy(&mut ctx, FEE_BPS, SLIPPAGE_BPS, bogus_router);
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

#[test]
fn test_deposit_fair_pricing() {
    let mut ctx = setup_full();
    standard_strategy(&mut ctx);

    // Alice deposits 900 USDC (first deposit 1:1 -> 900,000,000 shares), auto-deployed
    // 40/60: 1.44 TSLAx + 3.0 NVDAx. NAV = 900 USDC.
    let alice = fund_user(&mut ctx, 900_000_000);
    let alice_share = do_deposit(&mut ctx, &alice, 900_000_000, 1);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &alice_share).unwrap(),
        900_000_000
    );

    // NVDAx rises 180 -> 200. NAV rises to 0 + 1.44*250 + 3.0*200 = 960 USDC.
    set_nvda_price(&mut ctx, 20_000_000_000, 200);

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
    let strategy = read_strategy(&ctx);
    assert_eq!(strategy.total_shares, 1_350_000_000);
}

#[test]
fn test_rebalance() {
    let mut ctx = setup_full();
    standard_strategy(&mut ctx);

    // Alice deposits 900 USDC, auto-deployed to 1.44 TSLAx + 3.0 NVDAx (exactly 40/60).
    let alice = fund_user(&mut ctx, 900_000_000);
    do_deposit(&mut ctx, &alice, 900_000_000, 1);

    // NVDAx rises 180 -> 200, pushing the basket to 37.5 / 62.5 by value.
    set_nvda_price(&mut ctx, 20_000_000_000, 200);

    // Rebalance back toward 40/60: sell 0.12 NVDAx for 24 USDC, buy 0.096 TSLAx with it.
    do_rebalance(&mut ctx, 1, 0, 120_000, 24_000_000);

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
    standard_strategy(&mut ctx);

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
    standard_strategy(&mut ctx);

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

    let mut metas = vault_strategy::accounts::WithdrawAccountConstraints {
        user: user.pubkey(),
        strategy: ctx.strategy_pda,
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
        ctx.vault_program_id,
        &vault_strategy::instruction::Withdraw {
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
    standard_strategy(&mut ctx);

    let user = fund_user(&mut ctx, 10_000_000);
    let user_share = do_deposit(&mut ctx, &user, 10_000_000, 1);
    let shares = get_token_account_balance(&ctx.svm, &user_share).unwrap();

    let user_usdc = derive_ata(&user.pubkey(), &ctx.usdc_mint);
    create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.tsla_mint, &ctx.payer)
        .unwrap();
    create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.nvda_mint, &ctx.payer)
        .unwrap();

    let mut metas = vault_strategy::accounts::WithdrawAccountConstraints {
        user: user.pubkey(),
        strategy: ctx.strategy_pda,
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
        ctx.vault_program_id,
        &vault_strategy::instruction::Withdraw {
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
    standard_strategy(&mut ctx);

    let amount = 1_000_000u64;
    let user = fund_user(&mut ctx, amount);

    // Only one asset's accounts supplied (5) for a two-asset strategy (needs 10).
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
    let mut metas = vault_strategy::accounts::WithdrawAccountConstraints {
        user: user.pubkey(),
        strategy: ctx.strategy_pda,
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
        ctx.vault_program_id,
        &vault_strategy::instruction::Withdraw {
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
    standard_strategy(&mut ctx);

    // Retire NVDAx by setting its target weight to zero. Total drops to 4000, so the
    // strategy is under-allocated and stops accepting deposits.
    set_weight(&mut ctx, 1, 0).unwrap();
    let strategy = read_strategy(&ctx);
    assert_eq!(strategy.total_weight_bps, 4000);
    assert_eq!(read_asset_config(&ctx, 1).weight_bps, 0);

    let user = fund_user(&mut ctx, 100_000_000);
    let ix = deposit_instruction(&ctx, &user, 100_000_000, 1, deposit_remaining(&ctx));
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(
        r.is_err(),
        "an under-allocated (retired) strategy must reject deposits"
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
    standard_strategy(&mut ctx);
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
    standard_strategy(&mut ctx);

    let intruder = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::SetWeight { weight_bps: 0 }.data(),
        vault_strategy::accounts::SetWeightAccountConstraints {
            manager: intruder.pubkey(),
            strategy: ctx.strategy_pda,
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

/// The whole lifecycle with the exact figures the video script narrates: deposit and
/// auto-deploy, a price move, a rebalance back to target, a second depositor priced at
/// the new NAV, a year's fee, and an in-kind withdrawal.
#[test]
fn test_full_lifecycle() {
    let mut ctx = setup_full();
    standard_strategy(&mut ctx);

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

    // NVDAx 180 -> 200; basket drifts to 37.5 / 62.5. Rebalance back to 40/60.
    set_nvda_price(&mut ctx, 20_000_000_000, 200);
    do_rebalance(&mut ctx, 1, 0, 120_000, 24_000_000);
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

    // A year passes; the manager collects 1% of the 1,350,000,000 supply = 13,500,000.
    advance_one_year(&mut ctx);
    let manager_share = do_collect_fees(&mut ctx);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &manager_share).unwrap(),
        13_500_000
    );
    assert_eq!(read_strategy(&ctx).total_shares, 1_363_500_000);

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
    assert_eq!(read_strategy(&ctx).total_shares, 463_500_000);
}
