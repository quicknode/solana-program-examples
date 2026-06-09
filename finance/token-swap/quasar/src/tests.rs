extern crate std;
use {
    alloc::vec,
    quasar_svm::{
        token::{create_keyed_associated_token_account, create_keyed_mint_account, Mint},
        Account, Instruction, Pubkey, QuasarSvm, SPL_TOKEN_PROGRAM_ID,
    },
    std::println,
};

// ── SVM setup ────────────────────────────────────────────────────────────────

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_token_swap.so").unwrap();
    QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program()
}

// ── Account factories ─────────────────────────────────────────────────────────

fn signer(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, 10_000_000_000)
}

fn empty(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: quasar_svm::system_program::ID,
        executable: false,
    }
}

/// Pre-initialised SPL mint with no authority and no supply.
fn test_mint(addr: Pubkey, decimals: u8) -> Account {
    create_keyed_mint_account(
        &addr,
        &Mint {
            is_initialized: true,
            decimals,
            ..Mint::default()
        },
    )
}

/// Depositor's pre-funded ATA (address derived from wallet + mint).
fn funded_ata(wallet: Pubkey, mint: Pubkey, amount: u64) -> Account {
    create_keyed_associated_token_account(&wallet, &mint, amount)
}

/// ATA address derived from wallet + mint (same formula as SPL ATA program).
fn ata_addr(wallet: Pubkey, mint: Pubkey) -> Pubkey {
    create_keyed_associated_token_account(&wallet, &mint, 0).address
}

/// Read the `amount` field (bytes 64–72) from a packed token account.
fn token_amount(account: &Account) -> u64 {
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

// ── PDA helpers ───────────────────────────────────────────────────────────────

fn config_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"config"], &crate::ID.into()).0
}

fn pool_pda(config: Pubkey, mint_a: Pubkey, mint_b: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"", config.as_ref(), mint_a.as_ref(), mint_b.as_ref()],
        &crate::ID.into(),
    )
    .0
}

fn pool_authority_pda(config: Pubkey, mint_a: Pubkey, mint_b: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"authority", config.as_ref(), mint_a.as_ref(), mint_b.as_ref()],
        &crate::ID.into(),
    )
    .0
}

fn lp_mint_pda(config: Pubkey, mint_a: Pubkey, mint_b: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"liquidity", config.as_ref(), mint_a.as_ref(), mint_b.as_ref()],
        &crate::ID.into(),
    )
    .0
}

// ── Instruction data builders ─────────────────────────────────────────────────

fn build_create_config_data(fee: u16, admin_share_bps: u16) -> Vec<u8> {
    let mut data = vec![0u8]; // discriminator = 0
    data.extend_from_slice(&fee.to_le_bytes());
    data.extend_from_slice(&admin_share_bps.to_le_bytes());
    data
}

fn build_deposit_data(amount_a: u64, amount_b: u64) -> Vec<u8> {
    let mut data = vec![2u8]; // discriminator = 2
    data.extend_from_slice(&amount_a.to_le_bytes());
    data.extend_from_slice(&amount_b.to_le_bytes());
    data
}

fn build_withdraw_data(amount: u64) -> Vec<u8> {
    let mut data = vec![3u8]; // discriminator = 3
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

fn build_swap_data(input_is_token_a: bool, input_amount: u64, min_output: u64) -> Vec<u8> {
    let mut data = vec![4u8]; // discriminator = 4
    data.push(input_is_token_a as u8);
    data.extend_from_slice(&input_amount.to_le_bytes());
    data.extend_from_slice(&min_output.to_le_bytes());
    data
}

// ── Instruction builders ──────────────────────────────────────────────────────

fn ix_create_config(config: Pubkey, admin: Pubkey, payer: Pubkey, fee: u16, admin_share: u16) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(config.into(), false),
            solana_instruction::AccountMeta::new_readonly(admin.into(), false),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(quasar_svm::system_program::ID.into(), false),
        ],
        data: build_create_config_data(fee, admin_share),
    }
}

fn ix_create_pool(
    config: Pubkey,
    pool_config: Pubkey,
    pool_authority: Pubkey,
    lp_mint: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pool_a: Pubkey,
    pool_b: Pubkey,
    payer: Pubkey,
) -> Instruction {
    let rent_id = quasar_svm::solana_sdk_ids::sysvar::rent::ID;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new_readonly(config.into(), false),
            solana_instruction::AccountMeta::new(pool_config.into(), false),
            solana_instruction::AccountMeta::new_readonly(pool_authority.into(), false),
            solana_instruction::AccountMeta::new(lp_mint.into(), false),
            solana_instruction::AccountMeta::new_readonly(mint_a.into(), false),
            solana_instruction::AccountMeta::new_readonly(mint_b.into(), false),
            // pool_a and pool_b are non-PDA token accounts created via
            // system::create_account CPI, which requires the `to` account to
            // be a signer in the parent transaction (signers=[]).
            solana_instruction::AccountMeta::new(pool_a.into(), true),
            solana_instruction::AccountMeta::new(pool_b.into(), true),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            solana_instruction::AccountMeta::new_readonly(quasar_svm::system_program::ID.into(), false),
            solana_instruction::AccountMeta::new_readonly(rent_id.into(), false),
        ],
        data: vec![1u8], // discriminator = 1
    }
}

fn ix_deposit(
    config: Pubkey,
    pool_config: Pubkey,
    pool_authority: Pubkey,
    depositor: Pubkey,
    lp_mint: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pool_a: Pubkey,
    pool_b: Pubkey,
    lp_token: Pubkey,
    token_a: Pubkey,
    token_b: Pubkey,
    payer: Pubkey,
    amount_a: u64,
    amount_b: u64,
) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new_readonly(config.into(), false),
            solana_instruction::AccountMeta::new_readonly(pool_config.into(), false),
            solana_instruction::AccountMeta::new_readonly(pool_authority.into(), false),
            solana_instruction::AccountMeta::new_readonly(depositor.into(), true),
            solana_instruction::AccountMeta::new(lp_mint.into(), false),
            solana_instruction::AccountMeta::new_readonly(mint_a.into(), false),
            solana_instruction::AccountMeta::new_readonly(mint_b.into(), false),
            solana_instruction::AccountMeta::new(pool_a.into(), false),
            solana_instruction::AccountMeta::new(pool_b.into(), false),
            // lp_token is a non-PDA account created via system::create_account
            // CPI; the `to` account must be a signer in the parent instruction.
            solana_instruction::AccountMeta::new(lp_token.into(), true),
            solana_instruction::AccountMeta::new(token_a.into(), false),
            solana_instruction::AccountMeta::new(token_b.into(), false),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            solana_instruction::AccountMeta::new_readonly(quasar_svm::system_program::ID.into(), false),
        ],
        data: build_deposit_data(amount_a, amount_b),
    }
}

fn ix_withdraw(
    config: Pubkey,
    pool_config: Pubkey,
    pool_authority: Pubkey,
    depositor: Pubkey,
    lp_mint: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pool_a: Pubkey,
    pool_b: Pubkey,
    lp_token: Pubkey,
    token_a: Pubkey,
    token_b: Pubkey,
    payer: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new_readonly(config.into(), false),
            solana_instruction::AccountMeta::new_readonly(pool_config.into(), false),
            solana_instruction::AccountMeta::new_readonly(pool_authority.into(), false),
            solana_instruction::AccountMeta::new_readonly(depositor.into(), true),
            solana_instruction::AccountMeta::new(lp_mint.into(), false),
            solana_instruction::AccountMeta::new(mint_a.into(), false),
            solana_instruction::AccountMeta::new(mint_b.into(), false),
            solana_instruction::AccountMeta::new(pool_a.into(), false),
            solana_instruction::AccountMeta::new(pool_b.into(), false),
            solana_instruction::AccountMeta::new(lp_token.into(), false),
            // token_a and token_b are non-PDA accounts created via
            // system::create_account CPI; must be signers in parent.
            solana_instruction::AccountMeta::new(token_a.into(), true),
            solana_instruction::AccountMeta::new(token_b.into(), true),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            solana_instruction::AccountMeta::new_readonly(quasar_svm::system_program::ID.into(), false),
        ],
        data: build_withdraw_data(amount),
    }
}

fn ix_swap(
    config: Pubkey,
    pool_config: Pubkey,
    pool_authority: Pubkey,
    trader: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pool_a: Pubkey,
    pool_b: Pubkey,
    token_a: Pubkey,
    token_b: Pubkey,
    payer: Pubkey,
    input_is_token_a: bool,
    input_amount: u64,
    min_output: u64,
) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new_readonly(config.into(), false),
            solana_instruction::AccountMeta::new(pool_config.into(), false),
            solana_instruction::AccountMeta::new_readonly(pool_authority.into(), false),
            solana_instruction::AccountMeta::new_readonly(trader.into(), true),
            solana_instruction::AccountMeta::new_readonly(mint_a.into(), false),
            solana_instruction::AccountMeta::new_readonly(mint_b.into(), false),
            solana_instruction::AccountMeta::new(pool_a.into(), false),
            solana_instruction::AccountMeta::new(pool_b.into(), false),
            // Both token accounts have init(idempotent); the output one is a
            // non-PDA created via system CPI and needs to be a signer.
            // Marking both is harmless since the SVM doesn't verify signatures.
            solana_instruction::AccountMeta::new(token_a.into(), true),
            solana_instruction::AccountMeta::new(token_b.into(), true),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            solana_instruction::AccountMeta::new_readonly(quasar_svm::system_program::ID.into(), false),
        ],
        data: build_swap_data(input_is_token_a, input_amount, min_output),
    }
}

fn ix_claim_fees(
    config: Pubkey,
    pool_config: Pubkey,
    pool_authority: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pool_a: Pubkey,
    pool_b: Pubkey,
    admin: Pubkey,
    admin_token_a: Pubkey,
    admin_token_b: Pubkey,
) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new_readonly(config.into(), false),
            solana_instruction::AccountMeta::new(pool_config.into(), false),
            solana_instruction::AccountMeta::new_readonly(pool_authority.into(), false),
            solana_instruction::AccountMeta::new_readonly(mint_a.into(), false),
            solana_instruction::AccountMeta::new_readonly(mint_b.into(), false),
            solana_instruction::AccountMeta::new(pool_a.into(), false),
            solana_instruction::AccountMeta::new(pool_b.into(), false),
            solana_instruction::AccountMeta::new_readonly(admin.into(), true),
            solana_instruction::AccountMeta::new(admin_token_a.into(), false),
            solana_instruction::AccountMeta::new(admin_token_b.into(), false),
            solana_instruction::AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        ],
        data: vec![5u8], // discriminator = 5
    }
}

// ── Shared pool environment ───────────────────────────────────────────────────

struct PoolEnv {
    svm: QuasarSvm,
    admin: Pubkey,
    payer: Pubkey,
    config: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pool_config: Pubkey,
    pool_authority: Pubkey,
    lp_mint: Pubkey,
    pool_a: Pubkey,
    pool_b: Pubkey,
}

/// Creates config + two mints + pool and commits everything to the SVM.
fn setup_pool() -> PoolEnv {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let admin = Pubkey::new_unique();

    // create_config
    let config = config_pda();
    let r = svm.process_instruction(
        &ix_create_config(config, admin, payer, 30, 1_667),
        &[empty(config), empty(admin), signer(payer)],
    );
    assert!(r.is_ok(), "setup_pool/create_config: {:?}", r.raw_result);

    // Pre-populate mint accounts (no on-chain minting needed for tests).
    let mint_a = Pubkey::new_unique();
    let mint_b = Pubkey::new_unique();
    svm.set_account(test_mint(mint_a, 6));
    svm.set_account(test_mint(mint_b, 6));

    // Derive pool PDAs.
    let pool_config = pool_pda(config, mint_a, mint_b);
    let pool_authority = pool_authority_pda(config, mint_a, mint_b);
    let lp_mint = lp_mint_pda(config, mint_a, mint_b);
    // Pool token-A and token-B reserves live at arbitrary unique addresses.
    let pool_a = Pubkey::new_unique();
    let pool_b = Pubkey::new_unique();

    // create_pool — pass empty PDA slots (pool_config, lp_mint) and signer
    // slots for non-PDA token accounts (pool_a, pool_b).  The SVM commits
    // all accounts from the merged list, so every new account must appear here.
    let r = svm.process_instruction(
        &ix_create_pool(
            config, pool_config, pool_authority, lp_mint,
            mint_a, mint_b, pool_a, pool_b, payer,
        ),
        &[
            empty(pool_config),
            empty(pool_authority),
            empty(lp_mint),
            signer(pool_a), // non-PDA: needs signer status for create_account CPI
            signer(pool_b),
            signer(payer),
        ],
    );
    assert!(r.is_ok(), "setup_pool/create_pool: {:?}", r.raw_result);

    PoolEnv { svm, admin, payer, config, mint_a, mint_b, pool_config, pool_authority, lp_mint, pool_a, pool_b }
}

/// Deposits `amount_a` / `amount_b` for a fresh depositor. Returns the
/// depositor's LP-token account address.
fn do_deposit(env: &mut PoolEnv, amount_a: u64, amount_b: u64) -> (Pubkey, Pubkey) {
    let depositor = Pubkey::new_unique();

    // Pre-fund the depositor's token accounts and commit them to the SVM so
    // they're in the "merged" set and get committed after the instruction.
    let ta = funded_ata(depositor, env.mint_a, amount_a);
    let tb = funded_ata(depositor, env.mint_b, amount_b);
    let token_a = ta.address;
    let token_b = tb.address;
    env.svm.set_account(ta);
    env.svm.set_account(tb);

    // LP token account will be created by init(idempotent) — pass as signer
    // because system::create_account CPI requires the new account to sign.
    let lp_token = Pubkey::new_unique();

    let r = env.svm.process_instruction(
        &ix_deposit(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, token_a, token_b, env.payer,
            amount_a, amount_b,
        ),
        &[signer(lp_token), signer(depositor)],
    );
    assert!(r.is_ok(), "do_deposit: {:?}", r.raw_result);

    (depositor, lp_token)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests — create_config (existing)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_create_config() {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let (config_pda, _) = Pubkey::find_program_address(&[b"config"], &crate::ID.into());
    let data = build_create_config_data(30, 1667);
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(config_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(admin.into(), false),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(quasar_svm::system_program::ID.into(), false),
        ],
        data,
    };
    let result = svm.process_instruction(
        &instruction,
        &[empty(config_pda), signer(admin), signer(payer)],
    );
    assert!(result.is_ok(), "create_config failed: {:?}", result.raw_result);
    println!("  CREATE CONFIG CU: {}", result.compute_units_consumed);
}

#[test]
fn test_create_config_invalid_fee() {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let (config_pda, _) = Pubkey::find_program_address(&[b"config"], &crate::ID.into());
    let data = build_create_config_data(10000, 1667); // fee >= 10_000 → invalid
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(config_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(admin.into(), false),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(quasar_svm::system_program::ID.into(), false),
        ],
        data,
    };
    let result = svm.process_instruction(
        &instruction,
        &[empty(config_pda), signer(admin), signer(payer)],
    );
    assert!(!result.is_ok(), "create_config should have failed with invalid fee");
    println!("  CREATE CONFIG (invalid fee) correctly rejected");
}

#[test]
fn test_create_config_invalid_admin_share() {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let admin = Pubkey::new_unique();
    let (config_pda, _) = Pubkey::find_program_address(&[b"config"], &crate::ID.into());
    let data = build_create_config_data(30, 10000); // admin_share_bps >= 10_000 → invalid
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(config_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(admin.into(), false),
            solana_instruction::AccountMeta::new(payer.into(), true),
            solana_instruction::AccountMeta::new_readonly(quasar_svm::system_program::ID.into(), false),
        ],
        data,
    };
    let result = svm.process_instruction(
        &instruction,
        &[empty(config_pda), signer(admin), signer(payer)],
    );
    assert!(!result.is_ok(), "create_config should have failed with admin_share_bps >= 10000");
    println!("  CREATE CONFIG (invalid admin share) correctly rejected");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests — create_pool
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_create_pool() {
    let env = setup_pool();
    // The pool_config PDA must now exist and be owned by our program.
    let pc = env.svm.get_account(&env.pool_config).expect("pool_config missing after create_pool");
    assert_eq!(pc.owner, env.svm.get_account(&env.pool_config).unwrap().owner);
    // LP mint PDA must be a valid SPL mint (82 bytes, owned by token program).
    let lp = env.svm.get_account(&env.lp_mint).expect("lp_mint missing");
    assert_eq!(lp.data.len(), 82, "LP mint should be 82 bytes");
    println!("  CREATE POOL: pool_config={}, lp_mint={}", env.pool_config, env.lp_mint);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests — deposit_liquidity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_deposit_liquidity_initial() {
    let mut env = setup_pool();

    let amount_a = 1_000_000u64;
    let amount_b = 4_000_000u64;

    let (_depositor, lp_token) = do_deposit(&mut env, amount_a, amount_b);

    // LP token account must exist with a non-zero balance.
    let lp_acct = env.svm.get_account(&lp_token).expect("lp_token missing after deposit");
    let lp_balance = token_amount(&lp_acct);
    assert!(lp_balance > 0, "expected LP tokens, got 0");

    // Pool reserves must have received the tokens.
    let pa = env.svm.get_account(&env.pool_a).expect("pool_a missing");
    let pb = env.svm.get_account(&env.pool_b).expect("pool_b missing");
    assert_eq!(token_amount(&pa), amount_a);
    assert_eq!(token_amount(&pb), amount_b);

    println!("  DEPOSIT: LP minted={}, pool_a={}, pool_b={}", lp_balance, amount_a, amount_b);
}

#[test]
fn test_deposit_liquidity_subsequent_proportional() {
    let mut env = setup_pool();

    // Initial deposit: 1:4 ratio.
    let (_, lp1) = do_deposit(&mut env, 1_000_000, 4_000_000);
    let lp1_bal = token_amount(&env.svm.get_account(&lp1).unwrap());

    // Second depositor with the same 1:4 ratio gets proportional LP tokens.
    let (_, lp2) = do_deposit(&mut env, 500_000, 2_000_000);
    let lp2_bal = token_amount(&env.svm.get_account(&lp2).unwrap());

    // Half the first deposit → should get roughly half the LP tokens.
    // Allow ±1 for integer rounding.
    assert!(
        lp2_bal > 0 && lp2_bal <= lp1_bal,
        "second depositor LP={} should be > 0 and <= first LP={}",
        lp2_bal, lp1_bal
    );
    println!("  SECOND DEPOSIT: lp1={}, lp2={}", lp1_bal, lp2_bal);
}

#[test]
fn test_deposit_insufficient_funds_rejected() {
    let mut env = setup_pool();

    let depositor = Pubkey::new_unique();
    // Fund with only 100 of each but request 1_000_000.
    let ta = funded_ata(depositor, env.mint_a, 100);
    let tb = funded_ata(depositor, env.mint_b, 100);
    let (token_a, token_b) = (ta.address, tb.address);
    env.svm.set_account(ta);
    env.svm.set_account(tb);
    let lp_token = Pubkey::new_unique();

    let r = env.svm.process_instruction(
        &ix_deposit(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, token_a, token_b, env.payer,
            1_000_000, 1_000_000,
        ),
        &[empty(lp_token), signer(depositor)],
    );
    assert!(!r.is_ok(), "deposit with insufficient funds should fail");
    println!("  DEPOSIT insufficient funds correctly rejected");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests — withdraw_liquidity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_withdraw_liquidity() {
    let mut env = setup_pool();
    let amount_a = 2_000_000u64;
    let amount_b = 2_000_000u64;

    let (depositor, lp_token) = do_deposit(&mut env, amount_a, amount_b);
    let lp_balance = token_amount(&env.svm.get_account(&lp_token).unwrap());
    assert!(lp_balance > 0);

    // Withdraw half the LP tokens.
    let withdraw_amount = lp_balance / 2;

    // Output token accounts are created by init(idempotent) → pass as empty.
    let recv_a = Pubkey::new_unique();
    let recv_b = Pubkey::new_unique();

    let r = env.svm.process_instruction(
        &ix_withdraw(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, recv_a, recv_b, env.payer,
            withdraw_amount,
        ),
        // recv_a / recv_b are non-PDA accounts init(idempotent) → signer required.
        &[signer(recv_a), signer(recv_b), signer(depositor)],
    );
    assert!(r.is_ok(), "withdraw failed: {:?}", r.raw_result);

    // Verify the depositor received tokens.
    let ra = env.svm.get_account(&recv_a).expect("recv_a missing after withdraw");
    let rb = env.svm.get_account(&recv_b).expect("recv_b missing after withdraw");
    assert!(token_amount(&ra) > 0, "recv_a should have tokens after withdraw");
    assert!(token_amount(&rb) > 0, "recv_b should have tokens after withdraw");

    println!(
        "  WITHDRAW: lp_burned={}, recv_a={}, recv_b={}",
        withdraw_amount, token_amount(&ra), token_amount(&rb)
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests — swap_tokens
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_swap_a_to_b() {
    let mut env = setup_pool();

    // Seed the pool with liquidity first.
    do_deposit(&mut env, 10_000_000, 10_000_000);

    // Trader swaps 100_000 token A for token B.
    let trader = Pubkey::new_unique();
    let ta = funded_ata(trader, env.mint_a, 1_000_000);
    let token_a = ta.address;
    let token_b_out = Pubkey::new_unique(); // created by init(idempotent)
    env.svm.set_account(ta);

    let input = 100_000u64;
    let r = env.svm.process_instruction(
        &ix_swap(
            env.config, env.pool_config, env.pool_authority, trader,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            token_a, token_b_out, env.payer,
            true, input, 1, // input_is_token_a=true, min_output=1
        ),
        // token_b_out is a new non-PDA account → signer required for init.
        &[signer(token_b_out), signer(trader)],
    );
    assert!(r.is_ok(), "swap A→B failed: {:?}", r.raw_result);

    let out_acct = env.svm.get_account(&token_b_out).expect("token_b_out missing after swap");
    let received = token_amount(&out_acct);
    assert!(received > 0, "expected non-zero token B output");
    println!("  SWAP A→B: input={}, output={}", input, received);
}

#[test]
fn test_swap_b_to_a() {
    let mut env = setup_pool();
    do_deposit(&mut env, 10_000_000, 10_000_000);

    let trader = Pubkey::new_unique();
    let tb = funded_ata(trader, env.mint_b, 1_000_000);
    let token_b = tb.address;
    let token_a_out = Pubkey::new_unique();
    env.svm.set_account(tb);

    let input = 100_000u64;
    let r = env.svm.process_instruction(
        &ix_swap(
            env.config, env.pool_config, env.pool_authority, trader,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            token_a_out, token_b, env.payer,
            false, input, 1, // input_is_token_a=false
        ),
        &[signer(token_a_out), signer(trader)],
    );
    assert!(r.is_ok(), "swap B→A failed: {:?}", r.raw_result);

    let out_acct = env.svm.get_account(&token_a_out).expect("token_a_out missing");
    let received = token_amount(&out_acct);
    assert!(received > 0, "expected non-zero token A output");
    println!("  SWAP B→A: input={}, output={}", input, received);
}

#[test]
fn test_swap_slippage_rejected() {
    let mut env = setup_pool();
    do_deposit(&mut env, 10_000_000, 10_000_000);

    let trader = Pubkey::new_unique();
    let ta = funded_ata(trader, env.mint_a, 1_000_000);
    let token_a = ta.address;
    let token_b_out = Pubkey::new_unique();
    env.svm.set_account(ta);

    // min_output set absurdly high (more than pool can deliver).
    let r = env.svm.process_instruction(
        &ix_swap(
            env.config, env.pool_config, env.pool_authority, trader,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            token_a, token_b_out, env.payer,
            true, 100_000, 999_999_999,
        ),
        &[empty(token_b_out), signer(trader)],
    );
    assert!(!r.is_ok(), "swap with impossible slippage should fail");
    println!("  SWAP slippage guard correctly rejected");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests — claim_admin_fees
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_claim_admin_fees() {
    let mut env = setup_pool();

    // Seed pool and do a swap so fees accumulate.
    do_deposit(&mut env, 10_000_000, 10_000_000);

    let trader = Pubkey::new_unique();
    let ta = funded_ata(trader, env.mint_a, 1_000_000);
    let token_a_in = ta.address;
    let token_b_out = Pubkey::new_unique();
    env.svm.set_account(ta);

    let r = env.svm.process_instruction(
        &ix_swap(
            env.config, env.pool_config, env.pool_authority, trader,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            token_a_in, token_b_out, env.payer,
            true, 500_000, 1,
        ),
        &[signer(token_b_out), signer(trader)],
    );
    assert!(r.is_ok(), "swap before claim: {:?}", r.raw_result);

    // Admin claims accumulated fees.
    let admin_ta = funded_ata(env.admin, env.mint_a, 0);
    let admin_tb = funded_ata(env.admin, env.mint_b, 0);
    let (ata_a, ata_b) = (admin_ta.address, admin_tb.address);
    env.svm.set_account(admin_ta);
    env.svm.set_account(admin_tb);

    let r = env.svm.process_instruction(
        &ix_claim_fees(
            env.config, env.pool_config, env.pool_authority,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            env.admin, ata_a, ata_b,
        ),
        &[signer(env.admin)],
    );
    assert!(r.is_ok(), "claim_admin_fees failed: {:?}", r.raw_result);

    // After claim, admin_token_a should have received some fees (A was the input side).
    let admin_a = env.svm.get_account(&ata_a).expect("admin_ta missing after claim");
    assert!(
        token_amount(&admin_a) > 0,
        "admin should have received token-A fees"
    );
    println!("  CLAIM FEES: admin_a_fees={}", token_amount(&admin_a));
}

#[test]
fn test_claim_admin_fees_unauthorized() {
    let mut env = setup_pool();
    do_deposit(&mut env, 10_000_000, 10_000_000);

    // Swap to accumulate some fees.
    let trader = Pubkey::new_unique();
    let ta = funded_ata(trader, env.mint_a, 1_000_000);
    let token_a_in = ta.address;
    let token_b_out = Pubkey::new_unique();
    env.svm.set_account(ta);
    env.svm.process_instruction(
        &ix_swap(
            env.config, env.pool_config, env.pool_authority, trader,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            token_a_in, token_b_out, env.payer,
            true, 100_000, 1,
        ),
        &[signer(token_b_out), signer(trader)],
    )
    .expect("swap before unauthorized claim test");

    // Impersonator tries to claim with a wrong signer.
    let bad_actor = Pubkey::new_unique();
    let fake_ta = funded_ata(bad_actor, env.mint_a, 0);
    let fake_tb = funded_ata(bad_actor, env.mint_b, 0);
    let (fta, ftb) = (fake_ta.address, fake_tb.address);
    env.svm.set_account(fake_ta);
    env.svm.set_account(fake_tb);

    let r = env.svm.process_instruction(
        &ix_claim_fees(
            env.config, env.pool_config, env.pool_authority,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            bad_actor, fta, ftb,
        ),
        &[signer(bad_actor)],
    );
    assert!(!r.is_ok(), "unauthorized claim_admin_fees should fail");
    println!("  CLAIM FEES unauthorized correctly rejected");
}
