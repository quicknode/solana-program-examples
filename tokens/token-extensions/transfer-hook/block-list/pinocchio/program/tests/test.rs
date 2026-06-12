use block_list_client::client::{
    instructions::{
        BlockWallet, Init, SetupExtraMetas, SetupExtraMetasInstructionArgs, UnblockWallet,
    },
    programs::BLOCK_LIST_ID,
};
use litesvm::LiteSVM;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address_with_program_id, instruction::create_associated_token_account,
};
use spl_token_2022::{
    extension::ExtensionType,
    instruction::{initialize_mint2, mint_to_checked, transfer_checked},
    state::Mint,
};

// The .so is built into this project's workspace target/deploy by
// `cargo build-sbf --manifest-path=./program/Cargo.toml` (run from the
// project root). Rebuild after every program change: the binary is embedded
// at test-compile time, so a stale .so silently tests old code.
const PROGRAM_SO: &[u8] = include_bytes!("../../target/deploy/block_list.so");

const DECIMALS: u8 = 6;
const MINT_AMOUNT: u64 = 1_000 * 10u64.pow(DECIMALS as u32);
const TRANSFER_AMOUNT: u64 = 10 * 10u64.pow(DECIMALS as u32);

// Hook PDA derivations, mirroring program/src (seeds "config", "wallet_block",
// and the transfer-hook interface's "extra-account-metas").
fn find_config_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"config"], &BLOCK_LIST_ID).0
}

fn find_wallet_block_pda(wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"wallet_block", wallet.as_ref()], &BLOCK_LIST_ID).0
}

fn find_extra_metas_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"extra-account-metas", mint.as_ref()], &BLOCK_LIST_ID).0
}

enum ExtraMode {
    Empty,
    SourceOnly,
}

#[allow(clippy::too_many_arguments)]
fn build_transfer_with_hook_accounts(
    source: &Pubkey,
    mint: &Pubkey,
    destination: &Pubkey,
    owner: &Pubkey,
    source_owner: &Pubkey,
    extra_mode: ExtraMode,
) -> Instruction {
    let mut instruction = transfer_checked(
        &spl_token_2022::id(),
        source,
        mint,
        destination,
        owner,
        &[],
        TRANSFER_AMOUNT,
        DECIMALS,
    )
    .unwrap();

    // Token Extensions invokes the hook with these trailing accounts in this
    // order:
    //   validation_pda (extra-account-metas)
    //   resolved wallet_block for the source TA (when listed in the metas)
    // The hook program id is appended last so the Token Extensions transfer
    // instruction handler can CPI into it (it strips that entry from the hook
    // accounts list).
    instruction
        .accounts
        .push(AccountMeta::new_readonly(find_extra_metas_pda(mint), false));
    if let ExtraMode::SourceOnly = extra_mode {
        instruction.accounts.push(AccountMeta::new_readonly(
            find_wallet_block_pda(source_owner),
            false,
        ));
    }
    instruction
        .accounts
        .push(AccountMeta::new_readonly(BLOCK_LIST_ID, false));
    instruction
}

fn send_expecting_success(
    svm: &mut LiteSVM,
    instructions: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
    label: &str,
) {
    // Identical transactions across steps (same signers, instruction, and
    // blockhash) collide on signature and are rejected as AlreadyProcessed.
    // Expiring first gives each send a fresh blockhash and unique signature.
    svm.expire_blockhash();
    let transaction = Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        signers,
        svm.latest_blockhash(),
    );
    if let Err(failure) = svm.send_transaction(transaction) {
        panic!(
            "{label} failed: {:?}\nlogs:\n{}",
            failure.err,
            failure.meta.logs.join("\n")
        );
    }
}

fn send_expecting_failure(
    svm: &mut LiteSVM,
    instructions: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
    label: &str,
) -> Vec<String> {
    svm.expire_blockhash();
    let transaction = Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        signers,
        svm.latest_blockhash(),
    );
    match svm.send_transaction(transaction) {
        Ok(_) => panic!("{label} unexpectedly succeeded"),
        Err(failure) => failure.meta.logs,
    }
}

fn read_blocked_wallets_count(svm: &LiteSVM) -> u64 {
    let config = svm.get_account(&find_config_pda()).unwrap();
    // Config layout: discriminator(1) | authority(32) | blocked_wallets_count(8).
    u64::from_le_bytes(config.data[33..41].try_into().unwrap())
}

#[test]
fn block_list_transfer_hook_lifecycle() {
    let mut svm = LiteSVM::new();
    svm.add_program(BLOCK_LIST_ID, PROGRAM_SO);

    let payer = Keypair::new();
    let wallet_a = Keypair::new();
    let wallet_b = Keypair::new();
    let mint_keypair = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    svm.airdrop(&wallet_a.pubkey(), 100_000_000).unwrap();

    // init: creates the config PDA owned by the hook program.
    let init_instruction = Init {
        authority: payer.pubkey(),
        config: find_config_pda(),
        system_program: solana_sdk::system_program::id(),
    }
    .instruction();
    send_expecting_success(&mut svm, &[init_instruction], &payer, &[&payer], "init");

    let config = svm.get_account(&find_config_pda()).unwrap();
    assert_eq!(config.data.len(), 41, "config account size");
    assert_eq!(config.data[0], 0x01, "config discriminator");
    assert_eq!(
        Pubkey::try_from(&config.data[1..33]).unwrap(),
        payer.pubkey(),
        "config authority"
    );
    assert_eq!(read_blocked_wallets_count(&svm), 0);

    // Create a Token Extensions mint with the TransferHook extension pointing
    // at the block-list program, then write the (empty) extra-metas account.
    let mint_len =
        ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::TransferHook]).unwrap();
    let mint_rent = svm.minimum_balance_for_rent_exemption(mint_len);
    let create_mint_account_instruction = system_instruction::create_account(
        &payer.pubkey(),
        &mint_keypair.pubkey(),
        mint_rent,
        mint_len as u64,
        &spl_token_2022::id(),
    );
    let init_hook_instruction = spl_token_2022::extension::transfer_hook::instruction::initialize(
        &spl_token_2022::id(),
        &mint_keypair.pubkey(),
        Some(payer.pubkey()),
        Some(BLOCK_LIST_ID),
    )
    .unwrap();
    let init_mint_instruction = initialize_mint2(
        &spl_token_2022::id(),
        &mint_keypair.pubkey(),
        &payer.pubkey(),
        None,
        DECIMALS,
    )
    .unwrap();
    send_expecting_success(
        &mut svm,
        &[
            create_mint_account_instruction,
            init_hook_instruction,
            init_mint_instruction,
        ],
        &payer,
        &[&payer, &mint_keypair],
        "create-mint",
    );

    let setup_extra_metas_instruction = SetupExtraMetas {
        authority: payer.pubkey(),
        config: find_config_pda(),
        mint: mint_keypair.pubkey(),
        extra_metas: find_extra_metas_pda(&mint_keypair.pubkey()),
        system_program: solana_sdk::system_program::id(),
    }
    .instruction(SetupExtraMetasInstructionArgs {
        check_both_wallets: false,
    });
    send_expecting_success(
        &mut svm,
        &[setup_extra_metas_instruction.clone()],
        &payer,
        &[&payer],
        "setup_extra_metas (empty)",
    );
    let extra_metas = svm
        .get_account(&find_extra_metas_pda(&mint_keypair.pubkey()))
        .unwrap();
    // Empty ExtraAccountMetaList = 8 byte TLV header + 4 bytes length + 4 bytes count.
    assert_eq!(extra_metas.data.len(), 16, "empty extra-metas data length");

    // Create both ATAs and mint to wallet A.
    let ata_a = get_associated_token_address_with_program_id(
        &wallet_a.pubkey(),
        &mint_keypair.pubkey(),
        &spl_token_2022::id(),
    );
    let ata_b = get_associated_token_address_with_program_id(
        &wallet_b.pubkey(),
        &mint_keypair.pubkey(),
        &spl_token_2022::id(),
    );
    let create_ata_a = create_associated_token_account(
        &payer.pubkey(),
        &wallet_a.pubkey(),
        &mint_keypair.pubkey(),
        &spl_token_2022::id(),
    );
    let create_ata_b = create_associated_token_account(
        &payer.pubkey(),
        &wallet_b.pubkey(),
        &mint_keypair.pubkey(),
        &spl_token_2022::id(),
    );
    let mint_to_a = mint_to_checked(
        &spl_token_2022::id(),
        &mint_keypair.pubkey(),
        &ata_a,
        &payer.pubkey(),
        &[],
        MINT_AMOUNT,
        DECIMALS,
    )
    .unwrap();
    send_expecting_success(
        &mut svm,
        &[create_ata_a, create_ata_b, mint_to_a],
        &payer,
        &[&payer],
        "create-atas+mint",
    );
    let ata_a_data_len = svm.get_account(&ata_a).unwrap().data.len();
    assert!(
        ata_a_data_len > 165,
        "ATA has extension data (immutable owner)"
    );

    // Transfer succeeds while the source wallet is not blocked.
    let transfer_unblocked = build_transfer_with_hook_accounts(
        &ata_a,
        &mint_keypair.pubkey(),
        &ata_b,
        &wallet_a.pubkey(),
        &wallet_a.pubkey(),
        ExtraMode::Empty,
    );
    send_expecting_success(
        &mut svm,
        &[
            ComputeBudgetInstruction::set_compute_unit_limit(400_000),
            transfer_unblocked,
        ],
        &wallet_a,
        &[&wallet_a],
        "transfer (unblocked)",
    );

    // block_wallet: creates wallet A's wallet_block PDA and bumps the count.
    let block_instruction = BlockWallet {
        authority: payer.pubkey(),
        config: find_config_pda(),
        wallet: wallet_a.pubkey(),
        wallet_block: find_wallet_block_pda(&wallet_a.pubkey()),
        system_program: solana_sdk::system_program::id(),
    }
    .instruction();
    send_expecting_success(
        &mut svm,
        &[block_instruction],
        &payer,
        &[&payer],
        "block_wallet A",
    );
    let wallet_block = svm
        .get_account(&find_wallet_block_pda(&wallet_a.pubkey()))
        .unwrap();
    assert_eq!(wallet_block.data[0], 0x02, "wallet_block discriminator");
    assert_eq!(read_blocked_wallets_count(&svm), 1);

    // With a nonzero blocked count, setup_extra_metas writes the source
    // wallet_block dependency into the metas: 16-byte header + one 35-byte
    // ExtraAccountMeta entry.
    send_expecting_success(
        &mut svm,
        &[setup_extra_metas_instruction],
        &payer,
        &[&payer],
        "setup_extra_metas (source dep)",
    );
    let extra_metas = svm
        .get_account(&find_extra_metas_pda(&mint_keypair.pubkey()))
        .unwrap();
    assert_eq!(
        extra_metas.data.len(),
        51,
        "source-dependency extra-metas data length"
    );

    // Transfer from the blocked source wallet fails with
    // BlockListError::AccountBlocked (variant index 2 -> custom code 0x2).
    let transfer_blocked = build_transfer_with_hook_accounts(
        &ata_a,
        &mint_keypair.pubkey(),
        &ata_b,
        &wallet_a.pubkey(),
        &wallet_a.pubkey(),
        ExtraMode::SourceOnly,
    );
    let logs = send_expecting_failure(
        &mut svm,
        &[
            ComputeBudgetInstruction::set_compute_unit_limit(400_000),
            transfer_blocked,
        ],
        &wallet_a,
        &[&wallet_a],
        "transfer-from-blocked",
    );
    let joined_logs = logs.join("\n");
    assert!(
        joined_logs.contains("custom program error: 0x2"),
        "expected AccountBlocked (custom 0x2) error in logs, got:\n{joined_logs}"
    );

    // unblock_wallet: closes the wallet_block PDA and decrements the count.
    let unblock_instruction = UnblockWallet {
        authority: payer.pubkey(),
        config: find_config_pda(),
        wallet_block: find_wallet_block_pda(&wallet_a.pubkey()),
        system_program: solana_sdk::system_program::id(),
    }
    .instruction();
    send_expecting_success(
        &mut svm,
        &[unblock_instruction],
        &payer,
        &[&payer],
        "unblock_wallet A",
    );
    // After close the runtime reports either no account or a drained shell
    // (zero lamports, empty data) depending on whether the slot advanced.
    let closed_wallet_block = svm.get_account(&find_wallet_block_pda(&wallet_a.pubkey()));
    assert!(
        closed_wallet_block
            .map(|account| account.lamports == 0 && account.data.is_empty())
            .unwrap_or(true),
        "wallet_block PDA closed"
    );
    assert_eq!(read_blocked_wallets_count(&svm), 0);

    // Re-issue the transfer with the (now-closed) wallet_block PDA still in
    // the extra metas. The closed account is empty, so the hook no longer
    // blocks the transfer.
    let transfer_after_unblock = build_transfer_with_hook_accounts(
        &ata_a,
        &mint_keypair.pubkey(),
        &ata_b,
        &wallet_a.pubkey(),
        &wallet_a.pubkey(),
        ExtraMode::SourceOnly,
    );
    send_expecting_success(
        &mut svm,
        &[
            ComputeBudgetInstruction::set_compute_unit_limit(400_000),
            transfer_after_unblock,
        ],
        &wallet_a,
        &[&wallet_a],
        "transfer (after unblock)",
    );
}
