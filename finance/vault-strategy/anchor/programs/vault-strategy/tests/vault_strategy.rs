use {
    anchor_lang::{
        solana_program::{
            clock::Clock, instruction::AccountMeta, instruction::Instruction, pubkey::Pubkey,
            system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_account::Account as SolanaAccount,
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
    solana_signer::Signer,
};

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

fn pyth_receiver_program_id() -> Pubkey {
    "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ"
        .parse()
        .unwrap()
}

fn derive_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    let (ata, _bump) = Pubkey::find_program_address(
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

fn set_price_feed(svm: &mut LiteSVM, key: Pubkey, price: i64) {
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

struct TestContext {
    svm: LiteSVM,
    vault_program_id: Pubkey,
    router_program_id: Pubkey,
    manager: Keypair,
    payer: Keypair,
    usdc_mint: Pubkey,
    tsla_mint: Pubkey,
    nvda_mint: Pubkey,
    strategy_pda: Pubkey,
    share_mint_pda: Pubkey,
    registry_pda: Pubkey,
    whitelist_tsla: Pubkey,
    whitelist_nvda: Pubkey,
    router_config_pda: Pubkey,
    router_authority_pda: Pubkey,
    tsla_rate_pda: Pubkey,
    nvda_rate_pda: Pubkey,
    vault_usdc: Pubkey,
    vault_tsla: Pubkey,
    vault_nvda: Pubkey,
    router_usdc_treasury: Pubkey,
    price_feed_tsla: Pubkey,
    price_feed_nvda: Pubkey,
}

impl TestContext {
    fn asset_config(&self, index: u8) -> Pubkey {
        Pubkey::find_program_address(
            &[b"asset", self.strategy_pda.as_ref(), &[index]],
            &self.vault_program_id,
        )
        .0
    }
}

/// Mints, router (config + rates + treasury), Pyth feeds, a registry with TSLAx
/// and NVDAx whitelisted, and all derived PDAs. Does not create the strategy.
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
        Pubkey::find_program_address(&[b"router_authority"], &router_program_id);

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

    let (strategy_pda, _) =
        Pubkey::find_program_address(&[b"strategy", manager.pubkey().as_ref()], &vault_program_id);
    let (share_mint_pda, _) =
        Pubkey::find_program_address(&[b"share_mint", strategy_pda.as_ref()], &vault_program_id);
    let (registry_pda, _) =
        Pubkey::find_program_address(&[b"registry", payer.pubkey().as_ref()], &vault_program_id);
    let (whitelist_tsla, _) = Pubkey::find_program_address(
        &[b"whitelist", registry_pda.as_ref(), tsla_mint.as_ref()],
        &vault_program_id,
    );
    let (whitelist_nvda, _) = Pubkey::find_program_address(
        &[b"whitelist", registry_pda.as_ref(), nvda_mint.as_ref()],
        &vault_program_id,
    );
    let (router_config_pda, _) =
        Pubkey::find_program_address(&[b"router_config"], &router_program_id);
    let (tsla_rate_pda, _) =
        Pubkey::find_program_address(&[b"rate", tsla_mint.as_ref()], &router_program_id);
    let (nvda_rate_pda, _) =
        Pubkey::find_program_address(&[b"rate", nvda_mint.as_ref()], &router_program_id);

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
            system_program: system_program::id(),
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
                system_program: system_program::id(),
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

    // Registry with both basket assets whitelisted, bound to their feeds.
    let init_registry_ix = Instruction::new_with_bytes(
        vault_program_id,
        &vault_strategy::instruction::InitializeRegistry {}.data(),
        vault_strategy::accounts::InitializeRegistryAccountConstraints {
            authority: payer.pubkey(),
            registry: registry_pda,
            system_program: system_program::id(),
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
        (tsla_mint, price_feed_tsla, whitelist_tsla),
        (nvda_mint, price_feed_nvda, whitelist_nvda),
    ] {
        let ix = Instruction::new_with_bytes(
            vault_program_id,
            &vault_strategy::instruction::WhitelistAsset { price_feed: feed }.data(),
            vault_strategy::accounts::WhitelistAssetAccountConstraints {
                authority: payer.pubkey(),
                registry: registry_pda,
                asset_mint: mint,
                whitelist_entry: entry,
                system_program: system_program::id(),
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
        whitelist_tsla,
        whitelist_nvda,
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

fn init_strategy(ctx: &mut TestContext, fee_bps: u16, slippage_bps: u16, router: Pubkey) {
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::InitializeStrategy {
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
            system_program: system_program::id(),
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
    mint: Pubkey,
    whitelist_entry: Pubkey,
    vault: Pubkey,
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
            whitelist_entry,
            asset_config,
            vault_asset: vault,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
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
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.whitelist_tsla, ctx.vault_tsla);
    add_asset(ctx, 0, tm, wt, vt, 4000).unwrap();
    let (nm, wn, vn) = (ctx.nvda_mint, ctx.whitelist_nvda, ctx.vault_nvda);
    add_asset(ctx, 1, nm, wn, vn, 6000).unwrap();
}

/// remaining_accounts for deposit: [asset_config, vault, price_feed] per asset.
fn deposit_remaining(ctx: &TestContext) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new_readonly(ctx.asset_config(0), false),
        AccountMeta::new_readonly(ctx.vault_tsla, false),
        AccountMeta::new_readonly(ctx.price_feed_tsla, false),
        AccountMeta::new_readonly(ctx.asset_config(1), false),
        AccountMeta::new_readonly(ctx.vault_nvda, false),
        AccountMeta::new_readonly(ctx.price_feed_nvda, false),
    ]
}

fn do_deposit(
    ctx: &mut TestContext,
    user: &Keypair,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Pubkey {
    let user_usdc = derive_ata(&user.pubkey(), &ctx.usdc_mint);
    let user_share = derive_ata(&user.pubkey(), &ctx.share_mint_pda);

    let mut metas = vault_strategy::accounts::DepositAccountConstraints {
        depositor: user.pubkey(),
        strategy: ctx.strategy_pda,
        share_mint: ctx.share_mint_pda,
        usdc_mint: ctx.usdc_mint,
        depositor_usdc_account: user_usdc,
        depositor_share_account: user_share,
        vault_usdc: ctx.vault_usdc,
        associated_token_program: ata_program_id(),
        token_program: token_program_id(),
        system_program: system_program::id(),
    }
    .to_account_metas(None);
    metas.extend(deposit_remaining(ctx));

    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Deposit {
            usdc_amount,
            minimum_shares,
        }
        .data(),
        metas,
    );
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[user], &user.pubkey()).unwrap();
    user_share
}

fn invest_ix(
    ctx: &TestContext,
    mint: Pubkey,
    config: Pubkey,
    feed: Pubkey,
    vault: Pubkey,
    rate: Pubkey,
    usdc_amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Invest { usdc_amount }.data(),
        vault_strategy::accounts::InvestAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            asset_config: config,
            usdc_mint: ctx.usdc_mint,
            asset_mint: mint,
            price_feed: feed,
            vault_usdc: ctx.vault_usdc,
            vault_asset: vault,
            asset_rate: rate,
            router_config: ctx.router_config_pda,
            router_usdc_treasury: ctx.router_usdc_treasury,
            router_authority: ctx.router_authority_pda,
            swap_router_program: ctx.router_program_id,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
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
fn test_add_asset_rejects_non_whitelisted() {
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    init_strategy(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);

    // A mint that was never whitelisted: its whitelist_entry PDA does not exist.
    let rogue_mint = create_token_mint(&mut ctx.svm, &ctx.payer, TOKEN_DECIMALS, None).unwrap();
    let (rogue_entry, _) = Pubkey::find_program_address(
        &[b"whitelist", ctx.registry_pda.as_ref(), rogue_mint.as_ref()],
        &ctx.vault_program_id,
    );
    let rogue_vault = derive_ata(&ctx.strategy_pda, &rogue_mint);

    let result = add_asset(&mut ctx, 0, rogue_mint, rogue_entry, rogue_vault, 5000);
    assert!(result.is_err(), "adding a non-whitelisted mint must fail");
}

#[test]
fn test_add_asset_rejects_weight_overflow() {
    let mut ctx = setup_full();
    let router = ctx.router_program_id;
    init_strategy(&mut ctx, FEE_BPS, SLIPPAGE_BPS, router);
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.whitelist_tsla, ctx.vault_tsla);
    add_asset(&mut ctx, 0, tm, wt, vt, 6000).unwrap();
    let (nm, wn, vn) = (ctx.nvda_mint, ctx.whitelist_nvda, ctx.vault_nvda);
    let result = add_asset(&mut ctx, 1, nm, wn, vn, 6000);
    assert!(result.is_err(), "weights over 10000 bps must fail");
}

#[test]
fn test_initialize_rejects_excessive_fee() {
    let mut ctx = setup_full();
    let excessive = vault_strategy::instructions::initialize_strategy::MAX_FEE_BPS + 1;
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::InitializeStrategy {
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
            system_program: system_program::id(),
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
            system_program: system_program::id(),
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

    assert_eq!(
        get_token_account_balance(&ctx.svm, &user_share).unwrap(),
        amount
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        amount
    );
}

#[test]
fn test_invest() {
    let mut ctx = setup_full();
    standard_strategy(&mut ctx);

    let user = fund_user(&mut ctx, 10_000_000);
    do_deposit(&mut ctx, &user, 10_000_000, 1);

    let ix = invest_ix(
        &ctx,
        ctx.tsla_mint,
        ctx.asset_config(0),
        ctx.price_feed_tsla,
        ctx.vault_tsla,
        ctx.tsla_rate_pda,
        4_000_000,
    );
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    )
    .unwrap();

    // 4 USDC / 250 = 16000 TSLAx
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        16_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap(),
        6_000_000
    );
}

#[test]
fn test_invest_rejects_slippage() {
    let mut ctx = setup_full();
    standard_strategy(&mut ctx);
    let user = fund_user(&mut ctx, 10_000_000);
    do_deposit(&mut ctx, &user, 10_000_000, 1);

    // Make the router quote far worse than the oracle: rate 300 vs Pyth-implied 250.
    let bad_rate_ix = Instruction::new_with_bytes(
        ctx.router_program_id,
        &mock_swap_router::instruction::SetRate {
            mint: ctx.tsla_mint,
            usdc_per_token: 300,
        }
        .data(),
        mock_swap_router::accounts::SetRateAccountConstraints {
            authority: ctx.payer.pubkey(),
            router_config: ctx.router_config_pda,
            asset_mint: ctx.tsla_mint,
            usdc_mint: ctx.usdc_mint,
            asset_rate: ctx.tsla_rate_pda,
            router_authority: ctx.router_authority_pda,
            router_usdc_treasury: ctx.router_usdc_treasury,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![bad_rate_ix],
        &[&ctx.payer],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    let ix = invest_ix(
        &ctx,
        ctx.tsla_mint,
        ctx.asset_config(0),
        ctx.price_feed_tsla,
        ctx.vault_tsla,
        ctx.tsla_rate_pda,
        4_000_000,
    );
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    );
    assert!(
        r.is_err(),
        "swap worse than oracle beyond tolerance must revert"
    );
}

#[test]
fn test_invest_rejects_unregistered_router() {
    let mut ctx = setup_full();
    // Register a different router than the deployed mock.
    let bogus_router = Pubkey::new_unique();
    init_strategy(&mut ctx, FEE_BPS, SLIPPAGE_BPS, bogus_router);
    let (tm, wt, vt) = (ctx.tsla_mint, ctx.whitelist_tsla, ctx.vault_tsla);
    add_asset(&mut ctx, 0, tm, wt, vt, 4000).unwrap();

    let ix = invest_ix(
        &ctx,
        ctx.tsla_mint,
        ctx.asset_config(0),
        ctx.price_feed_tsla,
        ctx.vault_tsla,
        ctx.tsla_rate_pda,
        1_000_000,
    );
    let r = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    );
    assert!(
        r.is_err(),
        "invest through an unregistered router must fail"
    );
}

#[test]
fn test_deposit_after_invest() {
    let mut ctx = setup_full();
    standard_strategy(&mut ctx);

    // Alice deposits 10 USDC (1:1 -> 10,000,000 shares).
    let alice = fund_user(&mut ctx, 10_000_000);
    do_deposit(&mut ctx, &alice, 10_000_000, 1);

    // Manager invests 4 USDC into TSLAx.
    let ix = invest_ix(
        &ctx,
        ctx.tsla_mint,
        ctx.asset_config(0),
        ctx.price_feed_tsla,
        ctx.vault_tsla,
        ctx.tsla_rate_pda,
        4_000_000,
    );
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    )
    .unwrap();

    // NAV unchanged at 10 USDC (6 USDC + 16000 TSLAx * $250 = 6 + 4). Bob deposits 5 USDC -> 5,000,000 shares.
    let bob = fund_user(&mut ctx, 5_000_000);
    let bob_share = do_deposit(&mut ctx, &bob, 5_000_000, 1);
    assert_eq!(
        get_token_account_balance(&ctx.svm, &bob_share).unwrap(),
        5_000_000
    );
}

#[test]
fn test_rebalance() {
    let mut ctx = setup_full();
    standard_strategy(&mut ctx);
    let user = fund_user(&mut ctx, 100_000_000);
    do_deposit(&mut ctx, &user, 100_000_000, 1);

    // Invest 40 USDC -> TSLAx (160000), 30 USDC -> NVDAx (166666).
    let i1 = invest_ix(
        &ctx,
        ctx.tsla_mint,
        ctx.asset_config(0),
        ctx.price_feed_tsla,
        ctx.vault_tsla,
        ctx.tsla_rate_pda,
        40_000_000,
    );
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![i1],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    )
    .unwrap();
    let i2 = invest_ix(
        &ctx,
        ctx.nvda_mint,
        ctx.asset_config(1),
        ctx.price_feed_nvda,
        ctx.vault_nvda,
        ctx.nvda_rate_pda,
        30_000_000,
    );
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![i2],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    )
    .unwrap();

    let tsla_before = get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap();
    let nvda_before = get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap();

    // Sell 100000 TSLAx -> 25 USDC, buy NVDAx with 25 USDC -> 138888.
    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Rebalance {
            sell_amount: 100_000,
            usdc_to_invest: 25_000_000,
        }
        .data(),
        vault_strategy::accounts::RebalanceAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            sell_mint: ctx.tsla_mint,
            buy_mint: ctx.nvda_mint,
            sell_config: ctx.asset_config(0),
            buy_config: ctx.asset_config(1),
            sell_price_feed: ctx.price_feed_tsla,
            buy_price_feed: ctx.price_feed_nvda,
            vault_sell: ctx.vault_tsla,
            vault_buy: ctx.vault_nvda,
            vault_usdc: ctx.vault_usdc,
            sell_rate: ctx.tsla_rate_pda,
            buy_rate: ctx.nvda_rate_pda,
            router_config: ctx.router_config_pda,
            router_usdc_treasury: ctx.router_usdc_treasury,
            router_authority: ctx.router_authority_pda,
            swap_router_program: ctx.router_program_id,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
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

    assert_eq!(
        get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap(),
        tsla_before - 100_000
    );
    assert!(get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap() > nvda_before);
}

#[test]
fn test_collect_fees() {
    let mut ctx = setup_full();
    standard_strategy(&mut ctx);

    let user = fund_user(&mut ctx, 1_000_000_000); // 1000 USDC
    do_deposit(&mut ctx, &user, 1_000_000_000, 1);

    // Advance a full year.
    let clock = ctx.svm.get_sysvar::<Clock>();
    ctx.svm.set_sysvar(&Clock {
        slot: clock.slot + 1_000_000,
        epoch_start_timestamp: clock.epoch_start_timestamp,
        epoch: clock.epoch,
        leader_schedule_epoch: clock.leader_schedule_epoch,
        unix_timestamp: PUBLISH_TIME + SECONDS_PER_YEAR,
    });

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
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&ctx.payer], &ctx.payer.pubkey())
        .unwrap();

    // 1% of 1,000,000,000 = 10,000,000 fee shares.
    assert_eq!(
        get_token_account_balance(&ctx.svm, &manager_share).unwrap(),
        10_000_000
    );
}

fn withdraw_remaining(ctx: &TestContext, user: &Pubkey) -> Vec<AccountMeta> {
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
    let user_share = do_deposit(&mut ctx, &user, 10_000_000, 1);
    let shares = get_token_account_balance(&ctx.svm, &user_share).unwrap();

    // Manager invests 4 USDC into TSLAx so the vault holds a mix.
    let ix = invest_ix(
        &ctx,
        ctx.tsla_mint,
        ctx.asset_config(0),
        ctx.price_feed_tsla,
        ctx.vault_tsla,
        ctx.tsla_rate_pda,
        4_000_000,
    );
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![ix],
        &[&ctx.manager],
        &ctx.manager.pubkey(),
    )
    .unwrap();

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
        system_program: system_program::id(),
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

    // Sole holder withdraws everything: 6 USDC + all 16000 TSLAx back.
    assert_eq!(
        get_token_account_balance(&ctx.svm, &user_usdc).unwrap(),
        6_000_000
    );
    assert_eq!(
        get_token_account_balance(&ctx.svm, &derive_ata(&user.pubkey(), &ctx.tsla_mint)).unwrap(),
        16_000
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
        system_program: system_program::id(),
    }
    .to_account_metas(None);
    metas.extend(withdraw_remaining(&ctx, &user.pubkey()));

    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Withdraw {
            shares_to_burn: shares,
            min_usdc_out: 10_000_001, // more than available
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
    let user_usdc = derive_ata(&user.pubkey(), &ctx.usdc_mint);
    let user_share = derive_ata(&user.pubkey(), &ctx.share_mint_pda);

    // Only one asset's accounts supplied (3) for a two-asset strategy (needs 6).
    let mut metas = vault_strategy::accounts::DepositAccountConstraints {
        depositor: user.pubkey(),
        strategy: ctx.strategy_pda,
        share_mint: ctx.share_mint_pda,
        usdc_mint: ctx.usdc_mint,
        depositor_usdc_account: user_usdc,
        depositor_share_account: user_share,
        vault_usdc: ctx.vault_usdc,
        associated_token_program: ata_program_id(),
        token_program: token_program_id(),
        system_program: system_program::id(),
    }
    .to_account_metas(None);
    metas.push(AccountMeta::new_readonly(ctx.asset_config(0), false));
    metas.push(AccountMeta::new_readonly(ctx.vault_tsla, false));
    metas.push(AccountMeta::new_readonly(ctx.price_feed_tsla, false));

    let ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Deposit {
            usdc_amount: amount,
            minimum_shares: 1,
        }
        .data(),
        metas,
    );
    let r = send_transaction_from_instructions(&mut ctx.svm, vec![ix], &[&user], &user.pubkey());
    assert!(r.is_err(), "incomplete asset accounts must revert");
}
