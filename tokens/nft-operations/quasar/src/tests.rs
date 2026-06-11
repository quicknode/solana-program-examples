//! QuasarSVM integration tests, ported from the Anchor twin's LiteSVM suite.
//!
//! The SVM loads this program, the SPL Token program, and the Metaplex Token
//! Metadata fixture shared with the Anchor twin
//! (`../anchor/tests/fixtures/mpl_token_metadata.so`), then exercises the
//! full collection lifecycle: create_collection, mint_nft, verify_collection.

extern crate std;
use {
    quasar_svm::{Account, AccountMeta, Instruction, Pubkey, QuasarSvm},
    solana_program_pack::Pack,
    spl_token_interface::state::Account as TokenAccount,
    std::{vec, vec::Vec},
};

const METADATA_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

/// Comfortably above rent exemption for every account size used here.
const FUNDING_LAMPORTS: u64 = 10_000_000_000;

const CREATE_COLLECTION_DISCRIMINATOR: u8 = 0;
const MINT_NFT_DISCRIMINATOR: u8 = 1;
const VERIFY_COLLECTION_DISCRIMINATOR: u8 = 2;

fn program_id() -> Pubkey {
    Pubkey::from(crate::ID)
}

fn setup() -> QuasarSvm {
    let program_elf = std::fs::read("target/deploy/quasar_nft_operations.so").unwrap();
    // The fixture binary is shared with the Anchor twin's LiteSVM suite.
    let metadata_elf = std::fs::read("../anchor/tests/fixtures/mpl_token_metadata.so").unwrap();
    QuasarSvm::new()
        .with_program(&program_id(), &program_elf)
        .with_program(&METADATA_PROGRAM_ID, &metadata_elf)
        .with_token_program()
}

fn signer(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, FUNDING_LAMPORTS)
}

/// A not-yet-created account: empty and system-owned.
fn empty(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: quasar_svm::system_program::ID,
        executable: false,
    }
}

fn derive_mint_authority() -> Pubkey {
    let (mint_authority, _) = Pubkey::find_program_address(&[b"authority"], &program_id());
    mint_authority
}

fn derive_metadata_pda(mint: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[b"metadata", METADATA_PROGRAM_ID.as_ref(), mint.as_ref()],
        &METADATA_PROGRAM_ID,
    );
    pda
}

fn derive_edition_pda(mint: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[
            b"metadata",
            METADATA_PROGRAM_ID.as_ref(),
            mint.as_ref(),
            b"edition",
        ],
        &METADATA_PROGRAM_ID,
    );
    pda
}

/// Instruction data for create_collection / mint_nft. Quasar's compact
/// argument encoding packs the dynamic `String<N>` arguments as a header of
/// per-field length prefixes (u8 each) followed by the packed string bytes.
fn metadata_instruction_data(discriminator: u8, name: &str, symbol: &str, uri: &str) -> Vec<u8> {
    let mut data = vec![
        discriminator,
        name.len() as u8,
        symbol.len() as u8,
        uri.len() as u8,
    ];
    data.extend_from_slice(name.as_bytes());
    data.extend_from_slice(symbol.as_bytes());
    data.extend_from_slice(uri.as_bytes());
    data
}

/// Returns true if `haystack` contains `needle` anywhere. Used to check that
/// caller-supplied metadata strings landed in the Metaplex metadata account
/// without fully deserializing the Metaplex layout.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn token_amount(account: &Account) -> u64 {
    TokenAccount::unpack(&account.data).unwrap().amount
}

/// Addresses for one NFT (or collection NFT): mint, its Metaplex PDAs, and
/// the holding token account.
struct NftAccounts {
    mint: Pubkey,
    metadata: Pubkey,
    master_edition: Pubkey,
    destination: Pubkey,
}

impl NftAccounts {
    fn new() -> Self {
        let mint = Pubkey::new_unique();
        Self {
            mint,
            metadata: derive_metadata_pda(&mint),
            master_edition: derive_edition_pda(&mint),
            destination: Pubkey::new_unique(),
        }
    }
}

const COLLECTION_NAME: &str = "Quasar Collection";
const COLLECTION_SYMBOL: &str = "QCOL";
const COLLECTION_URI: &str = "https://example.com/collection.json";
const NFT_NAME: &str = "Quasar NFT #1";
const NFT_SYMBOL: &str = "QNFT";
const NFT_URI: &str = "https://example.com/nft-1.json";

fn build_create_collection_instruction(payer: Pubkey, collection: &NftAccounts) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(payer, true),
            // The mint and destination accounts are created by the
            // instruction, so they sign (fresh keypair accounts).
            AccountMeta::new(collection.mint, true),
            AccountMeta::new_readonly(derive_mint_authority(), false),
            AccountMeta::new(collection.metadata, false),
            AccountMeta::new(collection.master_edition, false),
            AccountMeta::new(collection.destination, true),
            AccountMeta::new_readonly(quasar_svm::system_program::ID, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(METADATA_PROGRAM_ID, false),
            AccountMeta::new_readonly(quasar_svm::solana_sdk_ids::sysvar::rent::ID, false),
        ],
        data: metadata_instruction_data(
            CREATE_COLLECTION_DISCRIMINATOR,
            COLLECTION_NAME,
            COLLECTION_SYMBOL,
            COLLECTION_URI,
        ),
    }
}

fn build_mint_nft_instruction(
    payer: Pubkey,
    nft: &NftAccounts,
    collection_mint: Pubkey,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(nft.mint, true),
            AccountMeta::new(nft.destination, true),
            AccountMeta::new(nft.metadata, false),
            AccountMeta::new(nft.master_edition, false),
            AccountMeta::new_readonly(derive_mint_authority(), false),
            AccountMeta::new(collection_mint, false),
            AccountMeta::new_readonly(quasar_svm::system_program::ID, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(METADATA_PROGRAM_ID, false),
            AccountMeta::new_readonly(quasar_svm::solana_sdk_ids::sysvar::rent::ID, false),
        ],
        data: metadata_instruction_data(MINT_NFT_DISCRIMINATOR, NFT_NAME, NFT_SYMBOL, NFT_URI),
    }
}

fn build_verify_collection_instruction(
    payer: Pubkey,
    nft: &NftAccounts,
    collection: &NftAccounts,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            // The Metaplex verify CPI takes the payer as writable signer.
            AccountMeta::new(payer, true),
            AccountMeta::new(nft.metadata, false),
            AccountMeta::new_readonly(derive_mint_authority(), false),
            AccountMeta::new_readonly(collection.mint, false),
            AccountMeta::new(collection.metadata, false),
            AccountMeta::new_readonly(collection.master_edition, false),
            AccountMeta::new_readonly(METADATA_PROGRAM_ID, false),
        ],
        data: vec![VERIFY_COLLECTION_DISCRIMINATOR],
    }
}

/// New (not-yet-created) accounts an NFT mint touches.
fn new_nft_accounts(nft: &NftAccounts) -> [Account; 4] {
    [
        empty(nft.mint),
        empty(nft.metadata),
        empty(nft.master_edition),
        empty(nft.destination),
    ]
}

#[test]
fn test_create_collection() {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let collection = NftAccounts::new();

    let mut accounts = vec![signer(payer), empty(derive_mint_authority())];
    accounts.extend(new_nft_accounts(&collection));

    let result = svm.process_instruction(
        &build_create_collection_instruction(payer, &collection),
        &accounts,
    );
    result.assert_success();

    // The collection mint exists and 1 token was minted to the destination.
    let mint_account = result.account(&collection.mint).unwrap();
    assert!(!mint_account.data.is_empty());
    assert_eq!(
        token_amount(&result.account(&collection.destination).unwrap()),
        1,
        "Should hold 1 collection token"
    );

    // The metadata account carries the caller-supplied name, and the master
    // edition exists.
    let metadata_account = result.account(&collection.metadata).unwrap();
    assert!(
        contains_bytes(&metadata_account.data, COLLECTION_NAME.as_bytes()),
        "Metadata should contain the caller-supplied collection name"
    );
    assert!(!result.account(&collection.master_edition).unwrap().data.is_empty());
}

#[test]
fn test_mint_nft_to_collection() {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let collection = NftAccounts::new();

    let mut create_accounts = vec![signer(payer), empty(derive_mint_authority())];
    create_accounts.extend(new_nft_accounts(&collection));
    svm.process_instruction(
        &build_create_collection_instruction(payer, &collection),
        &create_accounts,
    )
    .assert_success();

    // Mint an NFT into the collection. Only the NFT's own accounts are new;
    // the payer, authority PDA, and collection mint persist in the SVM.
    let nft = NftAccounts::new();
    let result = svm.process_instruction(
        &build_mint_nft_instruction(payer, &nft, collection.mint),
        &new_nft_accounts(&nft),
    );
    result.assert_success();

    assert_eq!(
        token_amount(&result.account(&nft.destination).unwrap()),
        1,
        "Should hold 1 NFT"
    );
    let nft_metadata_account = result.account(&nft.metadata).unwrap();
    assert!(
        contains_bytes(&nft_metadata_account.data, NFT_NAME.as_bytes()),
        "Metadata should contain the caller-supplied NFT name"
    );
    // The metadata carries the (unverified) collection reference.
    assert!(
        contains_bytes(&nft_metadata_account.data, collection.mint.as_ref()),
        "Metadata should reference the collection mint"
    );
}

#[test]
fn test_verify_collection() {
    let mut svm = setup();
    let payer = Pubkey::new_unique();
    let collection = NftAccounts::new();

    let mut create_accounts = vec![signer(payer), empty(derive_mint_authority())];
    create_accounts.extend(new_nft_accounts(&collection));
    svm.process_instruction(
        &build_create_collection_instruction(payer, &collection),
        &create_accounts,
    )
    .assert_success();

    let nft = NftAccounts::new();
    svm.process_instruction(
        &build_mint_nft_instruction(payer, &nft, collection.mint),
        &new_nft_accounts(&nft),
    )
    .assert_success();

    let unverified_metadata = svm.get_account(&nft.metadata).unwrap().data;

    let result = svm.process_instruction(
        &build_verify_collection_instruction(payer, &nft, &collection),
        &[],
    );
    result.assert_success();

    // Verification flips the collection's `verified` flag in the NFT's
    // metadata, so the account data must have changed.
    let verified_metadata = result.account(&nft.metadata).unwrap().data.clone();
    assert!(
        contains_bytes(&verified_metadata, collection.mint.as_ref()),
        "Metadata should still reference the collection mint"
    );
    assert_ne!(
        unverified_metadata, verified_metadata,
        "verify_collection should update the NFT metadata"
    );
}
