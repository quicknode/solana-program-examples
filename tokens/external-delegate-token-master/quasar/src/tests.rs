extern crate std;
use {
    crate::ExternalDelegateError,
    quasar_svm::{Account, Instruction, ProgramError, Pubkey, QuasarSvm},
    solana_program_pack::Pack,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::{println, vec, vec::Vec},
};

const SIGNER_LAMPORTS: u64 = 5_000_000_000;
const MINT_DECIMALS: u8 = 6;
const MINT_AMOUNT: u64 = 1_000_000_000;
const TRANSFER_AMOUNT: u64 = 500_000_000;

/// Fixed delegate key so tests are deterministic. Any nonzero scalar below
/// the secp256k1 curve order works.
const DELEGATE_SECP256K1_PRIVATE_KEY: [u8; 32] = [0x42; 32];

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_external_delegate_token_master.so").unwrap();
    QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program()
}

fn signer(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, SIGNER_LAMPORTS)
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

fn mint(address: Pubkey, authority: Pubkey) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &Mint {
            mint_authority: Some(authority).into(),
            supply: MINT_AMOUNT,
            decimals: MINT_DECIMALS,
            is_initialized: true,
            freeze_authority: None.into(),
        },
    )
}

fn token(address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
    quasar_svm::token::create_keyed_token_account(
        &address,
        &TokenAccount {
            mint,
            owner,
            amount,
            state: AccountState::Initialized,
            ..TokenAccount::default()
        },
    )
}

fn token_balance(svm: &QuasarSvm, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    TokenAccount::unpack(&account.data).unwrap().amount
}

/// Deserialized UserAccount state, parsed from the zero-copy layout:
/// [disc:1] [authority:32] [ethereum_address:20] [nonce:8 LE]
struct UserAccountState {
    authority: Pubkey,
    ethereum_address: [u8; 20],
    nonce: u64,
}

fn parse_user_account(data: &[u8]) -> UserAccountState {
    assert_eq!(data[0], 1, "UserAccount discriminator");
    let mut offset = 1usize;
    let mut take = |len: usize| {
        let bytes = &data[offset..offset + len];
        offset += len;
        bytes
    };
    UserAccountState {
        authority: Pubkey::new_from_array(take(32).try_into().unwrap()),
        ethereum_address: take(20).try_into().unwrap(),
        nonce: u64::from_le_bytes(take(8).try_into().unwrap()),
    }
}

fn read_user_account(svm: &QuasarSvm, address: &Pubkey) -> UserAccountState {
    parse_user_account(&svm.get_account(address).unwrap().data)
}

fn delegate_secret_key() -> libsecp256k1::SecretKey {
    libsecp256k1::SecretKey::parse(&DELEGATE_SECP256K1_PRIVATE_KEY).unwrap()
}

/// Ethereum address = last 20 bytes of keccak256 of the 64-byte uncompressed
/// public key (0x04 prefix dropped).
fn ethereum_address_of(secret_key: &libsecp256k1::SecretKey) -> [u8; 20] {
    let public_key = libsecp256k1::PublicKey::from_secret_key(secret_key);
    let uncompressed = public_key.serialize();
    let hash = solana_keccak_hasher::hash(&uncompressed[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash.as_ref()[12..]);
    address
}

/// Builds the exact preimage the program reconstructs onchain:
/// keccak256(program id || user account || amount LE || recipient token account || nonce LE).
fn build_transfer_authorization_message(
    user_account: &Pubkey,
    amount: u64,
    recipient_token_account: &Pubkey,
    nonce: u64,
) -> [u8; 32] {
    let mut preimage = Vec::new();
    preimage.extend_from_slice(crate::ID.as_ref());
    preimage.extend_from_slice(user_account.as_ref());
    preimage.extend_from_slice(&amount.to_le_bytes());
    preimage.extend_from_slice(recipient_token_account.as_ref());
    preimage.extend_from_slice(&nonce.to_le_bytes());
    let hash = solana_keccak_hasher::hash(&preimage);
    let mut message = [0u8; 32];
    message.copy_from_slice(hash.as_ref());
    message
}

/// 65-byte recoverable signature: r || s || recovery id.
fn sign_transfer_authorization(
    secret_key: &libsecp256k1::SecretKey,
    message: &[u8; 32],
) -> [u8; 65] {
    let (signature, recovery_id) =
        libsecp256k1::sign(&libsecp256k1::Message::parse(message), secret_key);
    let mut bytes = [0u8; 65];
    bytes[..64].copy_from_slice(&signature.serialize());
    bytes[64] = recovery_id.serialize();
    bytes
}

/// Build initialize instruction data.
/// Wire format: [disc=0]
fn build_initialize_data() -> Vec<u8> {
    vec![0u8]
}

/// Build set_ethereum_address instruction data.
/// Wire format: [disc=1] [ethereum_address: 20 bytes]
fn build_set_ethereum_address_data(ethereum_address: [u8; 20]) -> Vec<u8> {
    let mut data = vec![1u8];
    data.extend_from_slice(&ethereum_address);
    data
}

/// Build transfer_tokens instruction data.
/// Wire format: [disc=2] [amount: u64 LE] [signature: 65 bytes]
fn build_transfer_tokens_data(amount: u64, signature: [u8; 65]) -> Vec<u8> {
    let mut data = vec![2u8];
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&signature);
    data
}

/// Build authority_transfer instruction data.
/// Wire format: [disc=3] [amount: u64 LE]
fn build_authority_transfer_data(amount: u64) -> Vec<u8> {
    let mut data = vec![3u8];
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

fn initialize_instruction(user_account: Pubkey, authority: Pubkey) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(user_account.into(), true),
            solana_instruction::AccountMeta::new(authority.into(), true),
            solana_instruction::AccountMeta::new_readonly(
                quasar_svm::system_program::ID.into(),
                false,
            ),
        ],
        data: build_initialize_data(),
    }
}

fn set_ethereum_address_instruction(
    user_account: Pubkey,
    authority: Pubkey,
    ethereum_address: [u8; 20],
) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(user_account.into(), false),
            solana_instruction::AccountMeta::new_readonly(authority.into(), true),
        ],
        data: build_set_ethereum_address_data(ethereum_address),
    }
}

/// Addresses shared by every transfer test.
struct Fixture {
    authority: Pubkey,
    user_account: Pubkey,
    user_pda: Pubkey,
    mint: Pubkey,
    user_token_account: Pubkey,
    recipient_token_account: Pubkey,
}

fn fixture() -> Fixture {
    let user_account = Pubkey::new_unique();
    let (user_pda, _bump) =
        Pubkey::find_program_address(&[user_account.as_ref()], &crate::ID);
    Fixture {
        authority: Pubkey::new_unique(),
        user_account,
        user_pda,
        mint: Pubkey::new_unique(),
        user_token_account: Pubkey::new_unique(),
        recipient_token_account: Pubkey::new_unique(),
    }
}

fn transfer_tokens_instruction(
    fixture: &Fixture,
    authority: Pubkey,
    recipient_token_account: Pubkey,
    amount: u64,
    signature: [u8; 65],
) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(fixture.user_account.into(), false),
            solana_instruction::AccountMeta::new_readonly(authority.into(), true),
            solana_instruction::AccountMeta::new_readonly(fixture.mint.into(), false),
            solana_instruction::AccountMeta::new(fixture.user_token_account.into(), false),
            solana_instruction::AccountMeta::new(recipient_token_account.into(), false),
            solana_instruction::AccountMeta::new_readonly(fixture.user_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(
                quasar_svm::SPL_TOKEN_PROGRAM_ID.into(),
                false,
            ),
        ],
        data: build_transfer_tokens_data(amount, signature),
    }
}

/// Initializes the user account, links the fixed delegate Ethereum key, and
/// loads the PDA-owned token account plus an empty recipient token account.
fn setup_transfer_fixture() -> (QuasarSvm, Fixture) {
    let mut svm = setup();
    let fixture = fixture();

    svm.process_instruction(
        &initialize_instruction(fixture.user_account, fixture.authority),
        &[empty(fixture.user_account), signer(fixture.authority)],
    )
    .assert_success();

    svm.process_instruction(
        &set_ethereum_address_instruction(
            fixture.user_account,
            fixture.authority,
            ethereum_address_of(&delegate_secret_key()),
        ),
        &[],
    )
    .assert_success();

    // Load token state directly: a mint, the PDA-owned funded token account,
    // the empty recipient token account, and the (data-less) PDA itself.
    svm.set_account(mint(fixture.mint, fixture.authority));
    svm.set_account(token(
        fixture.user_token_account,
        fixture.mint,
        fixture.user_pda,
        MINT_AMOUNT,
    ));
    svm.set_account(token(
        fixture.recipient_token_account,
        fixture.mint,
        fixture.authority,
        0,
    ));
    svm.set_account(empty(fixture.user_pda));

    (svm, fixture)
}

fn external_delegate_error(error: ExternalDelegateError) -> ProgramError {
    ProgramError::Custom(error as u32)
}

#[test]
fn test_initialize() {
    let mut svm = setup();

    let authority = Pubkey::new_unique();
    let user_account = Pubkey::new_unique();

    let result = svm.process_instruction(
        &initialize_instruction(user_account, authority),
        &[empty(user_account), signer(authority)],
    );
    result.assert_success();
    println!("  INITIALIZE CU: {}", result.compute_units_consumed);

    let state = read_user_account(&svm, &user_account);
    assert_eq!(state.authority, authority);
    assert_eq!(state.ethereum_address, [0u8; 20]);
    assert_eq!(state.nonce, 0);
}

#[test]
fn test_set_ethereum_address() {
    let mut svm = setup();

    let authority = Pubkey::new_unique();
    let user_account = Pubkey::new_unique();

    svm.process_instruction(
        &initialize_instruction(user_account, authority),
        &[empty(user_account), signer(authority)],
    )
    .assert_success();

    let ethereum_address = ethereum_address_of(&delegate_secret_key());
    let result = svm.process_instruction(
        &set_ethereum_address_instruction(user_account, authority, ethereum_address),
        &[],
    );
    result.assert_success();
    println!("  SET_ETHEREUM_ADDRESS CU: {}", result.compute_units_consumed);

    assert_eq!(
        read_user_account(&svm, &user_account).ethereum_address,
        ethereum_address
    );
}

#[test]
fn test_set_ethereum_address_wrong_authority_fails() {
    let mut svm = setup();

    let authority = Pubkey::new_unique();
    let mallory = Pubkey::new_unique();
    let user_account = Pubkey::new_unique();

    svm.process_instruction(
        &initialize_instruction(user_account, authority),
        &[empty(user_account), signer(authority)],
    )
    .assert_success();

    let result = svm.process_instruction(
        &set_ethereum_address_instruction(
            user_account,
            mallory,
            ethereum_address_of(&delegate_secret_key()),
        ),
        &[signer(mallory)],
    );
    assert!(
        !result.is_ok(),
        "a signer other than user_account.authority must be rejected"
    );
}

#[test]
fn test_transfer_tokens_with_valid_signature_moves_tokens_and_increments_nonce() {
    let (mut svm, fixture) = setup_transfer_fixture();

    let message = build_transfer_authorization_message(
        &fixture.user_account,
        TRANSFER_AMOUNT,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let result = svm.process_instruction(
        &transfer_tokens_instruction(
            &fixture,
            fixture.authority,
            fixture.recipient_token_account,
            TRANSFER_AMOUNT,
            signature,
        ),
        &[],
    );
    result.assert_success();
    println!("  TRANSFER_TOKENS CU: {}", result.compute_units_consumed);

    assert_eq!(
        token_balance(&svm, &fixture.recipient_token_account),
        TRANSFER_AMOUNT
    );
    assert_eq!(
        token_balance(&svm, &fixture.user_token_account),
        MINT_AMOUNT - TRANSFER_AMOUNT
    );
    assert_eq!(read_user_account(&svm, &fixture.user_account).nonce, 1);
}

#[test]
fn test_transfer_tokens_replayed_signature_fails() {
    let (mut svm, fixture) = setup_transfer_fixture();

    let message = build_transfer_authorization_message(
        &fixture.user_account,
        TRANSFER_AMOUNT,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let instruction = transfer_tokens_instruction(
        &fixture,
        fixture.authority,
        fixture.recipient_token_account,
        TRANSFER_AMOUNT,
        signature,
    );
    svm.process_instruction(&instruction, &[]).assert_success();

    // Replay the identical instruction. The stored nonce is now 1, so the
    // onchain reconstruction differs from the signed message.
    let replay_result = svm.process_instruction(&instruction, &[]);
    replay_result.assert_error(external_delegate_error(
        ExternalDelegateError::InvalidSignature,
    ));

    // Exactly one transfer happened.
    assert_eq!(
        token_balance(&svm, &fixture.recipient_token_account),
        TRANSFER_AMOUNT
    );
    assert_eq!(read_user_account(&svm, &fixture.user_account).nonce, 1);
}

#[test]
fn test_transfer_tokens_signature_over_different_amount_fails() {
    let (mut svm, fixture) = setup_transfer_fixture();

    let authorized_amount = TRANSFER_AMOUNT;
    let attempted_amount = MINT_AMOUNT;
    let message = build_transfer_authorization_message(
        &fixture.user_account,
        authorized_amount,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let result = svm.process_instruction(
        &transfer_tokens_instruction(
            &fixture,
            fixture.authority,
            fixture.recipient_token_account,
            attempted_amount,
            signature,
        ),
        &[],
    );
    result.assert_error(external_delegate_error(
        ExternalDelegateError::InvalidSignature,
    ));
    assert_eq!(token_balance(&svm, &fixture.recipient_token_account), 0);
    assert_eq!(read_user_account(&svm, &fixture.user_account).nonce, 0);
}

#[test]
fn test_transfer_tokens_signature_over_different_recipient_fails() {
    let (mut svm, fixture) = setup_transfer_fixture();

    // Sign for the legitimate recipient, then try to redirect the transfer
    // to an attacker-controlled token account.
    let attacker = Pubkey::new_unique();
    let attacker_token_account = Pubkey::new_unique();
    svm.set_account(token(attacker_token_account, fixture.mint, attacker, 0));

    let message = build_transfer_authorization_message(
        &fixture.user_account,
        TRANSFER_AMOUNT,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let result = svm.process_instruction(
        &transfer_tokens_instruction(
            &fixture,
            fixture.authority,
            attacker_token_account,
            TRANSFER_AMOUNT,
            signature,
        ),
        &[],
    );
    result.assert_error(external_delegate_error(
        ExternalDelegateError::InvalidSignature,
    ));
    assert_eq!(token_balance(&svm, &attacker_token_account), 0);
}

#[test]
fn test_transfer_tokens_wrong_solana_authority_fails() {
    let (mut svm, fixture) = setup_transfer_fixture();

    // A correctly signed Ethereum authorization must not bypass the
    // Solana-side authority check.
    let mallory = Pubkey::new_unique();
    let message = build_transfer_authorization_message(
        &fixture.user_account,
        TRANSFER_AMOUNT,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let result = svm.process_instruction(
        &transfer_tokens_instruction(
            &fixture,
            mallory,
            fixture.recipient_token_account,
            TRANSFER_AMOUNT,
            signature,
        ),
        &[signer(mallory)],
    );
    assert!(
        !result.is_ok(),
        "a signer other than user_account.authority must be rejected"
    );
    assert_eq!(token_balance(&svm, &fixture.recipient_token_account), 0);
    assert_eq!(read_user_account(&svm, &fixture.user_account).nonce, 0);
}

#[test]
fn test_authority_transfer() {
    let (mut svm, fixture) = setup_transfer_fixture();

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new_readonly(fixture.user_account.into(), false),
            solana_instruction::AccountMeta::new_readonly(fixture.authority.into(), true),
            solana_instruction::AccountMeta::new_readonly(fixture.mint.into(), false),
            solana_instruction::AccountMeta::new(fixture.user_token_account.into(), false),
            solana_instruction::AccountMeta::new(fixture.recipient_token_account.into(), false),
            solana_instruction::AccountMeta::new_readonly(fixture.user_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(
                quasar_svm::SPL_TOKEN_PROGRAM_ID.into(),
                false,
            ),
        ],
        data: build_authority_transfer_data(TRANSFER_AMOUNT),
    };
    let result = svm.process_instruction(&instruction, &[]);
    result.assert_success();
    println!("  AUTHORITY_TRANSFER CU: {}", result.compute_units_consumed);

    assert_eq!(
        token_balance(&svm, &fixture.recipient_token_account),
        TRANSFER_AMOUNT
    );
    assert_eq!(
        token_balance(&svm, &fixture.user_token_account),
        MINT_AMOUNT - TRANSFER_AMOUNT
    );
}

#[test]
fn test_authority_transfer_wrong_authority_fails() {
    let (mut svm, fixture) = setup_transfer_fixture();

    let mallory = Pubkey::new_unique();
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new_readonly(fixture.user_account.into(), false),
            solana_instruction::AccountMeta::new_readonly(mallory.into(), true),
            solana_instruction::AccountMeta::new_readonly(fixture.mint.into(), false),
            solana_instruction::AccountMeta::new(fixture.user_token_account.into(), false),
            solana_instruction::AccountMeta::new(fixture.recipient_token_account.into(), false),
            solana_instruction::AccountMeta::new_readonly(fixture.user_pda.into(), false),
            solana_instruction::AccountMeta::new_readonly(
                quasar_svm::SPL_TOKEN_PROGRAM_ID.into(),
                false,
            ),
        ],
        data: build_authority_transfer_data(TRANSFER_AMOUNT),
    };
    let result = svm.process_instruction(&instruction, &[signer(mallory)]);
    assert!(
        !result.is_ok(),
        "a signer other than user_account.authority must be rejected"
    );
    assert_eq!(token_balance(&svm, &fixture.recipient_token_account), 0);
}
