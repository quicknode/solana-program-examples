use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
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

fn derive_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    let (ata, _bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id().as_ref(), mint.as_ref()],
        &ata_program_id(),
    );
    ata
}

fn setup() -> (LiteSVM, Pubkey, Keypair) {
    let program_id = swap_example::id();
    let mut svm = LiteSVM::new();

    let program_bytes = include_bytes!("../../../target/deploy/swap_example.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
    (svm, program_id, payer)
}

/// Ensure mint_a < mint_b by pubkey ordering (the program may require this).
fn ordered_mints(svm: &mut LiteSVM, authority: &Keypair, decimals: u8) -> (Pubkey, Pubkey) {
    loop {
        let a = create_token_mint(svm, authority, decimals, None).unwrap();
        let b = create_token_mint(svm, authority, decimals, None).unwrap();
        if a.as_ref() < b.as_ref() {
            return (a, b);
        }
    }
}

struct TestSetup {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    admin: Keypair,
    config_key: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pool_config_key: Pubkey,
    pool_authority: Pubkey,
    liquidity_provider_mint: Pubkey,
    pool_a: Pubkey,
    pool_b: Pubkey,
    holder_account_a: Pubkey,
    holder_account_b: Pubkey,
    liquidity_account: Pubkey,
}

fn full_setup() -> TestSetup {
    let (mut svm, program_id, payer) = setup();
    let admin = create_wallet(&mut svm, 100_000_000_000).unwrap();

    let decimals: u8 = 6;
    let minted_amount: u64 = 100 * 10u64.pow(decimals as u32);

    let (mint_a, mint_b) = ordered_mints(&mut svm, &admin, decimals);
    let fee: u16 = 500;
    // Uniswap V2's classic 1/6 split: admin keeps ~1/6 of the trading fee,
    // LPs keep ~5/6 (1667 / 10_000 ≈ 0.1667).
    let admin_share_bps: u16 = 1667;

    // Derive the singleton Config PDA (seeds = [b"config"]). One config per
    // deployed program.
    let (config_key, _) = Pubkey::find_program_address(&[b"config"], &program_id);
    let (pool_config_key, _) = Pubkey::find_program_address(
        &[config_key.as_ref(), mint_a.as_ref(), mint_b.as_ref()],
        &program_id,
    );
    let (pool_authority, _) = Pubkey::find_program_address(
        &[
            config_key.as_ref(),
            mint_a.as_ref(),
            mint_b.as_ref(),
            b"authority",
        ],
        &program_id,
    );
    let (liquidity_provider_mint, _) = Pubkey::find_program_address(
        &[
            config_key.as_ref(),
            mint_a.as_ref(),
            mint_b.as_ref(),
            b"liquidity",
        ],
        &program_id,
    );

    let pool_a = derive_ata(&pool_authority, &mint_a);
    let pool_b = derive_ata(&pool_authority, &mint_b);
    let liquidity_account = derive_ata(&admin.pubkey(), &liquidity_provider_mint);

    // Create ATAs for admin and mint tokens
    let holder_account_a =
        create_associated_token_account(&mut svm, &admin.pubkey(), &mint_a, &payer).unwrap();
    let holder_account_b =
        create_associated_token_account(&mut svm, &admin.pubkey(), &mint_b, &payer).unwrap();

    mint_tokens_to_token_account(&mut svm, &mint_a, &holder_account_a, minted_amount, &admin).unwrap();
    mint_tokens_to_token_account(&mut svm, &mint_b, &holder_account_b, minted_amount, &admin).unwrap();

    // Create AMM
    let create_config_ix = Instruction::new_with_bytes(
        program_id,
        &swap_example::instruction::CreateConfig { fee, admin_share_bps }.data(),
        swap_example::accounts::CreateConfigAccounts {
            config: config_key,
            admin: admin.pubkey(),
            payer: payer.pubkey(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut svm,
        vec![create_config_ix],
        &[&payer],
        &payer.pubkey(),
    )
    .unwrap();

    // Create Pool
    let create_pool_ix = Instruction::new_with_bytes(
        program_id,
        &swap_example::instruction::CreatePool {}.data(),
        swap_example::accounts::CreatePoolAccounts {
            config: config_key,
            pool_config: pool_config_key,
            pool_authority,
            liquidity_provider_mint,
            mint_a,
            mint_b,
            pool_a,
            pool_b,
            payer: payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut svm,
        vec![create_pool_ix],
        &[&payer],
        &payer.pubkey(),
    )
    .unwrap();

    TestSetup {
        svm,
        program_id,
        payer,
        admin,
        config_key,
        mint_a,
        mint_b,
        pool_config_key,
        pool_authority,
        liquidity_provider_mint,
        pool_a,
        pool_b,
        holder_account_a,
        holder_account_b,
        liquidity_account,
    }
}

#[test]
fn test_create_config() {
    let (mut svm, program_id, payer) = setup();
    let fee: u16 = 500;
    let admin_share_bps: u16 = 1667;
    let admin = Keypair::new();

    let (config_key, _) = Pubkey::find_program_address(&[b"config"], &program_id);

    let create_config_ix = Instruction::new_with_bytes(
        program_id,
        &swap_example::instruction::CreateConfig { fee, admin_share_bps }.data(),
        swap_example::accounts::CreateConfigAccounts {
            config: config_key,
            admin: admin.pubkey(),
            payer: payer.pubkey(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut svm,
        vec![create_config_ix],
        &[&payer],
        &payer.pubkey(),
    )
    .unwrap();

    // Verify the Config account exists
    let config_account = svm
        .get_account(&config_key)
        .expect("Config account should exist");
    assert!(!config_account.data.is_empty());
}

#[test]
fn test_deposit_liquidity() {
    let mut ts = full_setup();
    let deposit_amount_a: u64 = 4_000_000;
    let deposit_amount_b: u64 = 1_000_000;

    let deposit_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::DepositLiquidity {
            amount_a: deposit_amount_a,
            amount_b: deposit_amount_b,
            // 0 = no slippage floor for this baseline test
            minimum_lp_tokens_out: 0,
        }
        .data(),
        swap_example::accounts::DepositLiquidityAccounts {
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            depositor: ts.admin.pubkey(),
            liquidity_provider_mint: ts.liquidity_provider_mint,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            liquidity_provider_token: ts.liquidity_account,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut ts.svm,
        vec![deposit_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    // Verify liquidity tokens were minted
    let liq_amount = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();
    assert!(liq_amount > 0, "Should have received liquidity tokens");
}

#[test]
fn test_swap_a_to_b() {
    let mut ts = full_setup();

    // Deposit liquidity first
    let deposit_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::DepositLiquidity {
            amount_a: 4_000_000,
            amount_b: 1_000_000,
            // 0 = no slippage floor for this setup deposit
            minimum_lp_tokens_out: 0,
        }
        .data(),
        swap_example::accounts::DepositLiquidityAccounts {
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            depositor: ts.admin.pubkey(),
            liquidity_provider_mint: ts.liquidity_provider_mint,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            liquidity_provider_token: ts.liquidity_account,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![deposit_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    // Get balances before swap
    let before_b = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();

    // Swap 1M of token A for token B
    let swap_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::SwapTokens {
            input_is_token_a: true,
            input_amount: 1_000_000,
            min_output_amount: 100,
        }
        .data(),
        swap_example::accounts::SwapTokensAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            trader: ts.admin.pubkey(),
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![swap_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    // After swap, token B balance should have increased
    let after_b = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();
    assert!(
        after_b > before_b,
        "Token B balance should increase after swap A->B"
    );
}

#[test]
fn test_withdraw_liquidity() {
    let mut ts = full_setup();

    // Deposit liquidity
    let deposit_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::DepositLiquidity {
            amount_a: 4_000_000,
            amount_b: 4_000_000,
            // 0 = no slippage floor for this setup deposit
            minimum_lp_tokens_out: 0,
        }
        .data(),
        swap_example::accounts::DepositLiquidityAccounts {
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            depositor: ts.admin.pubkey(),
            liquidity_provider_mint: ts.liquidity_provider_mint,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            liquidity_provider_token: ts.liquidity_account,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![deposit_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    // Get liquidity token balance
    let liq_amount = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();
    assert!(liq_amount > 0);

    // Withdraw all liquidity
    let withdraw_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::WithdrawLiquidity {
            amount: liq_amount,
            // 0 = no slippage floor for this baseline test
            minimum_token_a_out: 0,
            minimum_token_b_out: 0,
        }
        .data(),
        swap_example::accounts::WithdrawLiquidityAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            depositor: ts.admin.pubkey(),
            liquidity_provider_mint: ts.liquidity_provider_mint,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            liquidity_provider_token: ts.liquidity_account,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![withdraw_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    // Liquidity balance should be 0
    let liq_amount = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();
    assert_eq!(liq_amount, 0, "Liquidity should be fully withdrawn");
}

/// Helper: do a deposit and one A->B swap on top of `full_setup`.
/// Returns the swap input amount (token A base units) for fee-arithmetic checks.
fn deposit_and_swap_a_to_b(ts: &mut TestSetup, deposit_a: u64, deposit_b: u64, swap_in_a: u64) -> u64 {
    let deposit_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::DepositLiquidity {
            amount_a: deposit_a,
            amount_b: deposit_b,
            // 0 = no slippage floor for setup deposits in helpers
            minimum_lp_tokens_out: 0,
        }
        .data(),
        swap_example::accounts::DepositLiquidityAccounts {
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            depositor: ts.admin.pubkey(),
            liquidity_provider_mint: ts.liquidity_provider_mint,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            liquidity_provider_token: ts.liquidity_account,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![deposit_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    let swap_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::SwapTokens {
            input_is_token_a: true,
            input_amount: swap_in_a,
            min_output_amount: 1,
        }
        .data(),
        swap_example::accounts::SwapTokensAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            trader: ts.admin.pubkey(),
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![swap_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    swap_in_a
}

/// Helper: build a `claim_admin_fees` instruction for the standard setup.
fn claim_admin_fees_ix(ts: &TestSetup) -> Instruction {
    Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::ClaimAdminFees {}.data(),
        swap_example::accounts::ClaimAdminFeesAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            admin: ts.admin.pubkey(),
            admin_token_a: ts.holder_account_a,
            admin_token_b: ts.holder_account_b,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    )
}

/// Helper: do an A->B swap of `input_amount` on the standard setup.
fn swap_a_to_b(ts: &mut TestSetup, input_amount: u64) {
    let swap_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::SwapTokens {
            input_is_token_a: true,
            input_amount,
            min_output_amount: 1,
        }
        .data(),
        swap_example::accounts::SwapTokensAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            trader: ts.admin.pubkey(),
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![swap_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();
}

#[test]
fn test_claim_admin_fees() {
    let mut ts = full_setup();
    // fee = 500 bps (5%), admin_share_bps = 1667 (~1/6).
    // Per the swap: fee_amount = input * 500 / 10_000 = input * 5 / 100
    //               admin_portion = fee_amount * 1667 / 10_000
    // Swap input is on the A side, so admin's claim accumulates in mint A.
    let swap_in = 1_000_000u64;
    deposit_and_swap_a_to_b(&mut ts, 4_000_000, 1_000_000, swap_in);

    let fee_amount = swap_in * 500 / 10_000;
    let expected_admin_a = fee_amount * 1667 / 10_000;
    assert!(expected_admin_a > 0, "expected admin portion > 0");

    // ---- Phase 1: first claim transfers the accumulated A-side fees ----
    let admin_balance_a_before =
        get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    let admin_balance_b_before =
        get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();

    let claim_ix_first = claim_admin_fees_ix(&ts);
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![claim_ix_first],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    let admin_balance_a_after =
        get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    let admin_balance_b_after =
        get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();

    assert_eq!(
        admin_balance_a_after - admin_balance_a_before,
        expected_admin_a,
        "admin should receive the accumulated A-side fee"
    );
    // No swap on the B side, so no B-side fees were owed.
    assert_eq!(
        admin_balance_b_after, admin_balance_b_before,
        "admin should receive zero B-side fees (no B-input swaps happened)"
    );

    // ---- Phase 2: swap more, claim again, verify the new fee is paid ----
    // This proves the accumulators were truly reset (not just zeroed in
    // memory): a fresh swap accrues new fees from a clean baseline, and the
    // next claim transfers exactly that new amount.
    let swap_in_2 = 500_000u64;
    swap_a_to_b(&mut ts, swap_in_2);
    let fee_amount_2 = swap_in_2 * 500 / 10_000;
    let expected_admin_a_2 = fee_amount_2 * 1667 / 10_000;
    assert!(expected_admin_a_2 > 0, "expected second admin portion > 0");

    let balance_a_pre_claim_2 =
        get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();

    // Bump the blockhash so this claim-ix tx isn't byte-identical to the
    // earlier one (same accounts + same payload → same signature →
    // `AlreadyProcessed` in litesvm).
    ts.svm.expire_blockhash();
    let claim_ix_second = claim_admin_fees_ix(&ts);
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![claim_ix_second],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .unwrap();

    let balance_a_post_claim_2 =
        get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    assert_eq!(
        balance_a_post_claim_2 - balance_a_pre_claim_2,
        expected_admin_a_2,
        "second claim should transfer only the fees from the second swap"
    );

    // ---- Phase 3: third claim with zero owed reverts with NothingToClaim ----
    // Bump the blockhash so this tx isn't byte-identical to the previous
    // claim - otherwise litesvm short-circuits with `AlreadyProcessed`
    // before the program even runs and we'd never see our error.
    ts.svm.expire_blockhash();
    let claim_ix_again = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::ClaimAdminFees {}.data(),
        swap_example::accounts::ClaimAdminFeesAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            admin: ts.admin.pubkey(),
            admin_token_a: ts.holder_account_a,
            admin_token_b: ts.holder_account_b,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    let balance_a_before_third_claim =
        get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    let result = send_transaction_from_instructions(
        &mut ts.svm,
        vec![claim_ix_again],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    );
    assert!(
        result.is_err(),
        "claim with both accumulators at zero must revert"
    );
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("NothingToClaim") || err_msg.contains("0x1777") || err_msg.contains("6007"),
        "expected NothingToClaim error, got: {err_msg}"
    );

    // Balance unchanged - the revert rolled back any partial state.
    let balance_a_after_third_claim =
        get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    assert_eq!(
        balance_a_after_third_claim, balance_a_before_third_claim,
        "failed claim must not move tokens"
    );
}

#[test]
fn test_claim_admin_fees_rejects_non_admin() {
    let mut ts = full_setup();
    // Need at least one swap so the program has a reason to reach the claim
    // handler (not strictly required, but matches the realistic flow).
    deposit_and_swap_a_to_b(&mut ts, 4_000_000, 1_000_000, 1_000_000);

    // Create a non-admin actor with their own ATAs and try to claim.
    let attacker = create_wallet(&mut ts.svm, 100_000_000_000).unwrap();
    let attacker_token_a =
        create_associated_token_account(&mut ts.svm, &attacker.pubkey(), &ts.mint_a, &ts.payer)
            .unwrap();
    let attacker_token_b =
        create_associated_token_account(&mut ts.svm, &attacker.pubkey(), &ts.mint_b, &ts.payer)
            .unwrap();

    let claim_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::ClaimAdminFees {}.data(),
        swap_example::accounts::ClaimAdminFeesAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            admin: attacker.pubkey(),
            admin_token_a: attacker_token_a,
            admin_token_b: attacker_token_b,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );

    // Should fail because the signer (attacker) does not match Config.admin
    // (enforced by Anchor's `has_one = admin` constraint).
    let result = send_transaction_from_instructions(
        &mut ts.svm,
        vec![claim_ix],
        &[&ts.payer, &attacker],
        &ts.payer.pubkey(),
    );
    assert!(
        result.is_err(),
        "claim_admin_fees by a non-admin signer must fail"
    );
}

/// Helper: issue a `deposit_liquidity` ix with the given amounts. Lets a test
/// fund the pool to any state without copy-pasting the full account list.
fn deposit_ix(ts: &TestSetup, amount_a: u64, amount_b: u64) -> Instruction {
    Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::DepositLiquidity {
            amount_a,
            amount_b,
            // 0 = no slippage floor; slippage-specific tests build their
            // own ix with a non-zero floor.
            minimum_lp_tokens_out: 0,
        }
        .data(),
        swap_example::accounts::DepositLiquidityAccounts {
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            depositor: ts.admin.pubkey(),
            liquidity_provider_mint: ts.liquidity_provider_mint,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            liquidity_provider_token: ts.liquidity_account,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

/// Wrap `send_transaction_from_instructions` so tests can call `?` /
/// `.expect()` on a deposit without re-stating the boilerplate. We don't
/// care about the success payload (`Ok` only signals "tx landed"), and the
/// concrete error type is `solana_kite::SolanaKiteError`. Returning a
/// `Result<(), String>` keeps tests insulated from the kite crate's error
/// type — they just need success/failure plus a message for `.expect()`.
fn send_deposit(ts: &mut TestSetup, amount_a: u64, amount_b: u64) -> Result<(), String> {
    let ix = deposit_ix(ts, amount_a, amount_b);
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .map(|_| ())
    .map_err(|e| format!("{e:?}"))
}

/// Test A: deposit into an already-funded pool at the *exact* current ratio
/// succeeds and both sides are pulled in full. Verifies the clamp leaves
/// matching amounts unchanged and that LP tokens are minted.
#[test]
fn test_deposit_into_funded_pool_at_correct_ratio() {
    let mut ts = full_setup();

    // Seed the pool at a 4:1 ratio. This hits the pool-creation branch, which
    // is unchanged by the fix.
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("initial deposit");

    let pool_a_before = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_before = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    let lp_before = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();

    // Second deposit at the same 4:1 ratio. Neither side should be clamped.
    send_deposit(&mut ts, 8_000_000, 2_000_000).expect("ratio-matched deposit");

    let pool_a_after = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_after = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    let lp_after = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();

    assert_eq!(
        pool_a_after - pool_a_before,
        8_000_000,
        "pool_a should grow by the full requested amount_a"
    );
    assert_eq!(
        pool_b_after - pool_b_before,
        2_000_000,
        "pool_b should grow by the full requested amount_b"
    );
    assert!(lp_after > lp_before, "LP tokens should be minted to depositor");
}

/// Test B: depositor offers more token B than the ratio needs. `amount_b`
/// should be clamped down; `amount_a` should be used in full.
#[test]
fn test_deposit_clamps_excess_amount_b() {
    let mut ts = full_setup();

    // Seed at 4:1.
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("initial deposit");

    let pool_a_before = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_before = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    let holder_a_before = get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    let holder_b_before = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();

    // Caller wants 8M A : 3M B, but at 4:1 only 2M B is needed for 8M A.
    // amount_b should clamp from 3M → 2M.
    send_deposit(&mut ts, 8_000_000, 3_000_000).expect("excess-b deposit");

    let pool_a_after = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_after = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    let holder_a_after = get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    let holder_b_after = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();

    assert_eq!(
        pool_a_after - pool_a_before,
        8_000_000,
        "amount_a should be used in full"
    );
    assert_eq!(
        pool_b_after - pool_b_before,
        2_000_000,
        "amount_b should clamp down to 2M (the ratio-matched amount)"
    );
    // Cross-check via the depositor's balance: only the clamped amount left
    // their wallet.
    assert_eq!(holder_a_before - holder_a_after, 8_000_000);
    assert_eq!(holder_b_before - holder_b_after, 2_000_000);
}

/// Test C: depositor offers more token A than the ratio can absorb. `amount_a`
/// should be clamped down; `amount_b` should be used in full.
#[test]
fn test_deposit_clamps_excess_amount_a() {
    let mut ts = full_setup();

    // Seed at 4:1.
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("initial deposit");

    let pool_a_before = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_before = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();

    // Caller wants 12M A : 2M B, but at 4:1 only 8M A is needed for 2M B.
    // amount_a should clamp from 12M → 8M.
    send_deposit(&mut ts, 12_000_000, 2_000_000).expect("excess-a deposit");

    let pool_a_after = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_after = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();

    assert_eq!(
        pool_a_after - pool_a_before,
        8_000_000,
        "amount_a should clamp down to 8M (the ratio-matched amount)"
    );
    assert_eq!(
        pool_b_after - pool_b_before,
        2_000_000,
        "amount_b should be used in full"
    );
}

/// Test D: end-to-end. A swap shifts the pool away from its seeded ratio; a
/// subsequent deposit must use the *new, shifted* effective ratio (not the
/// raw vault ratio, which includes admin fees). Proves the
/// effective-reserves subtraction works under real swap fees.
#[test]
fn test_deposit_after_swap_uses_shifted_effective_ratio() {
    let mut ts = full_setup();

    // Seed at 100:100 (1:1) so the post-swap ratio is dramatic and easy to
    // sanity-check.
    send_deposit(&mut ts, 10_000_000, 10_000_000).expect("initial deposit");

    // Swap 1M of A in. With fee = 500 bps and admin_share = 1667 bps:
    //   fee_amount = 50_000
    //   admin_portion = 50_000 * 1667 / 10_000 = 8_335 (accrues on side A)
    //   taxed_input = 950_000
    // Effective reserves pre-swap: (10M, 10M). Output b = 950_000 * 10M /
    //   (10M + 950_000) ≈ 867_579 (u128 integer division, exact value
    //   depends on the floor).
    // Post-swap raw vault: pool_a ≈ 11M, pool_b ≈ 9.13M.
    // Effective (LP) reserves: pool_a - 8_335, pool_b unchanged.
    let swap_in = 1_000_000u64;
    let swap_ix = Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::SwapTokens {
            input_is_token_a: true,
            input_amount: swap_in,
            min_output_amount: 1,
        }
        .data(),
        swap_example::accounts::SwapTokensAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            trader: ts.admin.pubkey(),
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![swap_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .expect("swap a→b");

    let pool_a_after_swap = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_after_swap = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    let admin_owed_a: u64 = {
        let account = ts.svm.get_account(&ts.pool_config_key).unwrap();
        // PoolConfig layout: 8-byte anchor discriminator, then Pubkey config
        // (32), Pubkey mint_a (32), Pubkey mint_b (32), u64
        // admin_fees_owed_a (8), u64 admin_fees_owed_b (8), u8 bump.
        let start = 8 + 32 * 3;
        u64::from_le_bytes(account.data[start..start + 8].try_into().unwrap())
    };
    assert!(admin_owed_a > 0, "swap should have accrued admin fees on A");
    let effective_pool_a = pool_a_after_swap - admin_owed_a;
    let effective_pool_b = pool_b_after_swap;
    // Sanity: pool moved meaningfully off 1:1.
    assert!(
        effective_pool_a > effective_pool_b,
        "after A→B swap, effective A side should be larger"
    );

    // Now deposit using the *effective* ratio. We'll deposit at exactly that
    // ratio so both sides should be pulled in full. Pick a base of 1M on the
    // B side and compute the matching A side from effective reserves.
    let deposit_b = 1_000_000u64;
    let deposit_a = ((deposit_b as u128) * (effective_pool_a as u128)
        / (effective_pool_b as u128)) as u64;

    let holder_a_before = get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    let holder_b_before = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();

    // Give the depositor a small headroom on A so the clamp logic has the
    // option to consume the full deposit_a. (We expect deposit_a to be fully
    // used because it matches the effective ratio.)
    send_deposit(&mut ts, deposit_a + 10, deposit_b).expect("post-swap deposit");

    let holder_a_after = get_token_account_balance(&ts.svm, &ts.holder_account_a).unwrap();
    let holder_b_after = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();

    // The contract should have pulled exactly deposit_a (or deposit_a ± 1
    // base unit because of integer division rounding) and the full deposit_b.
    let used_a = holder_a_before - holder_a_after;
    let used_b = holder_b_before - holder_b_after;
    assert_eq!(used_b, deposit_b, "amount_b should be fully used");
    // used_a must be close to deposit_a — never the unbounded raw value
    // (deposit_a + 10). If the bug were still here we'd see something
    // wildly off (or a transaction failure on transfer_checked).
    assert!(
        used_a <= deposit_a + 1 && used_a + 1 >= deposit_a,
        "used_a {} should clamp to ~deposit_a {} (±1 for integer division)",
        used_a,
        deposit_a
    );
}

/// Test E: a deposit so small that one clamped side rounds to zero must
/// revert with `DepositAmountTooSmall` rather than mint LP tokens against a
/// zero contribution.
#[test]
fn test_deposit_too_small_for_ratio_reverts() {
    let mut ts = full_setup();

    // Seed at 4M:1M (A is "cheaper" — 4 A per 1 B). To force amount_b to
    // round down to zero, the depositor must offer < 4 base units of A
    // (so amount_b_required = amount_a * 1M / 4M = 0). We offer 1 base unit
    // of A and a large amount_b.
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("initial deposit");

    let result = send_deposit(&mut ts, 1, 1_000_000);
    assert!(
        result.is_err(),
        "sub-ratio deposit should revert (clamped amount rounds to zero)"
    );
}

/// Test F: LP-mint correctness for a subsequent deposit at the current ratio.
/// With the Uniswap V2 formula `min(a*supply/pool_a, b*supply/pool_b)`, an
/// equal-ratio deposit must mint LP tokens exactly proportional to its share
/// of the pool. Previously the program used `sqrt(a*b)` for *all* deposits,
/// which over- or under-minted depending on pool size and broke
/// proportionality.
#[test]
fn test_lp_mint_proportional_to_share_of_pool() {
    let mut ts = full_setup();

    // Initial deposit: 4M : 1M. sqrt(4M * 1M) = 2_000_000. Minus
    // MINIMUM_LIQUIDITY (100) → depositor LP balance = 1_999_900, which is
    // also the total LP supply (we don't mint the locked floor anywhere).
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("initial deposit");

    let lp_supply_initial = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();
    let expected_initial: u64 = 2_000_000 - 100;
    assert_eq!(
        lp_supply_initial, expected_initial,
        "initial LP supply should equal sqrt(a*b) - MINIMUM_LIQUIDITY"
    );

    // Second deposit at the same 4:1 ratio doubles the pool. The proportional
    // formula must mint exactly `lp_supply_initial` more LP (so the depositor
    // doubles their stake).
    let lp_before_second = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();
    // Same depositor + same args as the first deposit → identical tx
    // signature. Bump the blockhash so litesvm doesn't reject the second
    // tx as `AlreadyProcessed`.
    ts.svm.expire_blockhash();
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("second deposit");
    let lp_after_second = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();

    let minted_on_second = lp_after_second - lp_before_second;
    // min(4M * 1_999_900 / 4M, 1M * 1_999_900 / 1M) = 1_999_900.
    let expected_second: u64 = 1_999_900;
    assert_eq!(
        minted_on_second, expected_second,
        "second deposit (same ratio, same size) should mint the same LP \
         amount as the initial deposit minus the locked floor"
    );
}

/// Test G: LP-mint correctness after a swap has shifted the pool ratio. The
/// effective reserves differ from the seeded ratio; LP minting must use
/// the post-swap effective reserves (vault balance minus admin fees) to
/// keep shares honest.
#[test]
fn test_lp_mint_after_swap_uses_effective_reserves() {
    let mut ts = full_setup();

    // Seed 10M : 10M, then swap A→B so the pool shifts off 1:1.
    send_deposit(&mut ts, 10_000_000, 10_000_000).expect("initial deposit");
    let lp_after_initial = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();
    let total_supply_before_second = lp_after_initial;

    let swap_in = 1_000_000u64;
    swap_a_to_b(&mut ts, swap_in);

    // Read post-swap effective reserves directly from onchain state.
    let pool_a_after_swap = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_after_swap = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    let admin_owed_a: u64 = {
        let account = ts.svm.get_account(&ts.pool_config_key).unwrap();
        let start = 8 + 32 * 3;
        u64::from_le_bytes(account.data[start..start + 8].try_into().unwrap())
    };
    let effective_pool_a = pool_a_after_swap - admin_owed_a;
    let effective_pool_b = pool_b_after_swap;

    // Deposit at exactly the effective ratio. Pick deposit_b, derive deposit_a.
    let deposit_b: u64 = 1_000_000;
    let deposit_a = ((deposit_b as u128) * (effective_pool_a as u128)
        / (effective_pool_b as u128)) as u64;

    // Expected LP minted = min(a*supply/pool_a, b*supply/pool_b) using the
    // *clamped* (a, b) the program actually transfers. After clamp at the
    // exact ratio, the binding side is whichever clamp picks: in
    // deposit_liquidity, `amount_b_required = amount_a * pool_b / pool_a`.
    // We pass `deposit_a` exactly, so amount_b_required = deposit_a * pool_b
    // / pool_a, which rounds down to ≤ deposit_b. The program then uses
    // (deposit_a, amount_b_required). Compute the expected LP from that.
    let amount_b_used = ((deposit_a as u128) * (effective_pool_b as u128)
        / (effective_pool_a as u128)) as u64;
    let expected_liquidity_from_a = (deposit_a as u128) * (total_supply_before_second as u128)
        / (effective_pool_a as u128);
    let expected_liquidity_from_b = (amount_b_used as u128) * (total_supply_before_second as u128)
        / (effective_pool_b as u128);
    let expected_liquidity = expected_liquidity_from_a.min(expected_liquidity_from_b) as u64;

    let lp_before = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();
    send_deposit(&mut ts, deposit_a, deposit_b).expect("post-swap deposit");
    let lp_after = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();

    let minted = lp_after - lp_before;
    assert_eq!(
        minted, expected_liquidity,
        "LP minted on post-swap deposit must match share-of-effective-pool math"
    );
}

// ---------------------------------------------------------------------------
// Slippage protection + invariant tests
// ---------------------------------------------------------------------------

/// Helper: build a `swap_tokens` ix with a custom `min_output_amount`.
fn swap_a_to_b_ix(ts: &TestSetup, input_amount: u64, min_output_amount: u64) -> Instruction {
    Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::SwapTokens {
            input_is_token_a: true,
            input_amount,
            min_output_amount,
        }
        .data(),
        swap_example::accounts::SwapTokensAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            trader: ts.admin.pubkey(),
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

/// Helper: build a `deposit_liquidity` ix with a custom `minimum_lp_tokens_out`.
fn deposit_ix_with_min_lp(
    ts: &TestSetup,
    amount_a: u64,
    amount_b: u64,
    minimum_lp_tokens_out: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::DepositLiquidity {
            amount_a,
            amount_b,
            minimum_lp_tokens_out,
        }
        .data(),
        swap_example::accounts::DepositLiquidityAccounts {
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            depositor: ts.admin.pubkey(),
            liquidity_provider_mint: ts.liquidity_provider_mint,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            liquidity_provider_token: ts.liquidity_account,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

/// Helper: build a `withdraw_liquidity` ix with custom slippage floors.
fn withdraw_ix_with_min(
    ts: &TestSetup,
    amount: u64,
    minimum_token_a_out: u64,
    minimum_token_b_out: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        ts.program_id,
        &swap_example::instruction::WithdrawLiquidity {
            amount,
            minimum_token_a_out,
            minimum_token_b_out,
        }
        .data(),
        swap_example::accounts::WithdrawLiquidityAccounts {
            config: ts.config_key,
            pool_config: ts.pool_config_key,
            pool_authority: ts.pool_authority,
            depositor: ts.admin.pubkey(),
            liquidity_provider_mint: ts.liquidity_provider_mint,
            mint_a: ts.mint_a,
            mint_b: ts.mint_b,
            pool_a: ts.pool_a,
            pool_b: ts.pool_b,
            liquidity_provider_token: ts.liquidity_account,
            token_a: ts.holder_account_a,
            token_b: ts.holder_account_b,
            payer: ts.payer.pubkey(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

/// Slippage test: a swap with `min_output_amount` strictly higher than the
/// achievable output must revert with `SlippageExceeded` rather than fill
/// at a worse rate.
#[test]
fn test_swap_reverts_when_output_below_min() {
    let mut ts = full_setup();
    // Seed a 4:1 pool so 1M of A out gives ~237k of B after a 5% fee.
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("seed");

    // First: prove the swap *would* succeed with a permissive floor.
    let baseline_ix = swap_a_to_b_ix(&ts, 1_000_000, 1);
    let before_b = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![baseline_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .expect("baseline swap should succeed");
    let after_b = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();
    let actual_output = after_b - before_b;
    assert!(actual_output > 0, "baseline swap should produce some B");

    // Reset and try the same swap with `min_output_amount = actual + 1`. It
    // must revert because the pool can't beat the previous output (in fact
    // it can't even match it — the first swap shifted the ratio).
    let mut ts = full_setup();
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("seed");
    let too_high = actual_output + 1;
    let strict_ix = swap_a_to_b_ix(&ts, 1_000_000, too_high);
    let result = send_transaction_from_instructions(
        &mut ts.svm,
        vec![strict_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    );
    let err = format!("{:?}", result.expect_err("must revert"));
    assert!(
        err.contains("SlippageExceeded"),
        "expected SlippageExceeded, got: {err}"
    );
}

/// Slippage test: a deposit with `minimum_lp_tokens_out` strictly higher
/// than the achievable LP mint amount must revert with
/// `DepositBelowMinimum`.
#[test]
fn test_deposit_reverts_when_lp_below_min() {
    let mut ts = full_setup();
    // Seed pool so the second deposit goes through the proportional branch.
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("seed");

    // Compute the LP that a `(4M, 1M)` deposit at the current ratio would
    // mint, using the same formula as the program (no probe tx needed):
    //   liquidity = min(a*supply/pool_a, b*supply/pool_b)
    // Effective reserves == raw reserves here because no swaps have happened.
    let lp_supply = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();
    let pool_a_amount = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_amount = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    let lp_from_a = (4_000_000u128 * lp_supply as u128) / pool_a_amount as u128;
    let lp_from_b = (1_000_000u128 * lp_supply as u128) / pool_b_amount as u128;
    let achievable_lp = lp_from_a.min(lp_from_b) as u64;

    // Require *strictly more* than that — the deposit must revert.
    let strict_ix =
        deposit_ix_with_min_lp(&ts, 4_000_000, 1_000_000, achievable_lp + 1);
    let result = send_transaction_from_instructions(
        &mut ts.svm,
        vec![strict_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    );
    let err = format!("{:?}", result.expect_err("must revert"));
    assert!(
        err.contains("DepositBelowMinimum"),
        "expected DepositBelowMinimum, got: {err}"
    );

    // Sanity: the same deposit with `achievable_lp` as the floor succeeds.
    let ok_ix = deposit_ix_with_min_lp(&ts, 4_000_000, 1_000_000, achievable_lp);
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![ok_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .expect("deposit at exact LP floor should succeed");
}

/// Slippage test: a withdrawal with `minimum_token_a_out` or
/// `minimum_token_b_out` strictly higher than the achievable output must
/// revert with `WithdrawalBelowMinimum`.
#[test]
fn test_withdraw_reverts_when_below_min() {
    let mut ts = full_setup();
    send_deposit(&mut ts, 4_000_000, 4_000_000).expect("seed");
    let lp = get_token_account_balance(&ts.svm, &ts.liquidity_account).unwrap();

    // Burning half the LP at a 4M:4M pool returns ~2M of each side, but the
    // exact amount is `lp/2 * 4_000_000 / (lp_supply + MINIMUM_LIQUIDITY)`.
    // Demand 4M of A out of a half-burn — clearly impossible, must revert.
    let strict_ix = withdraw_ix_with_min(&ts, lp / 2, 4_000_000, 0);
    let result = send_transaction_from_instructions(
        &mut ts.svm,
        vec![strict_ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    );
    let err = format!("{:?}", result.expect_err("must revert (A side)"));
    assert!(
        err.contains("WithdrawalBelowMinimum"),
        "expected WithdrawalBelowMinimum (A side), got: {err}"
    );

    // Same on the B side.
    let strict_ix_b = withdraw_ix_with_min(&ts, lp / 2, 0, 4_000_000);
    let result_b = send_transaction_from_instructions(
        &mut ts.svm,
        vec![strict_ix_b],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    );
    let err_b = format!("{:?}", result_b.expect_err("must revert (B side)"));
    assert!(
        err_b.contains("WithdrawalBelowMinimum"),
        "expected WithdrawalBelowMinimum (B side), got: {err_b}"
    );
}

/// Slippage test: passing `min_output_amount = 0` is the explicit
/// "I accept any non-zero output" signal — this is the documented escape
/// hatch and must still succeed.
#[test]
fn test_swap_with_zero_min_output_still_succeeds() {
    let mut ts = full_setup();
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("seed");

    let before_b = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();
    let ix = swap_a_to_b_ix(&ts, 1_000_000, 0);
    send_transaction_from_instructions(
        &mut ts.svm,
        vec![ix],
        &[&ts.payer, &ts.admin],
        &ts.payer.pubkey(),
    )
    .expect("swap with min_output_amount=0 must succeed");
    let after_b = get_token_account_balance(&ts.svm, &ts.holder_account_b).unwrap();
    assert!(after_b > before_b, "B balance should increase");
}

/// Invariant-check test: a normal swap leaves the effective `k = x * y`
/// at least as high as before (LP fee adds to LP-claimable reserves; admin
/// slice is excluded). This is the runtime guard that catches "the math
/// gave away too much" bugs — verify the happy path doesn't trip it.
#[test]
fn test_invariant_holds_after_normal_swap() {
    let mut ts = full_setup();
    send_deposit(&mut ts, 4_000_000, 1_000_000).expect("seed");

    // Read effective reserves before.
    let pool_a_before = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_before = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    // No swaps yet → admin_fees_owed_* = 0, so effective == raw.
    let k_before = (pool_a_before as u128) * (pool_b_before as u128);

    // Do an A→B swap and verify it succeeds.
    swap_a_to_b(&mut ts, 1_000_000);

    // Compute effective reserves after. We need to subtract admin_fees_owed_a
    // (the swap was input_is_token_a = true, so the admin fee accrued on A).
    let pool_a_after = get_token_account_balance(&ts.svm, &ts.pool_a).unwrap();
    let pool_b_after = get_token_account_balance(&ts.svm, &ts.pool_b).unwrap();
    // PoolConfig layout: 8 (discriminator) + 32*3 (config, mint_a, mint_b) →
    // admin_fees_owed_a starts at byte 104.
    let admin_owed_a: u64 = {
        let account = ts.svm.get_account(&ts.pool_config_key).unwrap();
        let start = 8 + 32 * 3;
        u64::from_le_bytes(account.data[start..start + 8].try_into().unwrap())
    };
    let effective_a_after = pool_a_after - admin_owed_a;
    let effective_b_after = pool_b_after;
    let k_after = (effective_a_after as u128) * (effective_b_after as u128);

    assert!(
        k_after >= k_before,
        "effective invariant must not decrease across a fee-paying swap: \
         before={k_before}, after={k_after}"
    );
}
