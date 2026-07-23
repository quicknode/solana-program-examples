//! quasar-test integration tests, ported from the Anchor twin's LiteSVM suite.
//!
//! The SVM loads this program, the SPL Token program, and the Metaplex Token
//! Metadata fixture shared with the Anchor twin
//! (`../anchor/tests/fixtures/mpl_token_metadata.so`), then exercises the
//! full collection lifecycle: create_collection, mint_nft, verify_collection.

extern crate std;
use {
    crate::cpi::{CreateCollectionInstruction, MintNftInstruction, VerifyCollectionInstruction},
    quasar_test::prelude::*,
};

const METADATA_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const COLLECTION_MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const COLLECTION_DESTINATION: Pubkey = Pubkey::new_from_array([3; 32]);
const NFT_MINT: Pubkey = Pubkey::new_from_array([4; 32]);
const NFT_DESTINATION: Pubkey = Pubkey::new_from_array([5; 32]);

const COLLECTION_NAME: &str = "Quasar Collection";
const COLLECTION_SYMBOL: &str = "QCOL";
const COLLECTION_URI: &str = "https://example.com/collection.json";
const NFT_NAME: &str = "Quasar NFT #1";
const NFT_SYMBOL: &str = "QNFT";
const NFT_URI: &str = "https://example.com/nft-1.json";

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

/// Returns true if `haystack` contains `needle` anywhere. Used to check that
/// caller-supplied metadata strings landed in the Metaplex metadata account
/// without fully deserializing the Metaplex layout.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Register the payer and the Metaplex Token Metadata program fixture
/// (the fixture binary is shared with the Anchor twin's LiteSVM suite).
fn base_world(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let metadata_elf = std::fs::read("../anchor/tests/fixtures/mpl_token_metadata.so").unwrap();
    test.add(Program::new(METADATA_PROGRAM_ID, &metadata_elf));
}

fn create_collection(test: &mut Test) -> Outcome {
    test.send(CreateCollectionInstruction {
        user: PAYER,
        mint: COLLECTION_MINT,
        metadata: derive_metadata_pda(&COLLECTION_MINT),
        master_edition: derive_edition_pda(&COLLECTION_MINT),
        destination: COLLECTION_DESTINATION,
        name: COLLECTION_NAME.into(),
        symbol: COLLECTION_SYMBOL.into(),
        uri: COLLECTION_URI.into(),
    })
}

fn mint_nft(test: &mut Test) -> Outcome {
    test.send(MintNftInstruction {
        owner: PAYER,
        mint: NFT_MINT,
        destination: NFT_DESTINATION,
        metadata: derive_metadata_pda(&NFT_MINT),
        master_edition: derive_edition_pda(&NFT_MINT),
        collection_mint: COLLECTION_MINT,
        name: NFT_NAME.into(),
        symbol: NFT_SYMBOL.into(),
        uri: NFT_URI.into(),
    })
}

#[quasar_test]
fn create_collection_mints_the_collection_nft(test: &mut Test) {
    base_world(test);

    create_collection(test)
        .succeeds()
        // The collection mint exists and 1 token was minted to the destination.
        .has_supply(COLLECTION_MINT, 1)
        .has_tokens(COLLECTION_DESTINATION, 1);

    // The metadata account carries the caller-supplied name, and the master
    // edition exists.
    let metadata_account = test
        .account(derive_metadata_pda(&COLLECTION_MINT))
        .unwrap();
    assert!(
        contains_bytes(&metadata_account.data, COLLECTION_NAME.as_bytes()),
        "Metadata should contain the caller-supplied collection name"
    );
    assert!(!test
        .account(derive_edition_pda(&COLLECTION_MINT))
        .unwrap()
        .data
        .is_empty());
}

#[quasar_test]
fn mint_nft_references_the_collection_unverified(test: &mut Test) {
    base_world(test);
    create_collection(test).succeeds();

    // Mint an NFT into the collection. Only the NFT's own accounts are new;
    // the payer, authority PDA, and collection mint persist in the SVM.
    mint_nft(test).succeeds().has_tokens(NFT_DESTINATION, 1);

    let nft_metadata_account = test.account(derive_metadata_pda(&NFT_MINT)).unwrap();
    assert!(
        contains_bytes(&nft_metadata_account.data, NFT_NAME.as_bytes()),
        "Metadata should contain the caller-supplied NFT name"
    );
    // The metadata carries the (unverified) collection reference.
    assert!(
        contains_bytes(&nft_metadata_account.data, COLLECTION_MINT.as_ref()),
        "Metadata should reference the collection mint"
    );
}

#[quasar_test]
fn verify_collection_updates_the_nft_metadata(test: &mut Test) {
    base_world(test);
    create_collection(test).succeeds();
    mint_nft(test).succeeds();

    let unverified_metadata = test.account(derive_metadata_pda(&NFT_MINT)).unwrap().data;

    test.send(VerifyCollectionInstruction {
        authority: PAYER,
        metadata: derive_metadata_pda(&NFT_MINT),
        collection_mint: COLLECTION_MINT,
        collection_metadata: derive_metadata_pda(&COLLECTION_MINT),
        collection_master_edition: derive_edition_pda(&COLLECTION_MINT),
    })
    .succeeds();

    // Verification flips the collection's `verified` flag in the NFT's
    // metadata, so the account data must have changed.
    let verified_metadata = test.account(derive_metadata_pda(&NFT_MINT)).unwrap().data;
    assert!(
        contains_bytes(&verified_metadata, COLLECTION_MINT.as_ref()),
        "Metadata should still reference the collection mint"
    );
    assert_ne!(
        unverified_metadata, verified_metadata,
        "verify_collection should update the NFT metadata"
    );
}
