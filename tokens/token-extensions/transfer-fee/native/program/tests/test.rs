use {
    litesvm::LiteSVM,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::{Keypair, Signer},
    solana_native_token::LAMPORTS_PER_SOL,
    solana_pubkey::{pubkey, Pubkey},
    solana_transaction::Transaction,
    spl_token_2022_interface::{
        extension::{
            transfer_fee::TransferFeeConfig, BaseStateWithExtensions, StateWithExtensions,
        },
        state::Mint,
    },
    token_2022_transfer_fees_program::CreateTokenArgs,
};

const RENT_SYSVAR_ID: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");

#[test]
fn test_create_token_with_transfer_fee() {
    let mut svm = LiteSVM::new();

    let program_id = Pubkey::new_unique();
    // The .so is built into the workspace target/deploy by
    // `cargo build-sbf --manifest-path=./program/Cargo.toml` (run from the
    // project root). Rebuild after every program change: the binary is
    // embedded at test-compile time, so a stale .so silently tests old code.
    let program_bytes = include_bytes!("../../../../../../target/deploy/token_2022_transfer_fees_program.so");
    svm.add_program(program_id, program_bytes).unwrap();

    // litesvm bundles the Token Extensions program by default.
    let token_program_id = spl_token_2022_interface::id();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    let mint = Keypair::new();

    let decimals = 9u8;
    let data = borsh::to_vec(&CreateTokenArgs {
        token_decimals: decimals,
    })
    .unwrap();

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), true),   // mint account
            AccountMeta::new(payer.pubkey(), false), // mint authority
            AccountMeta::new(payer.pubkey(), true),  // payer
            AccountMeta::new_readonly(RENT_SYSVAR_ID, false),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
            AccountMeta::new_readonly(token_program_id, false),
        ],
        data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &mint],
        svm.latest_blockhash(),
    );

    svm.send_transaction(tx).unwrap();

    // The mint should be owned by Token Extensions and carry the TransferFeeConfig
    // extension. The program initializes a 1% fee, then sets the newer fee to
    // 10% (1000 bps) with a max fee of 5 tokens.
    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(mint_account.owner, token_program_id);

    let state = StateWithExtensions::<Mint>::unpack(&mint_account.data).unwrap();
    assert_eq!(state.base.decimals, decimals);
    assert!(state.base.is_initialized);

    let config = state.get_extension::<TransferFeeConfig>().unwrap();
    let fee_authority: Option<Pubkey> = config.transfer_fee_config_authority.into();
    let withdraw_authority: Option<Pubkey> = config.withdraw_withheld_authority.into();
    assert_eq!(fee_authority, Some(payer.pubkey()));
    assert_eq!(withdraw_authority, Some(payer.pubkey()));

    let max_fee = 5 * 10u64.pow(decimals as u32);
    assert_eq!(
        u16::from(config.newer_transfer_fee.transfer_fee_basis_points),
        1000
    );
    assert_eq!(u64::from(config.newer_transfer_fee.maximum_fee), max_fee);
}
