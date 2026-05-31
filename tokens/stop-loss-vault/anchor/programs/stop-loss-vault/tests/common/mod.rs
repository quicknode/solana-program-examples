//! Shared test harness for the stop-loss-vault scenarios.
//!
//! The scenarios in `stop_loss_vault_scenarios.rs` read like stories: "Alice
//! deposits 10 SOL, Bob cranks, price drops, Alice pulls out stables". This
//! module hides the LiteSVM plumbing so the scenarios stay readable.
//!
//! The test harness deliberately uses `anchor_lang::solana_program` types
//! everywhere so there's no version skew with Anchor 1.0.2's pubkey/
//! instruction types. SPL instructions are built by hand (the discriminators
//! and serialisation are trivial) rather than via the older `spl-token` /
//! `spl-associated-token-account` crates, which pull in mismatching versions
//! of `solana-program`.

use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::pubkey::Pubkey;
use anchor_lang::solana_program::rent;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::system_program;
use anchor_lang::{Discriminator, InstructionData, ToAccountMetas};
use anchor_spl::associated_token::get_associated_token_address;
use borsh::BorshDeserialize;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

/// Native SOL has 9 decimals (lamports per SOL). Used only to fund accounts
/// with rent and transaction fees.
pub const SOL_DECIMALS: u8 = 9;
/// The volatile token (read it as NVDAx, a tokenized Nvidia share). A
/// 9-decimal stand-in for whatever volatile asset a real vault would hold.
pub const NVDAX_DECIMALS: u8 = 9;
/// USDC has 6 decimals.
pub const USDC_DECIMALS: u8 = 6;
/// Switchboard On-Demand prices come in with 8 decimal places by convention.
/// Real Switchboard feeds expose `scale` directly; the mock uses the same
/// value so the price-comparison logic is identical to production.
pub const ORACLE_SCALE: u32 = 8;

/// Length of a packed SPL token Mint account, copied from
/// `spl_token::state::Mint::LEN`. Used to compute rent for new mint accounts.
pub const SPL_TOKEN_MINT_LEN: usize = 82;

/// Convert a USD price (e.g. $200) into the fixed-point i128 the oracle uses.
pub fn dollars_to_oracle_price(dollars: u128) -> i128 {
    (dollars * 10u128.pow(ORACLE_SCALE)) as i128
}

pub fn sol(amount: u64) -> u64 {
    amount * 10u64.pow(SOL_DECIMALS as u32)
}
pub fn nvdax(amount: u64) -> u64 {
    amount * 10u64.pow(NVDAX_DECIMALS as u32)
}
pub fn usdc(amount: u64) -> u64 {
    amount * 10u64.pow(USDC_DECIMALS as u32)
}

pub struct TestWorld {
    pub svm: LiteSVM,
    pub mint_authority: Keypair,
    pub volatile_mint: Pubkey,
    pub stable_mint: Pubkey,
    pub pool_authority: Pubkey,
    pub pool_volatile_account: Pubkey,
    pub pool_stable_account: Pubkey,
    pub feed: Keypair,
    pub feed_authority: Keypair,
}

pub fn new_world() -> TestWorld {
    let mut svm = LiteSVM::new();
    svm.add_program(
        stop_loss_vault::id(),
        include_bytes!("../../../../target/deploy/stop_loss_vault.so"),
    )
    .unwrap();
    svm.add_program(
        mock_jupiter::id(),
        include_bytes!("../../../../target/deploy/mock_jupiter.so"),
    )
    .unwrap();
    svm.add_program(
        mock_switchboard::id(),
        include_bytes!("../../../../target/deploy/mock_switchboard.so"),
    )
    .unwrap();

    let mint_authority = Keypair::new();
    svm.airdrop(&mint_authority.pubkey(), sol(10)).unwrap();

    let volatile_mint = create_mint(&mut svm, &mint_authority, NVDAX_DECIMALS);
    let stable_mint = create_mint(&mut svm, &mint_authority, USDC_DECIMALS);

    let (pool_authority, _bump) = Pubkey::find_program_address(
        &[mock_jupiter::POOL_AUTHORITY_SEED],
        &mock_jupiter::id(),
    );

    let pool_volatile_account =
        create_ata(&mut svm, &mint_authority, &pool_authority, &volatile_mint);
    let pool_stable_account =
        create_ata(&mut svm, &mint_authority, &pool_authority, &stable_mint);

    // Pre-fund the mock pool with stables so it can pay every test swap.
    mint_to(
        &mut svm,
        &mint_authority,
        &stable_mint,
        &pool_stable_account,
        usdc(1_000_000_000),
    );

    let feed_authority = Keypair::new();
    svm.airdrop(&feed_authority.pubkey(), sol(10)).unwrap();
    let feed = Keypair::new();

    TestWorld {
        svm,
        mint_authority,
        volatile_mint,
        stable_mint,
        pool_authority,
        pool_volatile_account,
        pool_stable_account,
        feed,
        feed_authority,
    }
}

// ---- minimal SPL Token instruction builders (avoid version-skewed crates) ----

/// Token program ID (classic SPL Token, not Token-2022).
pub const TOKEN_PROGRAM_ID: Pubkey = anchor_spl::token::ID;
/// Associated Token Account program ID.
pub const ATA_PROGRAM_ID: Pubkey = anchor_spl::associated_token::ID;

fn token_initialize_mint_ix(
    mint: &Pubkey,
    mint_authority: &Pubkey,
    decimals: u8,
) -> Instruction {
    // SPL Token instruction layout for `InitializeMint`:
    //   tag (u8 = 0)
    //   decimals (u8)
    //   mint_authority (32 bytes)
    //   freeze_authority option: tag (u8 = 0 / 1) + 32 bytes if Some
    let mut data = Vec::with_capacity(1 + 1 + 32 + 1);
    data.push(0);
    data.push(decimals);
    data.extend_from_slice(mint_authority.as_ref());
    data.push(0); // freeze_authority = None
    Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*mint, false),
            AccountMeta::new_readonly(rent::ID, false),
        ],
        data,
    }
}

fn token_mint_to_ix(
    mint: &Pubkey,
    destination: &Pubkey,
    mint_authority: &Pubkey,
    amount: u64,
) -> Instruction {
    // SPL Token instruction `MintTo` (tag = 7) + u64 amount.
    let mut data = Vec::with_capacity(1 + 8);
    data.push(7);
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*mint, false),
            AccountMeta::new(*destination, false),
            AccountMeta::new_readonly(*mint_authority, true),
        ],
        data,
    }
}

fn ata_create_idempotent_ix(
    payer: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Instruction {
    let ata = get_associated_token_address(owner, mint);
    // `CreateIdempotent` is discriminator 1; `Create` is 0. Either works here
    // but idempotent is safer if a test ever calls twice.
    Instruction {
        program_id: ATA_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(*owner, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ],
        data: vec![1u8],
    }
}

pub fn create_mint(svm: &mut LiteSVM, mint_authority: &Keypair, decimals: u8) -> Pubkey {
    let mint = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(SPL_TOKEN_MINT_LEN);
    let create_account_ix = system_instruction::create_account(
        &mint_authority.pubkey(),
        &mint.pubkey(),
        rent,
        SPL_TOKEN_MINT_LEN as u64,
        &TOKEN_PROGRAM_ID,
    );
    let init_mint_ix = token_initialize_mint_ix(&mint.pubkey(), &mint_authority.pubkey(), decimals);
    send_tx(
        svm,
        &[create_account_ix, init_mint_ix],
        mint_authority,
        &[mint_authority, &mint],
    );
    mint.pubkey()
}

pub fn create_ata(
    svm: &mut LiteSVM,
    payer: &Keypair,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Pubkey {
    let ata = get_associated_token_address(owner, mint);
    let ix = ata_create_idempotent_ix(&payer.pubkey(), owner, mint);
    send_tx(svm, &[ix], payer, &[payer]);
    ata
}

pub fn mint_to(
    svm: &mut LiteSVM,
    mint_authority: &Keypair,
    mint: &Pubkey,
    destination: &Pubkey,
    amount: u64,
) {
    let ix = token_mint_to_ix(mint, destination, &mint_authority.pubkey(), amount);
    send_tx(svm, &[ix], mint_authority, &[mint_authority]);
}

/// SPL Token account layout: `mint(32) + owner(32) + amount(u64) + ...`.
/// We only need the amount, at offset 64.
pub fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let account = svm.get_account(ata).expect("ATA missing");
    let amount_bytes: [u8; 8] = account.data[64..72].try_into().unwrap();
    u64::from_le_bytes(amount_bytes)
}

pub fn vault_address(owner: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[stop_loss_vault::Vault::SEED_PREFIX, owner.as_ref()],
        &stop_loss_vault::id(),
    )
}

pub fn send_tx(
    svm: &mut LiteSVM,
    instructions: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(instructions, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap();
}

pub fn try_send_tx(
    svm: &mut LiteSVM,
    instructions: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<(), litesvm::types::FailedTransactionMetadata> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(instructions, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).map(|_| ())
}

pub fn initialize_feed(world: &mut TestWorld, price: i128) {
    let ix_data = mock_switchboard::instruction::InitializeFeed {
        price,
        scale: ORACLE_SCALE,
    }
    .data();
    let accounts = mock_switchboard::accounts::InitializeFeed {
        feed: world.feed.pubkey(),
        authority: world.feed_authority.pubkey(),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: mock_switchboard::id(),
        accounts,
        data: ix_data,
    };
    let payer = world.feed_authority.insecure_clone();
    let feed_kp = world.feed.insecure_clone();
    send_tx(&mut world.svm, &[ix], &payer, &[&payer, &feed_kp]);
}

pub fn set_feed_price(world: &mut TestWorld, price: i128) {
    let ix_data = mock_switchboard::instruction::SetPrice { price }.data();
    let accounts = mock_switchboard::accounts::SetPrice {
        feed: world.feed.pubkey(),
        authority: world.feed_authority.pubkey(),
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: mock_switchboard::id(),
        accounts,
        data: ix_data,
    };
    let payer = world.feed_authority.insecure_clone();
    send_tx(&mut world.svm, &[ix], &payer, &[&payer]);
    // LiteSVM (and real validators) reject byte-identical resent transactions.
    // Tests that call `set_feed_price` repeatedly with the same price would
    // produce identical bytes; expire the blockhash so the next tx is fresh.
    world.svm.expire_blockhash();
}

pub fn initialize_vault(
    world: &mut TestWorld,
    owner: &Keypair,
    threshold_price: i128,
    crank_interval_seconds: u32,
) -> Pubkey {
    let (vault, _bump) = vault_address(&owner.pubkey());
    let vault_volatile = get_associated_token_address(&vault, &world.volatile_mint);
    let vault_stable = get_associated_token_address(&vault, &world.stable_mint);

    let ix_data = stop_loss_vault::instruction::InitializeVault {
        threshold_price,
        crank_interval_seconds,
        tuktuk_task: Pubkey::default(),
    }
    .data();
    let accounts = stop_loss_vault::accounts::InitializeVaultAccountConstraints {
        vault,
        volatile_mint: world.volatile_mint,
        stable_mint: world.stable_mint,
        oracle_feed: world.feed.pubkey(),
        vault_volatile_account: vault_volatile,
        vault_stable_account: vault_stable,
        owner: owner.pubkey(),
        token_program: TOKEN_PROGRAM_ID,
        associated_token_program: ATA_PROGRAM_ID,
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: stop_loss_vault::id(),
        accounts,
        data: ix_data,
    };
    send_tx(&mut world.svm, &[ix], owner, &[owner]);
    vault
}

pub fn deposit(world: &mut TestWorld, owner: &Keypair, amount: u64) {
    let (vault, _bump) = vault_address(&owner.pubkey());
    let vault_volatile = get_associated_token_address(&vault, &world.volatile_mint);
    let owner_volatile = get_associated_token_address(&owner.pubkey(), &world.volatile_mint);
    let ix_data = stop_loss_vault::instruction::Deposit { amount }.data();
    let accounts = stop_loss_vault::accounts::DepositAccountConstraints {
        vault,
        volatile_mint: world.volatile_mint,
        vault_volatile_account: vault_volatile,
        owner_volatile_account: owner_volatile,
        owner: owner.pubkey(),
        token_program: TOKEN_PROGRAM_ID,
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: stop_loss_vault::id(),
        accounts,
        data: ix_data,
    };
    send_tx(&mut world.svm, &[ix], owner, &[owner]);
}

pub fn try_convert_if_triggered(
    world: &mut TestWorld,
    cranker: &Keypair,
    vault_owner: &Pubkey,
) -> Result<(), litesvm::types::FailedTransactionMetadata> {
    let (vault, _bump) = vault_address(vault_owner);
    let vault_volatile = get_associated_token_address(&vault, &world.volatile_mint);
    let vault_stable = get_associated_token_address(&vault, &world.stable_mint);

    let ix_data = stop_loss_vault::instruction::ConvertIfTriggered {
        switchboard_price_update_data: Vec::new(),
    }
    .data();
    let accounts = stop_loss_vault::accounts::ConvertIfTriggeredAccountConstraints {
        vault,
        volatile_mint: world.volatile_mint,
        stable_mint: world.stable_mint,
        oracle_feed: world.feed.pubkey(),
        vault_volatile_account: vault_volatile,
        vault_stable_account: vault_stable,
        pool_volatile_account: world.pool_volatile_account,
        pool_stable_account: world.pool_stable_account,
        pool_authority: world.pool_authority,
        swap_program: mock_jupiter::id(),
        cranker: cranker.pubkey(),
        token_program: TOKEN_PROGRAM_ID,
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: stop_loss_vault::id(),
        accounts,
        data: ix_data,
    };
    try_send_tx(&mut world.svm, &[ix], cranker, &[cranker])
}

pub fn try_update_threshold(
    world: &mut TestWorld,
    caller: &Keypair,
    new_threshold_price: Option<i128>,
    new_crank_interval_seconds: Option<u32>,
) -> Result<(), litesvm::types::FailedTransactionMetadata> {
    let (vault, _bump) = vault_address(&caller.pubkey());
    let ix_data = stop_loss_vault::instruction::UpdateThreshold {
        new_threshold_price,
        new_crank_interval_seconds,
    }
    .data();
    let accounts = stop_loss_vault::accounts::UpdateThresholdAccountConstraints {
        vault,
        owner: caller.pubkey(),
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: stop_loss_vault::id(),
        accounts,
        data: ix_data,
    };
    try_send_tx(&mut world.svm, &[ix], caller, &[caller])
}

pub fn try_withdraw_volatile(
    world: &mut TestWorld,
    caller: &Keypair,
    vault_owner_for_pda: &Pubkey,
    amount: u64,
) -> Result<(), litesvm::types::FailedTransactionMetadata> {
    let (vault, _bump) = vault_address(vault_owner_for_pda);
    let vault_volatile = get_associated_token_address(&vault, &world.volatile_mint);
    let owner_volatile = get_associated_token_address(&caller.pubkey(), &world.volatile_mint);
    let ix_data = stop_loss_vault::instruction::WithdrawVolatile { amount }.data();
    let accounts = stop_loss_vault::accounts::WithdrawVolatileAccountConstraints {
        vault,
        volatile_mint: world.volatile_mint,
        vault_volatile_account: vault_volatile,
        owner_volatile_account: owner_volatile,
        owner: caller.pubkey(),
        token_program: TOKEN_PROGRAM_ID,
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: stop_loss_vault::id(),
        accounts,
        data: ix_data,
    };
    try_send_tx(&mut world.svm, &[ix], caller, &[caller])
}

pub fn try_withdraw_stables(
    world: &mut TestWorld,
    caller: &Keypair,
    vault_owner_for_pda: &Pubkey,
    amount: u64,
) -> Result<(), litesvm::types::FailedTransactionMetadata> {
    let (vault, _bump) = vault_address(vault_owner_for_pda);
    let vault_stable = get_associated_token_address(&vault, &world.stable_mint);
    let owner_stable = get_associated_token_address(&caller.pubkey(), &world.stable_mint);
    let ix_data = stop_loss_vault::instruction::WithdrawStables { amount }.data();
    let accounts = stop_loss_vault::accounts::WithdrawStablesAccountConstraints {
        vault,
        stable_mint: world.stable_mint,
        vault_stable_account: vault_stable,
        owner_stable_account: owner_stable,
        owner: caller.pubkey(),
        token_program: TOKEN_PROGRAM_ID,
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: stop_loss_vault::id(),
        accounts,
        data: ix_data,
    };
    try_send_tx(&mut world.svm, &[ix], caller, &[caller])
}

pub fn fund_with_volatile(world: &mut TestWorld, actor: &Keypair, amount: u64) -> Pubkey {
    let ata = create_ata(&mut world.svm, actor, &actor.pubkey(), &world.volatile_mint);
    let mint_authority = world.mint_authority.insecure_clone();
    mint_to(&mut world.svm, &mint_authority, &world.volatile_mint, &ata, amount);
    ata
}

pub fn create_stable_ata(world: &mut TestWorld, actor: &Keypair) -> Pubkey {
    create_ata(&mut world.svm, actor, &actor.pubkey(), &world.stable_mint)
}

pub fn vault_state(svm: &LiteSVM, vault: &Pubkey) -> stop_loss_vault::Vault {
    let account = svm.get_account(vault).expect("vault missing");
    stop_loss_vault::Vault::try_from_slice(
        &account.data[stop_loss_vault::Vault::DISCRIMINATOR.len()..],
    )
    .unwrap()
}

pub fn new_funded_keypair(svm: &mut LiteSVM, lamports: u64) -> Keypair {
    let kp = Keypair::new();
    svm.airdrop(&kp.pubkey(), lamports).unwrap();
    kp
}

/// Advance the SVM clock past the vault's price-staleness bound so a price set
/// before this call reads as stale to `convert_if_triggered`.
pub fn warp_past_price_staleness(world: &mut TestWorld) {
    let clock = world
        .svm
        .get_sysvar::<anchor_lang::solana_program::clock::Clock>();
    world
        .svm
        .warp_to_slot(clock.slot + stop_loss_vault::MAX_PRICE_STALENESS_SLOTS + 1);
}
