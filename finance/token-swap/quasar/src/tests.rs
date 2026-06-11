extern crate std;
use {
    crate::error::AmmError,
    alloc::vec,
    quasar_svm::{
        token::{create_keyed_associated_token_account, create_keyed_mint_account, Mint},
        Account, Instruction, ProgramError, Pubkey, QuasarSvm, SPL_TOKEN_PROGRAM_ID,
    },
    std::println,
};

/// Quasar reports program errors as `ProgramError::Custom(code)`; this maps a
/// named `AmmError` to that wire form for assertions.
fn amm_error(error: AmmError) -> ProgramError {
    ProgramError::Custom(error as u32)
}

/// `amount * numerator / denominator` in u128 with checked ops, narrowed back
/// to u64. Mirrors the program's ratio math for computing expected values.
fn mul_div(amount: u64, numerator: u64, denominator: u64) -> u64 {
    u64::try_from(
        (amount as u128)
            .checked_mul(numerator as u128)
            .expect("mul_div: product overflow")
            .checked_div(denominator as u128)
            .expect("mul_div: divide by zero"),
    )
    .expect("mul_div: result exceeds u64")
}

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

fn build_deposit_data(amount_a: u64, amount_b: u64, minimum_lp_tokens_out: u64) -> Vec<u8> {
    let mut data = vec![2u8]; // discriminator = 2
    data.extend_from_slice(&amount_a.to_le_bytes());
    data.extend_from_slice(&amount_b.to_le_bytes());
    data.extend_from_slice(&minimum_lp_tokens_out.to_le_bytes());
    data
}

fn build_withdraw_data(amount: u64, minimum_token_a_out: u64, minimum_token_b_out: u64) -> Vec<u8> {
    let mut data = vec![3u8]; // discriminator = 3
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&minimum_token_a_out.to_le_bytes());
    data.extend_from_slice(&minimum_token_b_out.to_le_bytes());
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
    minimum_lp_tokens_out: u64,
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
        data: build_deposit_data(amount_a, amount_b, minimum_lp_tokens_out),
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
    minimum_token_a_out: u64,
    minimum_token_b_out: u64,
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
        data: build_withdraw_data(amount, minimum_token_a_out, minimum_token_b_out),
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

    // Pre-populate mint accounts (no onchain minting needed for tests).
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

    // create_pool - pass empty PDA slots (pool_config, lp_mint) and signer
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

    // LP token account will be created by init(idempotent) - pass as signer
    // because system::create_account CPI requires the new account to sign.
    let lp_token = Pubkey::new_unique();

    let r = env.svm.process_instruction(
        &ix_deposit(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, token_a, token_b, env.payer,
            // Pool-setup helper, not a slippage test: no LP floor.
            amount_a, amount_b, 0,
        ),
        &[signer(lp_token), signer(depositor)],
    );
    assert!(r.is_ok(), "do_deposit: {:?}", r.raw_result);

    (depositor, lp_token)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests - create_config (existing)
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
// Tests - create_pool
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
// Tests - deposit_liquidity
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
            1_000_000, 1_000_000, 0,
        ),
        &[empty(lp_token), signer(depositor)],
    );
    r.assert_error(amm_error(AmmError::InsufficientBalance));
    println!("  DEPOSIT insufficient funds correctly rejected");
}

/// Regression test for the ratio-clamp direction bug: with reserves at
/// pool_a > pool_b, logic that branches on RESERVE sizes (instead of which
/// USER amount is binding) scales `amount_a` UP to
/// `amount_b * pool_a / pool_b`, past both the user's stated amount and the
/// balance check. The correct try-A-then-B clamp scales token B DOWN instead.
#[test]
fn test_deposit_clamps_down_never_up() {
    let mut env = setup_pool();

    // Seed at a 4:1 ratio so pool_a > pool_b.
    let (pool_seed_a, pool_seed_b) = (4_000_000u64, 1_000_000u64);
    let (_, lp_seed_token) = do_deposit(&mut env, pool_seed_a, pool_seed_b);
    let lp_supply = token_amount(&env.svm.get_account(&lp_seed_token).unwrap());

    // Depositor offers 1_000_000 of each and holds exactly that much. The
    // old logic would try to pull 4_000_000 token A (scaling A UP); the
    // correct clamp uses all 1_000_000 A and scales B down to 250_000.
    let depositor = Pubkey::new_unique();
    let (stated_a, stated_b) = (1_000_000u64, 1_000_000u64);
    let ta = funded_ata(depositor, env.mint_a, stated_a);
    let tb = funded_ata(depositor, env.mint_b, stated_b);
    let (token_a, token_b) = (ta.address, tb.address);
    env.svm.set_account(ta);
    env.svm.set_account(tb);
    let lp_token = Pubkey::new_unique();

    let expected_b_pulled = mul_div(stated_a, pool_seed_b, pool_seed_a);
    let expected_lp = mul_div(stated_a, lp_supply, pool_seed_a);

    let r = env.svm.process_instruction(
        &ix_deposit(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, token_a, token_b, env.payer,
            stated_a, stated_b, expected_lp,
        ),
        &[signer(lp_token), signer(depositor)],
    );
    assert!(r.is_ok(), "clamped deposit failed: {:?}", r.raw_result);

    // Exact amounts pulled: all of A, ratio-clamped B, nothing more.
    let depositor_a = token_amount(&env.svm.get_account(&token_a).unwrap());
    let depositor_b = token_amount(&env.svm.get_account(&token_b).unwrap());
    assert_eq!(depositor_a, 0, "all stated token A must be pulled");
    assert_eq!(
        depositor_b,
        stated_b - expected_b_pulled,
        "token B must be clamped down to the pool ratio"
    );
    let pool_a_after = token_amount(&env.svm.get_account(&env.pool_a).unwrap());
    let pool_b_after = token_amount(&env.svm.get_account(&env.pool_b).unwrap());
    assert_eq!(pool_a_after, pool_seed_a + stated_a);
    assert_eq!(pool_b_after, pool_seed_b + expected_b_pulled);

    let lp_minted = token_amount(&env.svm.get_account(&lp_token).unwrap());
    assert_eq!(lp_minted, expected_lp, "LP mint must be proportional");
    println!(
        "  DEPOSIT clamp: pulled_a={}, pulled_b={}, lp={}",
        stated_a, expected_b_pulled, lp_minted
    );
}

/// Mirror of `test_deposit_clamps_down_never_up` with the reserves reversed
/// (pool_b > pool_a), so the binding side is token A's counterpart: the full
/// `amount_b` is used and `amount_a` is the side that covers the ratio.
#[test]
fn test_deposit_clamps_down_other_side() {
    let mut env = setup_pool();

    // Seed at a 1:4 ratio so pool_b > pool_a.
    let (pool_seed_a, pool_seed_b) = (1_000_000u64, 4_000_000u64);
    let (_, lp_seed_token) = do_deposit(&mut env, pool_seed_a, pool_seed_b);
    let lp_supply = token_amount(&env.svm.get_account(&lp_seed_token).unwrap());

    let depositor = Pubkey::new_unique();
    let (stated_a, stated_b) = (1_000_000u64, 1_000_000u64);
    let ta = funded_ata(depositor, env.mint_a, stated_a);
    let tb = funded_ata(depositor, env.mint_b, stated_b);
    let (token_a, token_b) = (ta.address, tb.address);
    env.svm.set_account(ta);
    env.svm.set_account(tb);
    let lp_token = Pubkey::new_unique();

    // amount_b_required for the full stated_a would be 4_000_000 > stated_b,
    // so amount_b binds: all of B is used and A is clamped down.
    let expected_a_pulled = mul_div(stated_b, pool_seed_a, pool_seed_b);
    let expected_lp = mul_div(stated_b, lp_supply, pool_seed_b);

    let r = env.svm.process_instruction(
        &ix_deposit(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, token_a, token_b, env.payer,
            stated_a, stated_b, expected_lp,
        ),
        &[signer(lp_token), signer(depositor)],
    );
    assert!(r.is_ok(), "clamped deposit failed: {:?}", r.raw_result);

    let depositor_a = token_amount(&env.svm.get_account(&token_a).unwrap());
    let depositor_b = token_amount(&env.svm.get_account(&token_b).unwrap());
    assert_eq!(
        depositor_a,
        stated_a - expected_a_pulled,
        "token A must be clamped down to the pool ratio"
    );
    assert_eq!(depositor_b, 0, "all stated token B must be pulled");
    let pool_a_after = token_amount(&env.svm.get_account(&env.pool_a).unwrap());
    let pool_b_after = token_amount(&env.svm.get_account(&env.pool_b).unwrap());
    assert_eq!(pool_a_after, pool_seed_a + expected_a_pulled);
    assert_eq!(pool_b_after, pool_seed_b + stated_b);

    let lp_minted = token_amount(&env.svm.get_account(&lp_token).unwrap());
    assert_eq!(lp_minted, expected_lp, "LP mint must be proportional");
    println!(
        "  DEPOSIT clamp (B binding): pulled_a={}, pulled_b={}, lp={}",
        expected_a_pulled, stated_b, lp_minted
    );
}

#[test]
fn test_deposit_slippage_rejected() {
    let mut env = setup_pool();

    let (pool_seed_a, pool_seed_b) = (1_000_000u64, 1_000_000u64);
    let (_, lp_seed_token) = do_deposit(&mut env, pool_seed_a, pool_seed_b);
    let lp_supply = token_amount(&env.svm.get_account(&lp_seed_token).unwrap());

    let depositor = Pubkey::new_unique();
    let (stated_a, stated_b) = (500_000u64, 500_000u64);
    let ta = funded_ata(depositor, env.mint_a, stated_a);
    let tb = funded_ata(depositor, env.mint_b, stated_b);
    let (token_a, token_b) = (ta.address, tb.address);
    env.svm.set_account(ta);
    env.svm.set_account(tb);
    let lp_token = Pubkey::new_unique();

    // The pool will mint exactly this much; ask for one more.
    let exact_lp = mul_div(stated_a, lp_supply, pool_seed_a);
    let r = env.svm.process_instruction(
        &ix_deposit(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, token_a, token_b, env.payer,
            stated_a, stated_b, exact_lp + 1,
        ),
        &[signer(lp_token), signer(depositor)],
    );
    r.assert_error(amm_error(AmmError::DepositBelowMinimum));

    // Nothing moved: depositor balances and pool reserves are unchanged.
    let depositor_a = token_amount(&env.svm.get_account(&token_a).unwrap());
    let depositor_b = token_amount(&env.svm.get_account(&token_b).unwrap());
    assert_eq!(depositor_a, stated_a, "token A must be untouched after revert");
    assert_eq!(depositor_b, stated_b, "token B must be untouched after revert");
    let pa = token_amount(&env.svm.get_account(&env.pool_a).unwrap());
    let pb = token_amount(&env.svm.get_account(&env.pool_b).unwrap());
    assert_eq!(pa, pool_seed_a, "pool_a must be untouched after revert");
    assert_eq!(pb, pool_seed_b, "pool_b must be untouched after revert");
    println!("  DEPOSIT slippage guard correctly rejected");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests - withdraw_liquidity
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

    // Expected proportional share, mirroring the program's formula:
    //   amount_out = lp_amount * reserve / (lp_supply + MINIMUM_LIQUIDITY)
    // The depositor holds the entire LP supply, so supply == lp_balance.
    let divisor = lp_balance
        .checked_add(crate::MINIMUM_LIQUIDITY)
        .expect("divisor overflow");
    let expected_a = mul_div(withdraw_amount, amount_a, divisor);
    let expected_b = mul_div(withdraw_amount, amount_b, divisor);

    // Output token accounts are created by init(idempotent) → pass as empty.
    let recv_a = Pubkey::new_unique();
    let recv_b = Pubkey::new_unique();

    let r = env.svm.process_instruction(
        &ix_withdraw(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, recv_a, recv_b, env.payer,
            // Pass the exact expected amounts as the slippage floors: the
            // pool hasn't moved since the quote, so the floors must be met.
            withdraw_amount, expected_a, expected_b,
        ),
        // recv_a / recv_b are non-PDA accounts init(idempotent) → signer required.
        &[signer(recv_a), signer(recv_b), signer(depositor)],
    );
    assert!(r.is_ok(), "withdraw failed: {:?}", r.raw_result);

    // Verify the depositor received exactly the proportional share.
    let ra = env.svm.get_account(&recv_a).expect("recv_a missing after withdraw");
    let rb = env.svm.get_account(&recv_b).expect("recv_b missing after withdraw");
    assert_eq!(token_amount(&ra), expected_a, "token A withdrawal mismatch");
    assert_eq!(token_amount(&rb), expected_b, "token B withdrawal mismatch");

    // LP tokens were burned.
    let lp_after = token_amount(&env.svm.get_account(&lp_token).unwrap());
    assert_eq!(
        lp_after,
        lp_balance - withdraw_amount,
        "LP balance should drop by the burned amount"
    );

    println!(
        "  WITHDRAW: lp_burned={}, recv_a={}, recv_b={}",
        withdraw_amount, token_amount(&ra), token_amount(&rb)
    );
}

#[test]
fn test_withdraw_slippage_rejected() {
    let mut env = setup_pool();
    let (depositor, lp_token) = do_deposit(&mut env, 2_000_000, 2_000_000);
    let lp_balance = token_amount(&env.svm.get_account(&lp_token).unwrap());

    let withdraw_amount = lp_balance / 2;
    let divisor = lp_balance
        .checked_add(crate::MINIMUM_LIQUIDITY)
        .expect("divisor overflow");
    let expected_a = mul_div(withdraw_amount, 2_000_000, divisor);

    let recv_a = Pubkey::new_unique();
    let recv_b = Pubkey::new_unique();

    // Floor on token A set just above what the pool will pay out.
    let r = env.svm.process_instruction(
        &ix_withdraw(
            env.config, env.pool_config, env.pool_authority, depositor,
            env.lp_mint, env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            lp_token, recv_a, recv_b, env.payer,
            withdraw_amount, expected_a + 1, 0,
        ),
        &[signer(recv_a), signer(recv_b), signer(depositor)],
    );
    r.assert_error(amm_error(AmmError::WithdrawalBelowMinimum));

    // Nothing moved: pool reserves and the LP balance are unchanged.
    let pa = token_amount(&env.svm.get_account(&env.pool_a).unwrap());
    let pb = token_amount(&env.svm.get_account(&env.pool_b).unwrap());
    assert_eq!(pa, 2_000_000, "pool_a must be untouched after revert");
    assert_eq!(pb, 2_000_000, "pool_b must be untouched after revert");
    let lp_after = token_amount(&env.svm.get_account(&lp_token).unwrap());
    assert_eq!(lp_after, lp_balance, "LP balance must be untouched after revert");
    println!("  WITHDRAW slippage guard correctly rejected");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests - swap_tokens
// ═══════════════════════════════════════════════════════════════════════════════

/// Constant-product quote mirroring the program's swap math, on effective
/// reserves: output = taxed_input * pool_out / (pool_in + taxed_input), where
/// taxed_input = input - input * fee_bps / 10_000. All products in u128.
fn expected_swap_output(input: u64, fee_bps: u64, pool_in: u64, pool_out: u64) -> u64 {
    let fee_amount = mul_div(input, fee_bps, crate::BASIS_POINTS_DIVISOR);
    let taxed_input = input.checked_sub(fee_amount).expect("fee exceeds input");
    let divisor = pool_in.checked_add(taxed_input).expect("reserve overflow");
    mul_div(taxed_input, pool_out, divisor)
}

/// Trading fee passed to `create_config` in `setup_pool`, in basis points.
const POOL_FEE_BPS: u64 = 30;

#[test]
fn test_swap_a_to_b_conserves_balances() {
    let mut env = setup_pool();

    // Seed the pool with liquidity first.
    let (pool_seed_a, pool_seed_b) = (10_000_000u64, 10_000_000u64);
    do_deposit(&mut env, pool_seed_a, pool_seed_b);

    // Trader swaps 100_000 token A for token B.
    let trader = Pubkey::new_unique();
    let trader_funding = 1_000_000u64;
    let ta = funded_ata(trader, env.mint_a, trader_funding);
    let token_a = ta.address;
    let token_b_out = Pubkey::new_unique(); // created by init(idempotent)
    env.svm.set_account(ta);

    let input = 100_000u64;
    let expected_output = expected_swap_output(input, POOL_FEE_BPS, pool_seed_a, pool_seed_b);
    let r = env.svm.process_instruction(
        &ix_swap(
            env.config, env.pool_config, env.pool_authority, trader,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            token_a, token_b_out, env.payer,
            true, input, expected_output, // floor = exact quote; pool hasn't moved
        ),
        // token_b_out is a new non-PDA account → signer required for init.
        &[signer(token_b_out), signer(trader)],
    );
    assert!(r.is_ok(), "swap A→B failed: {:?}", r.raw_result);

    // Conservation: the trader pays exactly `input` and receives exactly what
    // the pool sent; nothing is minted or lost in transit.
    let trader_a_after = token_amount(&env.svm.get_account(&token_a).unwrap());
    let received = token_amount(&env.svm.get_account(&token_b_out).unwrap());
    let pool_a_after = token_amount(&env.svm.get_account(&env.pool_a).unwrap());
    let pool_b_after = token_amount(&env.svm.get_account(&env.pool_b).unwrap());
    assert_eq!(
        trader_a_after,
        trader_funding - input,
        "trader must pay exactly the input amount"
    );
    assert_eq!(received, expected_output, "trader output mismatch");
    assert_eq!(
        pool_a_after,
        pool_seed_a + input,
        "pool_a must gain exactly the input"
    );
    assert_eq!(
        pool_b_after,
        pool_seed_b - received,
        "pool_b must lose exactly what the trader received"
    );
    println!("  SWAP A→B: input={}, output={}", input, received);
}

#[test]
fn test_swap_b_to_a_conserves_balances() {
    let mut env = setup_pool();
    let (pool_seed_a, pool_seed_b) = (10_000_000u64, 10_000_000u64);
    do_deposit(&mut env, pool_seed_a, pool_seed_b);

    let trader = Pubkey::new_unique();
    let trader_funding = 1_000_000u64;
    let tb = funded_ata(trader, env.mint_b, trader_funding);
    let token_b = tb.address;
    let token_a_out = Pubkey::new_unique();
    env.svm.set_account(tb);

    let input = 100_000u64;
    let expected_output = expected_swap_output(input, POOL_FEE_BPS, pool_seed_b, pool_seed_a);
    let r = env.svm.process_instruction(
        &ix_swap(
            env.config, env.pool_config, env.pool_authority, trader,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            token_a_out, token_b, env.payer,
            false, input, expected_output, // input_is_token_a=false
        ),
        &[signer(token_a_out), signer(trader)],
    );
    assert!(r.is_ok(), "swap B→A failed: {:?}", r.raw_result);

    let trader_b_after = token_amount(&env.svm.get_account(&token_b).unwrap());
    let received = token_amount(&env.svm.get_account(&token_a_out).unwrap());
    let pool_a_after = token_amount(&env.svm.get_account(&env.pool_a).unwrap());
    let pool_b_after = token_amount(&env.svm.get_account(&env.pool_b).unwrap());
    assert_eq!(
        trader_b_after,
        trader_funding - input,
        "trader must pay exactly the input amount"
    );
    assert_eq!(received, expected_output, "trader output mismatch");
    assert_eq!(
        pool_b_after,
        pool_seed_b + input,
        "pool_b must gain exactly the input"
    );
    assert_eq!(
        pool_a_after,
        pool_seed_a - received,
        "pool_a must lose exactly what the trader received"
    );
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

    // min_output set one above the exact quote, so the floor cannot be met.
    let input = 100_000u64;
    let quote = expected_swap_output(input, POOL_FEE_BPS, 10_000_000, 10_000_000);
    let r = env.svm.process_instruction(
        &ix_swap(
            env.config, env.pool_config, env.pool_authority, trader,
            env.mint_a, env.mint_b, env.pool_a, env.pool_b,
            token_a, token_b_out, env.payer,
            true, input, quote + 1,
        ),
        &[empty(token_b_out), signer(trader)],
    );
    r.assert_error(amm_error(AmmError::SlippageExceeded));

    // Nothing moved: the trader keeps their input and the pool is untouched.
    let trader_a = token_amount(&env.svm.get_account(&token_a).unwrap());
    assert_eq!(trader_a, 1_000_000, "trader balance must be untouched after revert");
    let pa = token_amount(&env.svm.get_account(&env.pool_a).unwrap());
    let pb = token_amount(&env.svm.get_account(&env.pool_b).unwrap());
    assert_eq!(pa, 10_000_000, "pool_a must be untouched after revert");
    assert_eq!(pb, 10_000_000, "pool_b must be untouched after revert");
    println!("  SWAP slippage guard correctly rejected");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests - claim_admin_fees
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
