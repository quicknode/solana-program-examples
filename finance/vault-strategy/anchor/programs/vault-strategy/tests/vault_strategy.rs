use {
    anchor_lang::{
        solana_program::{clock::Clock, instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_account::Account as SolanaAccount,
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account, send_transaction_from_instructions,
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

/// Build a mock PriceUpdateV2 account data buffer.
/// Layout (matching pyth-solana-receiver-sdk PriceUpdateV2):
///   [0..8]   discriminator  sha256("account:PriceUpdateV2")[..8]
///   [8..40]  write_authority (Pubkey, 32 bytes)
///   [40]     verification_level (1 byte enum: Full = 1)
///   [41..73] feed_id ([u8;32])
///   [73..81] price (i64 LE)
///   [81..89] conf (u64 LE)
///   [89..93] exponent (i32 LE)
///   [93..101] publish_time (i64 LE)
///   [101..109] prev_publish_time (i64 LE)
///   [109..117] ema_price (i64 LE)
///   [117..125] ema_conf (u64 LE)
///   [125..133] posted_slot (u64 LE)
fn build_mock_price_update_account(price: i64, exponent: i32, publish_time: i64) -> Vec<u8> {
    let discriminator: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];
    let mut data = Vec::with_capacity(133);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&[0u8; 32]); // write_authority placeholder
    data.push(1u8);                      // verification_level: Full
    data.extend_from_slice(&[0xEFu8; 32]); // feed_id
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&100_000u64.to_le_bytes()); // conf
    data.extend_from_slice(&exponent.to_le_bytes());
    data.extend_from_slice(&publish_time.to_le_bytes());
    data.extend_from_slice(&(publish_time - 1).to_le_bytes()); // prev_publish_time
    data.extend_from_slice(&price.to_le_bytes()); // ema_price
    data.extend_from_slice(&120_000u64.to_le_bytes()); // ema_conf
    data.extend_from_slice(&1u64.to_le_bytes()); // posted_slot
    data
}

/// Fixed publish time matching the test clock
const PUBLISH_TIME: i64 = 1_700_000_000;

/// All test mints (USDC and the basket assets) use 6 decimals, matching real USDC.
const TOKEN_DECIMALS: u8 = 6;

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

fn setup_full() -> TestContext {
    let vault_program_id = vault_strategy::id();
    let router_program_id = mock_swap_router::id();

    let mut svm = LiteSVM::new();

    let vault_bytes = include_bytes!("../../../target/deploy/vault_strategy.so");
    let router_bytes = include_bytes!("../../../target/deploy/mock_swap_router.so");

    svm.add_program(vault_program_id, vault_bytes).unwrap();
    svm.add_program(router_program_id, router_bytes).unwrap();

    // Set a fixed clock so Pyth staleness check passes
    svm.set_sysvar(&Clock {
        slot: 1,
        epoch_start_timestamp: PUBLISH_TIME,
        epoch: 0,
        leader_schedule_epoch: 0,
        unix_timestamp: PUBLISH_TIME,
    });

    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
    let manager = create_wallet(&mut svm, 10_000_000_000).unwrap();

    // Create mints with payer as the initial mint authority for all three
    let usdc_mint = create_token_mint(&mut svm, &payer, TOKEN_DECIMALS, None).unwrap();
    let tsla_mint = create_token_mint(&mut svm, &payer, TOKEN_DECIMALS, None).unwrap();
    let nvda_mint = create_token_mint(&mut svm, &payer, TOKEN_DECIMALS, None).unwrap();

    let (router_authority_pda, _) =
        Pubkey::find_program_address(&[b"router_authority"], &router_program_id);

    // The router pays out swap_usdc_for_asset by minting, so the basket asset
    // mints must have router_authority as their mint authority
    for basket_mint in [&tsla_mint, &nvda_mint] {
        let set_authority_instruction = spl_token::instruction::set_authority(
            &spl_token::ID,
            basket_mint,
            Some(&router_authority_pda),
            spl_token::instruction::AuthorityType::MintTokens,
            &payer.pubkey(),
            &[],
        )
        .unwrap();
        send_transaction_from_instructions(
            &mut svm,
            vec![set_authority_instruction],
            &[&payer],
            &payer.pubkey(),
        )
        .unwrap();
    }

    // Derive PDAs
    let (strategy_pda, _) = Pubkey::find_program_address(
        &[b"strategy", manager.pubkey().as_ref()],
        &vault_program_id,
    );
    let (share_mint_pda, _) = Pubkey::find_program_address(
        &[b"share_mint", strategy_pda.as_ref()],
        &vault_program_id,
    );
    let (router_config_pda, _) =
        Pubkey::find_program_address(&[b"router_config"], &router_program_id);
    let (tsla_rate_pda, _) =
        Pubkey::find_program_address(&[b"rate", tsla_mint.as_ref()], &router_program_id);
    let (nvda_rate_pda, _) =
        Pubkey::find_program_address(&[b"rate", nvda_mint.as_ref()], &router_program_id);

    // ATAs
    let vault_usdc = derive_ata(&strategy_pda, &usdc_mint);
    let vault_tsla = derive_ata(&strategy_pda, &tsla_mint);
    let vault_nvda = derive_ata(&strategy_pda, &nvda_mint);
    let router_usdc_treasury = derive_ata(&router_authority_pda, &usdc_mint);

    // Create mock Pyth price feed accounts
    // TSLAx: $250 = 25_000_000_000 * 10^-8
    let price_feed_tsla_key = Keypair::new();
    let tsla_data = build_mock_price_update_account(25_000_000_000i64, -8i32, PUBLISH_TIME);
    let rent_tsla = svm.minimum_balance_for_rent_exemption(tsla_data.len());
    svm.set_account(
        price_feed_tsla_key.pubkey(),
        SolanaAccount {
            lamports: rent_tsla,
            data: tsla_data,
            owner: pyth_receiver_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    // NVDAx: $180 = 18_000_000_000 * 10^-8
    let price_feed_nvda_key = Keypair::new();
    let nvda_data = build_mock_price_update_account(18_000_000_000i64, -8i32, PUBLISH_TIME);
    let rent_nvda = svm.minimum_balance_for_rent_exemption(nvda_data.len());
    svm.set_account(
        price_feed_nvda_key.pubkey(),
        SolanaAccount {
            lamports: rent_nvda,
            data: nvda_data,
            owner: pyth_receiver_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let price_feed_tsla = price_feed_tsla_key.pubkey();
    let price_feed_nvda = price_feed_nvda_key.pubkey();

    // Step 1: Initialize router
    let init_router_ix = Instruction::new_with_bytes(
        router_program_id,
        &mock_swap_router::instruction::InitializeRouter {
            usdc_mint,
        }
        .data(),
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
    send_transaction_from_instructions(
        &mut svm,
        vec![init_router_ix],
        &[&payer],
        &payer.pubkey(),
    )
    .unwrap();

    // Step 2: Set TSLAx rate = 250 usdc per token
    let set_tsla_rate_ix = Instruction::new_with_bytes(
        router_program_id,
        &mock_swap_router::instruction::SetRate {
            mint: tsla_mint,
            usdc_per_token: 250,
        }
        .data(),
        mock_swap_router::accounts::SetRateAccountConstraints {
            authority: payer.pubkey(),
            router_config: router_config_pda,
            asset_mint: tsla_mint,
            usdc_mint,
            asset_rate: tsla_rate_pda,
            router_authority: router_authority_pda,
            router_usdc_treasury,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut svm,
        vec![set_tsla_rate_ix],
        &[&payer],
        &payer.pubkey(),
    )
    .unwrap();

    // Step 3: Set NVDAx rate = 180 usdc per token
    let set_nvda_rate_ix = Instruction::new_with_bytes(
        router_program_id,
        &mock_swap_router::instruction::SetRate {
            mint: nvda_mint,
            usdc_per_token: 180,
        }
        .data(),
        mock_swap_router::accounts::SetRateAccountConstraints {
            authority: payer.pubkey(),
            router_config: router_config_pda,
            asset_mint: nvda_mint,
            usdc_mint,
            asset_rate: nvda_rate_pda,
            router_authority: router_authority_pda,
            router_usdc_treasury,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut svm,
        vec![set_nvda_rate_ix],
        &[&payer],
        &payer.pubkey(),
    )
    .unwrap();

    // Step 4: Seed the router USDC treasury with 10,000 USDC so swap_asset_for_usdc can pay out
    mint_tokens_to_token_account(
        &mut svm,
        &usdc_mint,
        &router_usdc_treasury,
        10_000_000_000u64, // 10,000 USDC
        &payer,
    )
    .unwrap();

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

fn build_initialize_strategy_instruction(
    ctx: &TestContext,
    fee_bps: u16,
    swap_router: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::InitializeStrategy {
            weight_bps_a: 4000,
            weight_bps_b: 6000,
            fee_bps,
            swap_router,
            price_feed_a: ctx.price_feed_tsla,
            price_feed_b: ctx.price_feed_nvda,
        }
        .data(),
        vault_strategy::accounts::InitializeStrategyAccountConstraints {
            manager: ctx.manager.pubkey(),
            usdc_mint: ctx.usdc_mint,
            asset_mint_a: ctx.tsla_mint,
            asset_mint_b: ctx.nvda_mint,
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            vault_usdc: ctx.vault_usdc,
            vault_asset_a: ctx.vault_tsla,
            vault_asset_b: ctx.vault_nvda,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

/// Annual management fee used by the happy-path tests: 100 bps = 1%.
const TEST_FEE_BPS: u16 = 100;

fn initialize_strategy(ctx: &mut TestContext) {
    initialize_strategy_with_router(ctx, ctx.router_program_id);
}

/// Initialize the strategy with an arbitrary stored swap router, so tests can
/// prove that invest/rebalance reject a router program the strategy did not register.
fn initialize_strategy_with_router(ctx: &mut TestContext, swap_router: Pubkey) {
    let init_strategy_ix = build_initialize_strategy_instruction(ctx, TEST_FEE_BPS, swap_router);
    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![init_strategy_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    )
    .unwrap();
}

#[test]
fn test_initialize_strategy() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // Verify strategy PDA exists
    assert!(
        ctx.svm.get_account(&ctx.strategy_pda).is_some(),
        "Strategy PDA should exist"
    );

    // Verify share mint exists
    assert!(
        ctx.svm.get_account(&ctx.share_mint_pda).is_some(),
        "Share mint PDA should exist"
    );

    // Verify vault ATAs exist
    assert!(
        ctx.svm.get_account(&ctx.vault_usdc).is_some(),
        "Vault USDC ATA should exist"
    );
    assert!(
        ctx.svm.get_account(&ctx.vault_tsla).is_some(),
        "Vault TSLAx ATA should exist"
    );
    assert!(
        ctx.svm.get_account(&ctx.vault_nvda).is_some(),
        "Vault NVDAx ATA should exist"
    );
}

#[test]
fn test_deposit_first() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    let user = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let deposit_amount: u64 = 1_000_000; // 1 USDC

    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    let user_share = derive_ata(&user.pubkey(), &ctx.share_mint_pda);

    mint_tokens_to_token_account(&mut ctx.svm, &ctx.usdc_mint, &user_usdc, deposit_amount, &ctx.payer)
        .unwrap();

    let deposit_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Deposit {
            usdc_amount: deposit_amount,
            minimum_shares: deposit_amount, // 1:1 on first deposit
        }
        .data(),
        vault_strategy::accounts::DepositAccountConstraints {
            depositor: user.pubkey(),
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint_a: ctx.tsla_mint,
            asset_mint_b: ctx.nvda_mint,
            depositor_usdc_account: user_usdc,
            depositor_share_account: user_share,
            vault_usdc: ctx.vault_usdc,
            vault_asset_a: ctx.vault_tsla,
            vault_asset_b: ctx.vault_nvda,
            price_feed_a: ctx.price_feed_tsla,
            price_feed_b: ctx.price_feed_nvda,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![deposit_ix],
        &[&ctx.payer, &user],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    // First deposit is 1:1 - shares == usdc_amount
    let share_balance = get_token_account_balance(&ctx.svm, &user_share).unwrap();
    assert_eq!(share_balance, deposit_amount, "First deposit should be 1:1");

    let vault_usdc_balance = get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap();
    assert_eq!(vault_usdc_balance, deposit_amount, "Vault USDC should hold deposit");
}

fn do_deposit(ctx: &mut TestContext, user: &Keypair, usdc_amount: u64) -> Pubkey {
    let user_usdc = derive_ata(&user.pubkey(), &ctx.usdc_mint);
    let user_share = derive_ata(&user.pubkey(), &ctx.share_mint_pda);

    let deposit_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Deposit {
            usdc_amount,
            minimum_shares: 0,
        }
        .data(),
        vault_strategy::accounts::DepositAccountConstraints {
            depositor: user.pubkey(),
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint_a: ctx.tsla_mint,
            asset_mint_b: ctx.nvda_mint,
            depositor_usdc_account: user_usdc,
            depositor_share_account: user_share,
            vault_usdc: ctx.vault_usdc,
            vault_asset_a: ctx.vault_tsla,
            vault_asset_b: ctx.vault_nvda,
            price_feed_a: ctx.price_feed_tsla,
            price_feed_b: ctx.price_feed_nvda,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![deposit_ix],
        &[&ctx.payer, user],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    user_share
}

#[test]
fn test_invest() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // Setup user and deposit 10 USDC
    let user = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let deposit_amount: u64 = 10_000_000; // 10 USDC
    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(
        &mut ctx.svm,
        &ctx.usdc_mint,
        &user_usdc,
        deposit_amount,
        &ctx.payer,
    )
    .unwrap();
    do_deposit(&mut ctx, &user, deposit_amount);

    // Invest 4 USDC into TSLAx (rate=250, so 4/250 = 0.016 TSLAx = 16000 tokens at 6 decimals)
    let invest_amount: u64 = 4_000_000;
    let invest_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Invest {
            usdc_amount: invest_amount,
            minimum_asset_out: 0,
        }
        .data(),
        vault_strategy::accounts::InvestAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint: ctx.tsla_mint,
            vault_usdc: ctx.vault_usdc,
            vault_asset: ctx.vault_tsla,
            asset_rate: ctx.tsla_rate_pda,
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
        vec![invest_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    // 4_000_000 USDC / 250 rate = 16_000 TSLAx tokens
    let tsla_balance = get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap();
    assert_eq!(tsla_balance, 16_000, "Vault should hold 16000 TSLAx tokens");

    let usdc_balance = get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap();
    assert_eq!(
        usdc_balance,
        deposit_amount - invest_amount,
        "Vault USDC should decrease by invest amount"
    );
}

#[test]
fn test_deposit_after_invest() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // Alice deposits 10 USDC first
    let alice = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let alice_deposit: u64 = 10_000_000;
    let alice_usdc =
        create_associated_token_account(&mut ctx.svm, &alice.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(
        &mut ctx.svm,
        &ctx.usdc_mint,
        &alice_usdc,
        alice_deposit,
        &ctx.payer,
    )
    .unwrap();
    let alice_share = do_deposit(&mut ctx, &alice, alice_deposit);

    let alice_shares = get_token_account_balance(&ctx.svm, &alice_share).unwrap();
    assert_eq!(alice_shares, 10_000_000, "Alice first deposit 1:1");

    // Manager invests 4 USDC into TSLAx
    let invest_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Invest {
            usdc_amount: 4_000_000,
            minimum_asset_out: 0,
        }
        .data(),
        vault_strategy::accounts::InvestAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint: ctx.tsla_mint,
            vault_usdc: ctx.vault_usdc,
            vault_asset: ctx.vault_tsla,
            asset_rate: ctx.tsla_rate_pda,
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
        vec![invest_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    // NAV after invest (using Pyth prices, PYTH_PRICE_PRECISION = 10^8):
    // vault_usdc = 6_000_000
    // vault_tsla = 16_000 tokens * 25_000_000_000 / 10^8 = 16_000 * 250 = 4_000_000 USDC value
    // total NAV = 10_000_000 (same as before)
    // total_shares = 10_000_000
    // share price = 1.0 USDC per share (unchanged)

    // Bob deposits 5 USDC
    let bob = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let bob_deposit: u64 = 5_000_000;
    let bob_usdc =
        create_associated_token_account(&mut ctx.svm, &bob.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(
        &mut ctx.svm,
        &ctx.usdc_mint,
        &bob_usdc,
        bob_deposit,
        &ctx.payer,
    )
    .unwrap();
    let bob_share = do_deposit(&mut ctx, &bob, bob_deposit);

    // shares = 5_000_000 * 10_000_000 / 10_000_000 = 5_000_000
    let bob_shares = get_token_account_balance(&ctx.svm, &bob_share).unwrap();
    assert_eq!(bob_shares, 5_000_000, "Bob should get 5M shares at par");
}

#[test]
fn test_collect_fees() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // Deposit 1M USDC so there are shares outstanding
    let user = create_wallet(&mut ctx.svm, 100_000_000_000).unwrap();
    let deposit_amount: u64 = 1_000_000_000; // 1000 USDC
    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(
        &mut ctx.svm,
        &ctx.usdc_mint,
        &user_usdc,
        deposit_amount,
        &ctx.payer,
    )
    .unwrap();
    do_deposit(&mut ctx, &user, deposit_amount);

    // Advance clock by 1 year to trigger fee accrual
    let current_clock = ctx.svm.get_sysvar::<Clock>();
    ctx.svm.set_sysvar(&Clock {
        slot: current_clock.slot + 1_000_000,
        epoch_start_timestamp: current_clock.epoch_start_timestamp,
        epoch: current_clock.epoch + 100,
        leader_schedule_epoch: current_clock.leader_schedule_epoch + 100,
        unix_timestamp: current_clock.unix_timestamp + 31_536_000i64,
    });

    let manager_share = derive_ata(&ctx.manager.pubkey(), &ctx.share_mint_pda);

    let collect_fees_ix = Instruction::new_with_bytes(
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

    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![collect_fees_ix],
        &[&ctx.payer],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    // Fee = 1000_000_000 * 100 / 10_000 * (31_536_000 / 31_536_000) = 10_000_000
    // ~1% of 1000 USDC worth of shares = 10 USDC worth of shares
    let fee_shares = get_token_account_balance(&ctx.svm, &manager_share).unwrap();
    assert!(fee_shares > 0, "Manager should receive fee shares");
    // 1% of 1_000_000_000 = 10_000_000
    assert_eq!(fee_shares, 10_000_000, "Annual fee should be 1% of total shares");
}

#[test]
fn test_withdraw() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // User deposits 10 USDC
    let user = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let deposit_amount: u64 = 10_000_000;
    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(
        &mut ctx.svm,
        &ctx.usdc_mint,
        &user_usdc,
        deposit_amount,
        &ctx.payer,
    )
    .unwrap();
    let user_share = do_deposit(&mut ctx, &user, deposit_amount);

    let shares = get_token_account_balance(&ctx.svm, &user_share).unwrap();
    assert_eq!(shares, deposit_amount);

    // Withdraw all shares
    let user_tsla = derive_ata(&user.pubkey(), &ctx.tsla_mint);
    let user_nvda = derive_ata(&user.pubkey(), &ctx.nvda_mint);

    let withdraw_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Withdraw {
            shares_to_burn: shares,
            min_usdc_out: 0,
            min_asset_a_out: 0,
            min_asset_b_out: 0,
        }
        .data(),
        vault_strategy::accounts::WithdrawAccountConstraints {
            user: user.pubkey(),
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint_a: ctx.tsla_mint,
            asset_mint_b: ctx.nvda_mint,
            user_share_account: user_share,
            user_usdc_account: user_usdc,
            user_asset_a_account: user_tsla,
            user_asset_b_account: user_nvda,
            vault_usdc: ctx.vault_usdc,
            vault_asset_a: ctx.vault_tsla,
            vault_asset_b: ctx.vault_nvda,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut ctx.svm,
        vec![withdraw_ix],
        &[&ctx.payer, &user],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    // User should have their USDC back (all shares were minted for USDC only, no assets in vault)
    let usdc_back = get_token_account_balance(&ctx.svm, &user_usdc).unwrap();
    assert_eq!(usdc_back, deposit_amount, "User should get all USDC back");

    // Shares should be burned
    let remaining_shares = get_token_account_balance(&ctx.svm, &user_share).unwrap();
    assert_eq!(remaining_shares, 0, "All shares should be burned");
}

#[test]
fn test_withdraw_rejects_slippage() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    let user = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let deposit_amount: u64 = 10_000_000;
    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(
        &mut ctx.svm,
        &ctx.usdc_mint,
        &user_usdc,
        deposit_amount,
        &ctx.payer,
    )
    .unwrap();
    let user_share = do_deposit(&mut ctx, &user, deposit_amount);

    let shares = get_token_account_balance(&ctx.svm, &user_share).unwrap();

    let user_tsla = derive_ata(&user.pubkey(), &ctx.tsla_mint);
    let user_nvda = derive_ata(&user.pubkey(), &ctx.nvda_mint);

    // Set min_usdc_out too high to trigger slippage rejection
    let withdraw_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Withdraw {
            shares_to_burn: shares,
            min_usdc_out: deposit_amount + 1, // more than available - should fail
            min_asset_a_out: 0,
            min_asset_b_out: 0,
        }
        .data(),
        vault_strategy::accounts::WithdrawAccountConstraints {
            user: user.pubkey(),
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint_a: ctx.tsla_mint,
            asset_mint_b: ctx.nvda_mint,
            user_share_account: user_share,
            user_usdc_account: user_usdc,
            user_asset_a_account: user_tsla,
            user_asset_b_account: user_nvda,
            vault_usdc: ctx.vault_usdc,
            vault_asset_a: ctx.vault_tsla,
            vault_asset_b: ctx.vault_nvda,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    let result = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![withdraw_ix],
        &[&ctx.payer, &user],
        &ctx.payer.pubkey(),
    );
    assert!(result.is_err(), "Withdraw should fail when slippage too high");
}

#[test]
fn test_rebalance() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // Deposit 100 USDC
    let user = create_wallet(&mut ctx.svm, 100_000_000_000).unwrap();
    let deposit_amount: u64 = 100_000_000; // 100 USDC
    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(
        &mut ctx.svm,
        &ctx.usdc_mint,
        &user_usdc,
        deposit_amount,
        &ctx.payer,
    )
    .unwrap();
    do_deposit(&mut ctx, &user, deposit_amount);

    // Invest some into TSLAx: invest 40 USDC → 160_000 TSLAx base (40_000_000 / 250)
    let invest_tsla_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Invest {
            usdc_amount: 40_000_000,
            minimum_asset_out: 0,
        }
        .data(),
        vault_strategy::accounts::InvestAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint: ctx.tsla_mint,
            vault_usdc: ctx.vault_usdc,
            vault_asset: ctx.vault_tsla,
            asset_rate: ctx.tsla_rate_pda,
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
        vec![invest_tsla_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    // Invest some into NVDAx: invest 30 USDC → 166_666 NVDAx base (30_000_000 / 180)
    let invest_nvda_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Invest {
            usdc_amount: 30_000_000,
            minimum_asset_out: 0,
        }
        .data(),
        vault_strategy::accounts::InvestAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint: ctx.nvda_mint,
            vault_usdc: ctx.vault_usdc,
            vault_asset: ctx.vault_nvda,
            asset_rate: ctx.nvda_rate_pda,
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
        vec![invest_nvda_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    let tsla_before = get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap();
    let nvda_before = get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap();

    // Rebalance: sell 100_000 TSLAx (vault holds 160_000) → receive
    // 25_000_000 USDC (100_000 * 250), then buy NVDAx with that USDC
    // → 138_888 NVDAx (25_000_000 / 180, floor)
    let sell_amount: u64 = 100_000;
    let usdc_from_sell: u64 = sell_amount * 250; // 25_000_000
    let nvda_bought: u64 = usdc_from_sell / 180; // 138_888

    let rebalance_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Rebalance {
            sell_amount,
            minimum_usdc_from_sell: usdc_from_sell,
            usdc_to_invest: usdc_from_sell,
            minimum_buy_amount: nvda_bought,
        }
        .data(),
        vault_strategy::accounts::RebalanceAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            sell_mint: ctx.tsla_mint,
            buy_mint: ctx.nvda_mint,
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
        vec![rebalance_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    )
    .unwrap();

    let tsla_after = get_token_account_balance(&ctx.svm, &ctx.vault_tsla).unwrap();
    let nvda_after = get_token_account_balance(&ctx.svm, &ctx.vault_nvda).unwrap();

    assert_eq!(tsla_after, tsla_before - sell_amount, "TSLAx balance should decrease by sell_amount");
    assert_eq!(nvda_after, nvda_before + nvda_bought, "NVDAx balance should increase by nvda_bought");
}

fn assert_transaction_fails_with(
    result: Result<(), solana_kite::SolanaKiteError>,
    expected_error_name: &str,
) {
    let error = result.expect_err("transaction should fail");
    let error_text = format!("{error:?}");
    assert!(
        error_text.contains(expected_error_name),
        "expected failure with {expected_error_name}, got: {error_text}"
    );
}

#[test]
fn test_initialize_rejects_excessive_fee() {
    let mut ctx = setup_full();

    let excessive_fee_bps = vault_strategy::MAX_FEE_BPS + 1;
    let init_strategy_ix =
        build_initialize_strategy_instruction(&ctx, excessive_fee_bps, ctx.router_program_id);
    let result = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![init_strategy_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    );
    assert_transaction_fails_with(result, "FeeTooHigh");

    assert!(
        ctx.svm.get_account(&ctx.strategy_pda).is_none(),
        "Strategy PDA must not be created when fee_bps exceeds MAX_FEE_BPS"
    );
}

#[test]
fn test_deposit_rejects_wrong_usdc_mint() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // A real but unregistered mint: its strategy-owned vault is empty, so
    // accepting it would understate NAV and mint inflated shares.
    let junk_mint = create_token_mint(&mut ctx.svm, &ctx.payer, TOKEN_DECIMALS, None).unwrap();
    let junk_vault =
        create_associated_token_account(&mut ctx.svm, &ctx.strategy_pda, &junk_mint, &ctx.payer)
            .unwrap();

    let user = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let deposit_amount: u64 = 1_000_000;
    let user_junk =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &junk_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(&mut ctx.svm, &junk_mint, &user_junk, deposit_amount, &ctx.payer)
        .unwrap();
    let user_share = derive_ata(&user.pubkey(), &ctx.share_mint_pda);

    let deposit_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Deposit {
            usdc_amount: deposit_amount,
            minimum_shares: 0,
        }
        .data(),
        vault_strategy::accounts::DepositAccountConstraints {
            depositor: user.pubkey(),
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            usdc_mint: junk_mint,
            asset_mint_a: ctx.tsla_mint,
            asset_mint_b: ctx.nvda_mint,
            depositor_usdc_account: user_junk,
            depositor_share_account: user_share,
            vault_usdc: junk_vault,
            vault_asset_a: ctx.vault_tsla,
            vault_asset_b: ctx.vault_nvda,
            price_feed_a: ctx.price_feed_tsla,
            price_feed_b: ctx.price_feed_nvda,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    let result = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![deposit_ix],
        &[&ctx.payer, &user],
        &ctx.payer.pubkey(),
    );
    assert_transaction_fails_with(result, "InvalidUsdcMint");
}

#[test]
fn test_deposit_rejects_wrong_asset_mint() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // An unregistered mint passed as asset_mint_a: its empty strategy-owned
    // vault would hide the real TSLAx holdings from the NAV calculation.
    let junk_mint = create_token_mint(&mut ctx.svm, &ctx.payer, TOKEN_DECIMALS, None).unwrap();
    let junk_vault =
        create_associated_token_account(&mut ctx.svm, &ctx.strategy_pda, &junk_mint, &ctx.payer)
            .unwrap();

    let user = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let deposit_amount: u64 = 1_000_000;
    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(&mut ctx.svm, &ctx.usdc_mint, &user_usdc, deposit_amount, &ctx.payer)
        .unwrap();
    let user_share = derive_ata(&user.pubkey(), &ctx.share_mint_pda);

    let deposit_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Deposit {
            usdc_amount: deposit_amount,
            minimum_shares: 0,
        }
        .data(),
        vault_strategy::accounts::DepositAccountConstraints {
            depositor: user.pubkey(),
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint_a: junk_mint,
            asset_mint_b: ctx.nvda_mint,
            depositor_usdc_account: user_usdc,
            depositor_share_account: user_share,
            vault_usdc: ctx.vault_usdc,
            vault_asset_a: junk_vault,
            vault_asset_b: ctx.vault_nvda,
            price_feed_a: ctx.price_feed_tsla,
            price_feed_b: ctx.price_feed_nvda,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    let result = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![deposit_ix],
        &[&ctx.payer, &user],
        &ctx.payer.pubkey(),
    );
    assert_transaction_fails_with(result, "InvalidAssetMint");

    // The deposit must not have moved funds or minted shares
    let vault_usdc_balance = get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap();
    assert_eq!(vault_usdc_balance, 0, "Vault USDC must be untouched");
}

#[test]
fn test_withdraw_rejects_wrong_asset_mint() {
    let mut ctx = setup_full();
    initialize_strategy(&mut ctx);

    // Deposit normally so the user holds shares
    let user = create_wallet(&mut ctx.svm, 10_000_000_000).unwrap();
    let deposit_amount: u64 = 10_000_000;
    let user_usdc =
        create_associated_token_account(&mut ctx.svm, &user.pubkey(), &ctx.usdc_mint, &ctx.payer)
            .unwrap();
    mint_tokens_to_token_account(&mut ctx.svm, &ctx.usdc_mint, &user_usdc, deposit_amount, &ctx.payer)
        .unwrap();
    let user_share = do_deposit(&mut ctx, &user, deposit_amount);

    // An unregistered mint passed as asset_mint_a on withdraw: the empty junk
    // vault would replace the real TSLAx vault in the proportional payout.
    let junk_mint = create_token_mint(&mut ctx.svm, &ctx.payer, TOKEN_DECIMALS, None).unwrap();
    let junk_vault =
        create_associated_token_account(&mut ctx.svm, &ctx.strategy_pda, &junk_mint, &ctx.payer)
            .unwrap();
    let user_junk = derive_ata(&user.pubkey(), &junk_mint);
    let user_nvda = derive_ata(&user.pubkey(), &ctx.nvda_mint);

    let withdraw_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Withdraw {
            shares_to_burn: deposit_amount,
            min_usdc_out: 0,
            min_asset_a_out: 0,
            min_asset_b_out: 0,
        }
        .data(),
        vault_strategy::accounts::WithdrawAccountConstraints {
            user: user.pubkey(),
            strategy: ctx.strategy_pda,
            share_mint: ctx.share_mint_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint_a: junk_mint,
            asset_mint_b: ctx.nvda_mint,
            user_share_account: user_share,
            user_usdc_account: user_usdc,
            user_asset_a_account: user_junk,
            user_asset_b_account: user_nvda,
            vault_usdc: ctx.vault_usdc,
            vault_asset_a: junk_vault,
            vault_asset_b: ctx.vault_nvda,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    let result = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![withdraw_ix],
        &[&ctx.payer, &user],
        &ctx.payer.pubkey(),
    );
    assert_transaction_fails_with(result, "InvalidAssetMint");

    // Shares must not have been burned and the vault must still hold the USDC
    let shares_after = get_token_account_balance(&ctx.svm, &user_share).unwrap();
    assert_eq!(shares_after, deposit_amount, "Shares must be untouched");
    let vault_usdc_balance = get_token_account_balance(&ctx.svm, &ctx.vault_usdc).unwrap();
    assert_eq!(vault_usdc_balance, deposit_amount, "Vault USDC must be untouched");
}

#[test]
fn test_invest_rejects_unregistered_router() {
    let mut ctx = setup_full();

    // Strategy registers a router that is NOT the deployed mock-swap-router
    let registered_router = Pubkey::new_unique();
    initialize_strategy_with_router(&mut ctx, registered_router);

    let invest_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Invest {
            usdc_amount: 1_000_000,
            minimum_asset_out: 0,
        }
        .data(),
        vault_strategy::accounts::InvestAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            asset_mint: ctx.tsla_mint,
            vault_usdc: ctx.vault_usdc,
            vault_asset: ctx.vault_tsla,
            asset_rate: ctx.tsla_rate_pda,
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

    let result = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![invest_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    );
    assert_transaction_fails_with(result, "InvalidSwapRouter");
}

#[test]
fn test_rebalance_rejects_unregistered_router() {
    let mut ctx = setup_full();

    let registered_router = Pubkey::new_unique();
    initialize_strategy_with_router(&mut ctx, registered_router);

    let rebalance_ix = Instruction::new_with_bytes(
        ctx.vault_program_id,
        &vault_strategy::instruction::Rebalance {
            sell_amount: 1,
            minimum_usdc_from_sell: 0,
            usdc_to_invest: 0,
            minimum_buy_amount: 0,
        }
        .data(),
        vault_strategy::accounts::RebalanceAccountConstraints {
            manager: ctx.manager.pubkey(),
            strategy: ctx.strategy_pda,
            usdc_mint: ctx.usdc_mint,
            sell_mint: ctx.tsla_mint,
            buy_mint: ctx.nvda_mint,
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

    let result = send_transaction_from_instructions(
        &mut ctx.svm,
        vec![rebalance_ix],
        &[&ctx.payer, &ctx.manager],
        &ctx.payer.pubkey(),
    );
    assert_transaction_fails_with(result, "InvalidSwapRouter");
}
