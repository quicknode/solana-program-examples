use {
    litesvm::LiteSVM,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::{Keypair, Signer},
    solana_native_token::LAMPORTS_PER_SOL,
    solana_pubkey::{pubkey, Pubkey},
    solana_transaction::Transaction,
    spl_token_2022_interface::{
        extension::{
            mint_close_authority::MintCloseAuthority, BaseStateWithExtensions, StateWithExtensions,
        },
        state::Mint,
    },
    token_2022_mint_close_authority_program::CreateTokenArgs,
};

const RENT_SYSVAR_ID: Pubkey = pubkey!("SysvarRent111111111111111111111111111111111");

#[test]
fn test_create_token_with_mint_close_authority() {
    let mut svm = LiteSVM::new();

    let program_id = Pubkey::new_unique();
    let program_bytes =
        include_bytes!("../../tests/fixtures/token_2022_mint_close_authority_program.so");
    svm.add_program(program_id, program_bytes).unwrap();

    // litesvm bundles the SPL Token-2022 program by default.
    let token_program_id = spl_token_2022_interface::id();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    let mint = Keypair::new();

    let data = borsh::to_vec(&CreateTokenArgs { token_decimals: 9 }).unwrap();

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), true),   // mint account
            AccountMeta::new(payer.pubkey(), false), // mint authority
            AccountMeta::new(payer.pubkey(), false), // close authority
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

    // The mint should be owned by Token-2022 and carry the MintCloseAuthority
    // extension pointing at the payer.
    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(mint_account.owner, token_program_id);

    let state = StateWithExtensions::<Mint>::unpack(&mint_account.data).unwrap();
    assert_eq!(state.base.decimals, 9);
    assert!(state.base.is_initialized);

    let close_authority = state.get_extension::<MintCloseAuthority>().unwrap();
    let close_authority_key: Option<Pubkey> = close_authority.close_authority.into();
    assert_eq!(close_authority_key, Some(payer.pubkey()));
}
