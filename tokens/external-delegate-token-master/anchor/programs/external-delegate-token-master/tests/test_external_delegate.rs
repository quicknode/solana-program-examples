use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    borsh::BorshDeserialize,
    litesvm::LiteSVM,
    sha3::{Digest, Keccak256},
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account, send_transaction_from_instructions,
    },
    solana_signer::Signer,
};

const WALLET_LAMPORTS: u64 = 10_000_000_000;
const MINT_DECIMALS: u8 = 6;
const MINT_AMOUNT: u64 = 1_000_000_000;
const TRANSFER_AMOUNT: u64 = 500_000_000;

/// Fixed delegate key so tests are deterministic. Any nonzero scalar below
/// the secp256k1 curve order works.
const DELEGATE_SECP256K1_PRIVATE_KEY: [u8; 32] = [0x42; 32];

fn token_program_id() -> Pubkey {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

/// Mirror of the program's `UserAccount` for reading state in tests
/// (after the 8-byte Anchor discriminator).
#[derive(BorshDeserialize)]
struct UserAccountState {
    authority: [u8; 32],
    ethereum_address: [u8; 20],
    nonce: u64,
}

fn read_user_account(svm: &LiteSVM, address: &Pubkey) -> UserAccountState {
    let account = svm.get_account(address).expect("user account should exist");
    let anchor_discriminator_len = 8;
    UserAccountState::try_from_slice(&account.data[anchor_discriminator_len..]).unwrap()
}

fn delegate_secret_key() -> libsecp256k1::SecretKey {
    libsecp256k1::SecretKey::parse(&DELEGATE_SECP256K1_PRIVATE_KEY).unwrap()
}

/// Ethereum address = last 20 bytes of keccak256 of the 64-byte uncompressed
/// public key (0x04 prefix dropped).
fn ethereum_address_of(secret_key: &libsecp256k1::SecretKey) -> [u8; 20] {
    let public_key = libsecp256k1::PublicKey::from_secret_key(secret_key);
    let uncompressed = public_key.serialize();
    let hash = Keccak256::digest(&uncompressed[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    address
}

/// Builds the exact preimage the program reconstructs onchain:
/// keccak256(program id || user account || amount LE || recipient token account || nonce LE).
fn build_transfer_authorization_message(
    program_id: &Pubkey,
    user_account: &Pubkey,
    amount: u64,
    recipient_token_account: &Pubkey,
    nonce: u64,
) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(program_id.as_ref());
    hasher.update(user_account.as_ref());
    hasher.update(amount.to_le_bytes());
    hasher.update(recipient_token_account.as_ref());
    hasher.update(nonce.to_le_bytes());
    hasher.finalize().into()
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

fn setup() -> (LiteSVM, Pubkey, Keypair) {
    let program_id = external_delegate_token_master::id();
    let mut svm = LiteSVM::new();

    let program_bytes = include_bytes!("../../../target/deploy/external_delegate_token_master.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = create_wallet(&mut svm, WALLET_LAMPORTS).unwrap();
    (svm, program_id, payer)
}

fn initialize_user_account(
    svm: &mut LiteSVM,
    program_id: &Pubkey,
    authority: &Keypair,
    user_account: &Keypair,
) {
    let init_instruction = Instruction::new_with_bytes(
        *program_id,
        &external_delegate_token_master::instruction::Initialize {}.data(),
        external_delegate_token_master::accounts::InitializeAccountConstraints {
            user_account: user_account.pubkey(),
            authority: authority.pubkey(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        svm,
        vec![init_instruction],
        &[authority, user_account],
        &authority.pubkey(),
    )
    .unwrap();
}

fn set_ethereum_address(
    svm: &mut LiteSVM,
    program_id: &Pubkey,
    authority: &Keypair,
    user_account: &Pubkey,
    ethereum_address: [u8; 20],
) {
    let set_address_instruction = Instruction::new_with_bytes(
        *program_id,
        &external_delegate_token_master::instruction::SetEthereumAddress { ethereum_address }
            .data(),
        external_delegate_token_master::accounts::SetEthereumAddressAccountConstraints {
            user_account: *user_account,
            authority: authority.pubkey(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        svm,
        vec![set_address_instruction],
        &[authority],
        &authority.pubkey(),
    )
    .unwrap();
}

/// Everything a transfer_tokens test needs: a user account linked to the
/// fixed delegate Ethereum key, a funded PDA-owned token account, and a
/// recipient token account.
struct TransferFixture {
    svm: LiteSVM,
    program_id: Pubkey,
    authority: Keypair,
    user_account: Pubkey,
    user_pda: Pubkey,
    mint: Pubkey,
    user_pda_token_account: Pubkey,
    recipient_token_account: Pubkey,
}

fn setup_transfer_fixture() -> TransferFixture {
    let (mut svm, program_id, authority) = setup();
    let user_account_keypair = Keypair::new();
    initialize_user_account(&mut svm, &program_id, &authority, &user_account_keypair);

    let user_account = user_account_keypair.pubkey();
    set_ethereum_address(
        &mut svm,
        &program_id,
        &authority,
        &user_account,
        ethereum_address_of(&delegate_secret_key()),
    );

    let (user_pda, _bump) = Pubkey::find_program_address(&[user_account.as_ref()], &program_id);

    let mint = create_token_mint(&mut svm, &authority, MINT_DECIMALS, None).unwrap();
    let user_pda_token_account =
        create_associated_token_account(&mut svm, &user_pda, &mint, &authority).unwrap();
    mint_tokens_to_token_account(&mut svm, &mint, &user_pda_token_account, MINT_AMOUNT, &authority)
        .unwrap();

    let recipient = Keypair::new();
    let recipient_token_account =
        create_associated_token_account(&mut svm, &recipient.pubkey(), &mint, &authority).unwrap();

    TransferFixture {
        svm,
        program_id,
        authority,
        user_account,
        user_pda,
        mint,
        user_pda_token_account,
        recipient_token_account,
    }
}

fn build_transfer_tokens_instruction(
    fixture: &TransferFixture,
    authority: &Pubkey,
    recipient_token_account: &Pubkey,
    amount: u64,
    signature: [u8; 65],
) -> Instruction {
    Instruction::new_with_bytes(
        fixture.program_id,
        &external_delegate_token_master::instruction::TransferTokens { amount, signature }.data(),
        external_delegate_token_master::accounts::TransferTokensAccountConstraints {
            user_account: fixture.user_account,
            authority: *authority,
            mint: fixture.mint,
            user_token_account: fixture.user_pda_token_account,
            recipient_token_account: *recipient_token_account,
            user_pda: fixture.user_pda,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    )
}

#[test]
fn test_initialize_user_account() {
    let (mut svm, program_id, authority) = setup();
    let user_account = Keypair::new();
    initialize_user_account(&mut svm, &program_id, &authority, &user_account);

    let state = read_user_account(&svm, &user_account.pubkey());
    assert_eq!(state.authority, authority.pubkey().to_bytes());
    assert_eq!(state.ethereum_address, [0u8; 20]);
    assert_eq!(state.nonce, 0);
}

#[test]
fn test_set_ethereum_address() {
    let (mut svm, program_id, authority) = setup();
    let user_account = Keypair::new();
    initialize_user_account(&mut svm, &program_id, &authority, &user_account);

    let ethereum_address = ethereum_address_of(&delegate_secret_key());
    set_ethereum_address(
        &mut svm,
        &program_id,
        &authority,
        &user_account.pubkey(),
        ethereum_address,
    );

    let state = read_user_account(&svm, &user_account.pubkey());
    assert_eq!(state.ethereum_address, ethereum_address);
}

#[test]
fn test_transfer_tokens_with_valid_signature_moves_tokens_and_increments_nonce() {
    let mut fixture = setup_transfer_fixture();

    let message = build_transfer_authorization_message(
        &fixture.program_id,
        &fixture.user_account,
        TRANSFER_AMOUNT,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let authority_pubkey = fixture.authority.pubkey();
    let transfer_instruction = build_transfer_tokens_instruction(
        &fixture,
        &authority_pubkey,
        &fixture.recipient_token_account.clone(),
        TRANSFER_AMOUNT,
        signature,
    );
    send_transaction_from_instructions(
        &mut fixture.svm,
        vec![transfer_instruction],
        &[&fixture.authority],
        &authority_pubkey,
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&fixture.svm, &fixture.recipient_token_account).unwrap(),
        TRANSFER_AMOUNT
    );
    assert_eq!(
        get_token_account_balance(&fixture.svm, &fixture.user_pda_token_account).unwrap(),
        MINT_AMOUNT - TRANSFER_AMOUNT
    );
    assert_eq!(read_user_account(&fixture.svm, &fixture.user_account).nonce, 1);
}

#[test]
fn test_transfer_tokens_replayed_signature_fails() {
    let mut fixture = setup_transfer_fixture();

    let message = build_transfer_authorization_message(
        &fixture.program_id,
        &fixture.user_account,
        TRANSFER_AMOUNT,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let authority_pubkey = fixture.authority.pubkey();
    let transfer_instruction = build_transfer_tokens_instruction(
        &fixture,
        &authority_pubkey,
        &fixture.recipient_token_account.clone(),
        TRANSFER_AMOUNT,
        signature,
    );
    send_transaction_from_instructions(
        &mut fixture.svm,
        vec![transfer_instruction.clone()],
        &[&fixture.authority],
        &authority_pubkey,
    )
    .unwrap();

    // Replay the identical instruction. The stored nonce is now 1, so the
    // onchain reconstruction differs from the signed message.
    fixture.svm.expire_blockhash();
    let replay_result = send_transaction_from_instructions(
        &mut fixture.svm,
        vec![transfer_instruction],
        &[&fixture.authority],
        &authority_pubkey,
    );
    assert!(replay_result.is_err(), "replayed signature must be rejected");

    // Exactly one transfer happened.
    assert_eq!(
        get_token_account_balance(&fixture.svm, &fixture.recipient_token_account).unwrap(),
        TRANSFER_AMOUNT
    );
    assert_eq!(read_user_account(&fixture.svm, &fixture.user_account).nonce, 1);
}

#[test]
fn test_transfer_tokens_signature_over_different_amount_fails() {
    let mut fixture = setup_transfer_fixture();

    let authorized_amount = TRANSFER_AMOUNT;
    let attempted_amount = MINT_AMOUNT;
    let message = build_transfer_authorization_message(
        &fixture.program_id,
        &fixture.user_account,
        authorized_amount,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let authority_pubkey = fixture.authority.pubkey();
    let transfer_instruction = build_transfer_tokens_instruction(
        &fixture,
        &authority_pubkey,
        &fixture.recipient_token_account.clone(),
        attempted_amount,
        signature,
    );
    let result = send_transaction_from_instructions(
        &mut fixture.svm,
        vec![transfer_instruction],
        &[&fixture.authority],
        &authority_pubkey,
    );
    assert!(
        result.is_err(),
        "signature over a different amount must be rejected"
    );
    assert_eq!(
        get_token_account_balance(&fixture.svm, &fixture.recipient_token_account).unwrap(),
        0
    );
    assert_eq!(read_user_account(&fixture.svm, &fixture.user_account).nonce, 0);
}

#[test]
fn test_transfer_tokens_signature_over_different_recipient_fails() {
    let mut fixture = setup_transfer_fixture();

    // Sign for the legitimate recipient, then try to redirect the transfer
    // to an attacker-controlled token account.
    let attacker = Keypair::new();
    let attacker_token_account = create_associated_token_account(
        &mut fixture.svm,
        &attacker.pubkey(),
        &fixture.mint,
        &fixture.authority,
    )
    .unwrap();

    let message = build_transfer_authorization_message(
        &fixture.program_id,
        &fixture.user_account,
        TRANSFER_AMOUNT,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let authority_pubkey = fixture.authority.pubkey();
    let transfer_instruction = build_transfer_tokens_instruction(
        &fixture,
        &authority_pubkey,
        &attacker_token_account,
        TRANSFER_AMOUNT,
        signature,
    );
    let result = send_transaction_from_instructions(
        &mut fixture.svm,
        vec![transfer_instruction],
        &[&fixture.authority],
        &authority_pubkey,
    );
    assert!(
        result.is_err(),
        "signature over a different recipient must be rejected"
    );
    assert_eq!(
        get_token_account_balance(&fixture.svm, &attacker_token_account).unwrap(),
        0
    );
}

#[test]
fn test_transfer_tokens_wrong_solana_authority_fails() {
    let mut fixture = setup_transfer_fixture();

    // A correctly signed Ethereum authorization must not bypass the
    // Solana-side authority check.
    let message = build_transfer_authorization_message(
        &fixture.program_id,
        &fixture.user_account,
        TRANSFER_AMOUNT,
        &fixture.recipient_token_account,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let mallory = create_wallet(&mut fixture.svm, WALLET_LAMPORTS).unwrap();
    let mallory_pubkey = mallory.pubkey();
    let transfer_instruction = build_transfer_tokens_instruction(
        &fixture,
        &mallory_pubkey,
        &fixture.recipient_token_account.clone(),
        TRANSFER_AMOUNT,
        signature,
    );
    let result = send_transaction_from_instructions(
        &mut fixture.svm,
        vec![transfer_instruction],
        &[&mallory],
        &mallory_pubkey,
    );
    assert!(
        result.is_err(),
        "a signer other than user_account.authority must be rejected"
    );
    assert_eq!(
        get_token_account_balance(&fixture.svm, &fixture.recipient_token_account).unwrap(),
        0
    );
}

#[test]
fn test_authority_transfer() {
    let fixture = setup_transfer_fixture();
    let mut svm = fixture.svm;

    let authority_transfer_instruction = Instruction::new_with_bytes(
        fixture.program_id,
        &external_delegate_token_master::instruction::AuthorityTransfer {
            amount: TRANSFER_AMOUNT,
        }
        .data(),
        external_delegate_token_master::accounts::AuthorityTransferAccountConstraints {
            user_account: fixture.user_account,
            authority: fixture.authority.pubkey(),
            mint: fixture.mint,
            user_token_account: fixture.user_pda_token_account,
            recipient_token_account: fixture.recipient_token_account,
            user_pda: fixture.user_pda,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut svm,
        vec![authority_transfer_instruction],
        &[&fixture.authority],
        &fixture.authority.pubkey(),
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&svm, &fixture.recipient_token_account).unwrap(),
        TRANSFER_AMOUNT
    );
    assert_eq!(
        get_token_account_balance(&svm, &fixture.user_pda_token_account).unwrap(),
        MINT_AMOUNT - TRANSFER_AMOUNT
    );
}
