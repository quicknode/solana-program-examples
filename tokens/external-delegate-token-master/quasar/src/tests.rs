extern crate std;
use {
    crate::{
        cpi::{
            AuthorityTransferInstruction, InitializeInstruction, SetEthereumAddressInstruction,
            TransferTokensInstruction,
        },
        ExternalDelegateError, UserAccount, UserPda,
    },
    quasar_test::prelude::*,
    std::vec::Vec,
};

const MINT_DECIMALS: u8 = 6;
const MINT_AMOUNT: u64 = 1_000_000_000;
const TRANSFER_AMOUNT: u64 = 500_000_000;

/// Fixed delegate key so tests are deterministic. Any nonzero scalar below
/// the secp256k1 curve order works.
const DELEGATE_SECP256K1_PRIVATE_KEY: [u8; 32] = [0x42; 32];

// Deterministic addresses avoid Pubkey::new_unique(), whose global counter
// produces different values depending on test binary layout / discovery order.
const AUTHORITY: Pubkey = Pubkey::new_from_array([1; 32]);
const USER_ACCOUNT: Pubkey = Pubkey::new_from_array([2; 32]);
const MALLORY: Pubkey = Pubkey::new_from_array([3; 32]);
const MINT: Pubkey = Pubkey::new_from_array([4; 32]);
const USER_TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([5; 32]);
const RECIPIENT_TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([6; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([7; 32]);
const ATTACKER_TOKEN_ACCOUNT: Pubkey = Pubkey::new_from_array([8; 32]);

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

/// Fund the authority and initialize the user account. The init target enters
/// the transaction as an empty system account automatically; the generated
/// builder marks it as a required signer (fresh keypair account).
fn initialize_user_account(test: &mut Test) {
    test.add(Wallet::new().at(AUTHORITY));
    test.send(InitializeInstruction {
        user_account: USER_ACCOUNT,
        authority: AUTHORITY,
    })
    .succeeds();
}

/// Initializes the user account, links the fixed delegate Ethereum key, and
/// loads the PDA-owned token account plus an empty recipient token account.
fn setup_transfer_world(test: &mut Test) -> Pubkey {
    initialize_user_account(test);
    test.send(SetEthereumAddressInstruction {
        user_account: USER_ACCOUNT,
        authority: AUTHORITY,
        ethereum_address: ethereum_address_of(&delegate_secret_key()),
    })
    .succeeds();

    // Load token state directly: a mint, the PDA-owned funded token account,
    // and the empty recipient token account.
    let user_pda = test.derive_pda(UserPda::seeds(&USER_ACCOUNT));
    test.add(
        Mint::new(AUTHORITY)
            .at(MINT)
            .supply(MINT_AMOUNT)
            .decimals(MINT_DECIMALS),
    );
    test.add(
        TokenAccount::new(MINT, user_pda)
            .at(USER_TOKEN_ACCOUNT)
            .amount(MINT_AMOUNT),
    );
    test.add(TokenAccount::new(MINT, AUTHORITY).at(RECIPIENT_TOKEN_ACCOUNT));
    user_pda
}

/// Transfer instruction for the Ethereum-signature path. The user PDA and
/// token program are canonical derivations, so the generated instruction
/// omits them.
fn transfer_tokens_instruction(
    authority: Pubkey,
    recipient_token_account: Pubkey,
    amount: u64,
    signature: [u8; 65],
) -> TransferTokensInstruction {
    TransferTokensInstruction {
        user_account: USER_ACCOUNT,
        authority,
        mint: MINT,
        user_token_account: USER_TOKEN_ACCOUNT,
        recipient_token_account,
        amount,
        signature,
    }
}

#[quasar_test]
fn initialize_creates_user_account_with_zero_state(test: &mut Test) {
    initialize_user_account(test);

    let state = test.read::<UserAccount>(USER_ACCOUNT);
    assert_eq!(state.authority, AUTHORITY);
    assert_eq!(state.ethereum_address, [0u8; 20]);
    assert_eq!(u64::from(state.nonce), 0);
}

#[quasar_test]
fn set_ethereum_address_stores_the_address(test: &mut Test) {
    initialize_user_account(test);

    let ethereum_address = ethereum_address_of(&delegate_secret_key());
    test.send(SetEthereumAddressInstruction {
        user_account: USER_ACCOUNT,
        authority: AUTHORITY,
        ethereum_address,
    })
    .succeeds();

    assert_eq!(
        test.read::<UserAccount>(USER_ACCOUNT).ethereum_address,
        ethereum_address
    );
}

#[quasar_test]
fn set_ethereum_address_wrong_authority_fails(test: &mut Test) {
    initialize_user_account(test);
    test.add(Wallet::new().at(MALLORY));

    let result = test.send(SetEthereumAddressInstruction {
        user_account: USER_ACCOUNT,
        authority: MALLORY,
        ethereum_address: ethereum_address_of(&delegate_secret_key()),
    });
    assert!(
        result.is_err(),
        "a signer other than user_account.authority must be rejected"
    );
}

#[quasar_test]
fn transfer_tokens_with_valid_signature_moves_tokens_and_increments_nonce(test: &mut Test) {
    setup_transfer_world(test);

    let message = build_transfer_authorization_message(
        &USER_ACCOUNT,
        TRANSFER_AMOUNT,
        &RECIPIENT_TOKEN_ACCOUNT,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    test.send(transfer_tokens_instruction(
        AUTHORITY,
        RECIPIENT_TOKEN_ACCOUNT,
        TRANSFER_AMOUNT,
        signature,
    ))
    .succeeds()
    .has_tokens(RECIPIENT_TOKEN_ACCOUNT, TRANSFER_AMOUNT)
    .has_tokens(USER_TOKEN_ACCOUNT, MINT_AMOUNT - TRANSFER_AMOUNT);

    assert_eq!(u64::from(test.read::<UserAccount>(USER_ACCOUNT).nonce), 1);
}

#[quasar_test]
fn transfer_tokens_replayed_signature_fails(test: &mut Test) {
    setup_transfer_world(test);

    let message = build_transfer_authorization_message(
        &USER_ACCOUNT,
        TRANSFER_AMOUNT,
        &RECIPIENT_TOKEN_ACCOUNT,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let instruction: Instruction = transfer_tokens_instruction(
        AUTHORITY,
        RECIPIENT_TOKEN_ACCOUNT,
        TRANSFER_AMOUNT,
        signature,
    )
    .into();
    test.send(instruction.clone()).succeeds();

    // Replay the identical instruction. The stored nonce is now 1, so the
    // onchain reconstruction differs from the signed message.
    test.send(instruction)
        .fails_with(ExternalDelegateError::InvalidSignature);

    // Exactly one transfer happened.
    assert_eq!(test.tokens(RECIPIENT_TOKEN_ACCOUNT), TRANSFER_AMOUNT);
    assert_eq!(u64::from(test.read::<UserAccount>(USER_ACCOUNT).nonce), 1);
}

#[quasar_test]
fn transfer_tokens_signature_over_different_amount_fails(test: &mut Test) {
    setup_transfer_world(test);

    let authorized_amount = TRANSFER_AMOUNT;
    let attempted_amount = MINT_AMOUNT;
    let message = build_transfer_authorization_message(
        &USER_ACCOUNT,
        authorized_amount,
        &RECIPIENT_TOKEN_ACCOUNT,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    test.send(transfer_tokens_instruction(
        AUTHORITY,
        RECIPIENT_TOKEN_ACCOUNT,
        attempted_amount,
        signature,
    ))
    .fails_with(ExternalDelegateError::InvalidSignature);

    assert_eq!(test.tokens(RECIPIENT_TOKEN_ACCOUNT), 0);
    assert_eq!(u64::from(test.read::<UserAccount>(USER_ACCOUNT).nonce), 0);
}

#[quasar_test]
fn transfer_tokens_signature_over_different_recipient_fails(test: &mut Test) {
    setup_transfer_world(test);

    // Sign for the legitimate recipient, then try to redirect the transfer
    // to an attacker-controlled token account.
    test.add(TokenAccount::new(MINT, ATTACKER).at(ATTACKER_TOKEN_ACCOUNT));

    let message = build_transfer_authorization_message(
        &USER_ACCOUNT,
        TRANSFER_AMOUNT,
        &RECIPIENT_TOKEN_ACCOUNT,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    test.send(transfer_tokens_instruction(
        AUTHORITY,
        ATTACKER_TOKEN_ACCOUNT,
        TRANSFER_AMOUNT,
        signature,
    ))
    .fails_with(ExternalDelegateError::InvalidSignature);

    assert_eq!(test.tokens(ATTACKER_TOKEN_ACCOUNT), 0);
}

#[quasar_test]
fn transfer_tokens_wrong_solana_authority_fails(test: &mut Test) {
    setup_transfer_world(test);
    test.add(Wallet::new().at(MALLORY));

    // A correctly signed Ethereum authorization must not bypass the
    // Solana-side authority check.
    let message = build_transfer_authorization_message(
        &USER_ACCOUNT,
        TRANSFER_AMOUNT,
        &RECIPIENT_TOKEN_ACCOUNT,
        0,
    );
    let signature = sign_transfer_authorization(&delegate_secret_key(), &message);

    let result = test.send(transfer_tokens_instruction(
        MALLORY,
        RECIPIENT_TOKEN_ACCOUNT,
        TRANSFER_AMOUNT,
        signature,
    ));
    assert!(
        result.is_err(),
        "a signer other than user_account.authority must be rejected"
    );
    assert_eq!(test.tokens(RECIPIENT_TOKEN_ACCOUNT), 0);
    assert_eq!(u64::from(test.read::<UserAccount>(USER_ACCOUNT).nonce), 0);
}

#[quasar_test]
fn authority_transfer_moves_tokens(test: &mut Test) {
    setup_transfer_world(test);

    test.send(AuthorityTransferInstruction {
        user_account: USER_ACCOUNT,
        authority: AUTHORITY,
        mint: MINT,
        user_token_account: USER_TOKEN_ACCOUNT,
        recipient_token_account: RECIPIENT_TOKEN_ACCOUNT,
        amount: TRANSFER_AMOUNT,
    })
    .succeeds()
    .has_tokens(RECIPIENT_TOKEN_ACCOUNT, TRANSFER_AMOUNT)
    .has_tokens(USER_TOKEN_ACCOUNT, MINT_AMOUNT - TRANSFER_AMOUNT);
}

#[quasar_test]
fn authority_transfer_wrong_authority_fails(test: &mut Test) {
    setup_transfer_world(test);
    test.add(Wallet::new().at(MALLORY));

    let result = test.send(AuthorityTransferInstruction {
        user_account: USER_ACCOUNT,
        authority: MALLORY,
        mint: MINT,
        user_token_account: USER_TOKEN_ACCOUNT,
        recipient_token_account: RECIPIENT_TOKEN_ACCOUNT,
        amount: TRANSFER_AMOUNT,
    });
    assert!(
        result.is_err(),
        "a signer other than user_account.authority must be rejected"
    );
    assert_eq!(test.tokens(RECIPIENT_TOKEN_ACCOUNT), 0);
}
