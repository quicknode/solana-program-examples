//! LiteSVM integration test for the `cutils` Anchor program.
//!
//! The cutils program exposes two instructions:
//!   * `mint` - CPIs Bubblegum `MintToCollectionV1` to mint a cNFT into a
//!     (Token-Metadata) verified collection and a Bubblegum tree.
//!   * `verify` - recomputes the V1 leaf hash and CPIs SPL account-compression
//!     `verify_leaf` to prove the leaf is present in the tree.
//!
//! Full flow exercised here:
//!   1. Load the cutils program plus the four mainnet fixtures
//!      (mpl-bubblegum, spl-account-compression, spl-noop, mpl-token-metadata).
//!      SPL Token + the Associated Token program are provided by LiteSVM.
//!   2. Build a real Token-Metadata *sized collection* NFT (mint + metadata +
//!      master edition) so Bubblegum's `MintToCollectionV1` collection check
//!      passes. `payer` is the collection update authority.
//!   3. Allocate + initialize a Bubblegum Merkle tree (max_depth=3,
//!      max_buffer_size=8, canopy=0) via `create_tree_config` (same mechanics
//!      as the sibling cnft-burn reference test).
//!   4. Call cutils `mint` to mint one cNFT into that tree/collection.
//!   5. Recompute `data_hash` / `creator_hash` exactly as Bubblegum stores
//!      them for the minted leaf (note: after MintToCollectionV1 the collection
//!      is stored *verified*, so the data_hash reflects `verified = true`).
//!   6. Build the Merkle proof for leaf 0 (all empty-node siblings), read the
//!      live root from the onchain tree account, and call cutils `verify`,
//!      asserting success. A second `verify` with a tampered data_hash must
//!      fail.

use {
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
    borsh::BorshSerialize,
    solana_instruction::{account_meta::AccountMeta, Instruction},
    solana_keccak_hasher::hashv,
    solana_kite::{create_wallet, send_transaction_from_instructions},
    solana_pubkey::{pubkey, Pubkey},
};

// ---- Program IDs ----------------------------------------------------------

// Track the crate's declared id (CI runs `anchor keys sync` before building).
const CUTILS_ID: Pubkey = cutils::ID;
const BUBBLEGUM_ID: Pubkey = pubkey!("BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY");
const COMPRESSION_ID: Pubkey = pubkey!("cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK");
const NOOP_ID: Pubkey = pubkey!("noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV");
const TOKEN_METADATA_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
const TOKEN_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const SYSTEM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

// ---- Instruction discriminators -------------------------------------------

const CREATE_TREE_CONFIG_DISC: [u8; 8] = [165, 83, 136, 142, 89, 202, 47, 220];

// ---- Tree parameters ------------------------------------------------------

const MAX_DEPTH: u32 = 3;
const MAX_BUFFER_SIZE: u32 = 8;

// ---- MetadataArgs (mirrors mpl_bubblegum::types::MetadataArgs borsh layout) ----
//
// These mirror the values the cutils `mint` instruction hardcodes (see
// programs/cutils/src/instructions/mint.rs): name "BURGER", symbol "BURG",
// the test-supplied uri, a single creator equal to the collection authority,
// seller_fee 0, primary_sale_happened false, is_mutable false, edition_nonce
// Some(0), token_standard NonFungible, token_program_version Original, and a
// Collection pointing at the collection mint.

#[derive(BorshSerialize, Clone)]
struct Creator {
    address: [u8; 32],
    verified: bool,
    share: u8,
}

#[derive(BorshSerialize, Clone)]
struct Collection {
    verified: bool,
    key: [u8; 32],
}

#[derive(BorshSerialize, Clone)]
enum TokenProgramVersion {
    #[allow(dead_code)]
    Original,
    #[allow(dead_code)]
    Token2022,
}

#[derive(BorshSerialize, Clone)]
struct MetadataArgs {
    name: String,
    symbol: String,
    uri: String,
    seller_fee_basis_points: u16,
    primary_sale_happened: bool,
    is_mutable: bool,
    edition_nonce: Option<u8>,
    token_standard: Option<u8>, // TokenStandard, variant index (NonFungible = 0)
    collection: Option<Collection>,
    uses: Option<u8>, // None - Uses, kept absent
    token_program_version: TokenProgramVersion,
    creators: Vec<Creator>,
}

// ---- Hashing, exactly as the Bubblegum program does ------------------------

fn hash_metadata(metadata: &MetadataArgs) -> [u8; 32] {
    let serialized = borsh::to_vec(metadata).unwrap();
    let inner = hashv(&[serialized.as_slice()]).to_bytes();
    hashv(&[&inner, &metadata.seller_fee_basis_points.to_le_bytes()]).to_bytes()
}

fn hash_creators(creators: &[Creator]) -> [u8; 32] {
    let creator_data: Vec<Vec<u8>> = creators
        .iter()
        .map(|c| [c.address.as_ref(), &[c.verified as u8], &[c.share]].concat())
        .collect();
    hashv(
        creator_data
            .iter()
            .map(|c| c.as_slice())
            .collect::<Vec<&[u8]>>()
            .as_slice(),
    )
    .to_bytes()
}

// ---- SPL account-compression empty-node helper -----------------------------

fn empty_node(level: u32) -> [u8; 32] {
    if level == 0 {
        return [0u8; 32];
    }
    let lower = empty_node(level - 1);
    hashv(&[&lower, &lower]).to_bytes()
}

// ---- ConcurrentMerkleTree<3,8> account layout ------------------------------
//
// account_data = header (56) || zero-copy ConcurrentMerkleTree (1248) || canopy(0)
// Current root = change_logs[active_index].root. See cnft-burn reference test
// for the full layout derivation.

const HEADER_SIZE: usize = 56;
const CMT_SIZE: usize = {
    let changelog = 32 + 3 * 32 + 4 + 4; // 136
    let path = 3 * 32 + 32 + 4 + 4; // 136
    8 + 8 + 8 + changelog * 8 + path
};
const TREE_ACCOUNT_SIZE: usize = HEADER_SIZE + CMT_SIZE;

fn read_current_root(data: &[u8]) -> [u8; 32] {
    let tree = &data[HEADER_SIZE..];
    let active_index = u64::from_le_bytes(tree[8..16].try_into().unwrap()) as usize;
    let changelog_stride = 136;
    let root_off = 24 + active_index * changelog_stride;
    let mut root = [0u8; 32];
    root.copy_from_slice(&tree[root_off..root_off + 32]);
    root
}

// ---- Anchor discriminators -------------------------------------------------

fn anchor_disc(name: &str) -> [u8; 8] {
    let digest = sha256(format!("global:{name}").as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

// Minimal SHA-256 (FIPS 180-4) - used only to derive Anchor discriminators,
// avoiding a crypto crate that conflicts with the program's solana version.
fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = input.to_vec();
    let bitlen = (input.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, wi) in w.iter_mut().enumerate().take(16) {
            *wi = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ ((!v[4]) & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(t1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = t1.wrapping_add(t2);
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

// ---- keccak256 leaf hash (mirrors leaf_schema_v1_hash in bubblegum_types.rs) ----

fn leaf_schema_v1_hash(
    id: &Pubkey,
    owner: &Pubkey,
    delegate: &Pubkey,
    nonce: u64,
    data_hash: &[u8; 32],
    creator_hash: &[u8; 32],
) -> [u8; 32] {
    hashv(&[
        &[1u8], // Version::V1 = 1
        id.as_ref(),
        owner.as_ref(),
        delegate.as_ref(),
        &nonce.to_le_bytes(),
        data_hash,
        creator_hash,
    ])
    .to_bytes()
}

fn get_asset_id(tree: &Pubkey, nonce: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[b"asset", tree.as_ref(), &nonce.to_le_bytes()],
        &BUBBLEGUM_ID,
    )
    .0
}

// ---- Helpers ---------------------------------------------------------------

fn metadata_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"metadata", TOKEN_METADATA_ID.as_ref(), mint.as_ref()],
        &TOKEN_METADATA_ID,
    )
    .0
}

fn master_edition_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"metadata",
            TOKEN_METADATA_ID.as_ref(),
            mint.as_ref(),
            b"edition",
        ],
        &TOKEN_METADATA_ID,
    )
    .0
}

/// Create a Token-Metadata *sized collection* NFT whose update authority is
/// `authority`. Returns (collection_mint, collection_metadata, master_edition).
fn create_collection_nft(
    svm: &mut LiteSVM,
    payer: &Keypair,
    authority: &Keypair,
) -> (Keypair, Pubkey, Pubkey) {
    let mint = Keypair::new();
    let metadata = metadata_pda(&mint.pubkey());
    let master_edition = master_edition_pda(&mint.pubkey());

    // 1. Create + initialize the SPL mint (mint authority = authority).
    let mint_rent = svm.minimum_balance_for_rent_exemption(82);
    let create_mint = Instruction {
        program_id: SYSTEM_ID,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(mint.pubkey(), true),
        ],
        data: {
            let mut d = 0u32.to_le_bytes().to_vec();
            d.extend_from_slice(&mint_rent.to_le_bytes());
            d.extend_from_slice(&82u64.to_le_bytes());
            d.extend_from_slice(TOKEN_ID.as_ref());
            d
        },
    };
    // InitializeMint2 (tag 20): decimals=0, mint_authority=authority, freeze=None.
    let init_mint = Instruction {
        program_id: TOKEN_ID,
        accounts: vec![AccountMeta::new(mint.pubkey(), false)],
        data: {
            let mut d = vec![20u8, 0u8];
            d.extend_from_slice(authority.pubkey().as_ref());
            d.push(1); // freeze authority present (required for NFTs)
            d.extend_from_slice(authority.pubkey().as_ref());
            d
        },
    };

    // 2. Create + initialize a token account, then mint 1 to it.
    let token_account = Keypair::new();
    let acct_rent = svm.minimum_balance_for_rent_exemption(165);
    let create_token_acct = Instruction {
        program_id: SYSTEM_ID,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(token_account.pubkey(), true),
        ],
        data: {
            let mut d = 0u32.to_le_bytes().to_vec();
            d.extend_from_slice(&acct_rent.to_le_bytes());
            d.extend_from_slice(&165u64.to_le_bytes());
            d.extend_from_slice(TOKEN_ID.as_ref());
            d
        },
    };
    // InitializeAccount3 (tag 18): owner = authority.
    let init_token_acct = Instruction {
        program_id: TOKEN_ID,
        accounts: vec![
            AccountMeta::new(token_account.pubkey(), false),
            AccountMeta::new_readonly(mint.pubkey(), false),
        ],
        data: {
            let mut d = vec![18u8];
            d.extend_from_slice(authority.pubkey().as_ref());
            d
        },
    };
    // MintTo (tag 7): amount = 1.
    let mint_to = Instruction {
        program_id: TOKEN_ID,
        accounts: vec![
            AccountMeta::new(mint.pubkey(), false),
            AccountMeta::new(token_account.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: {
            let mut d = vec![7u8];
            d.extend_from_slice(&1u64.to_le_bytes());
            d
        },
    };

    send_transaction_from_instructions(
        svm,
        vec![
            create_mint,
            init_mint,
            create_token_acct,
            init_token_acct,
            mint_to,
        ],
        &[payer, &mint, &token_account, authority],
        &payer.pubkey(),
    )
    .expect("collection mint setup should succeed");

    // 3. CreateMetadataAccountV3 (disc 33) with collection_details = V1{size:0}
    //    (this makes it a *sized* collection that Bubblegum accepts).
    let create_metadata = Instruction {
        program_id: TOKEN_METADATA_ID,
        accounts: vec![
            AccountMeta::new(metadata, false),
            AccountMeta::new_readonly(mint.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), true), // mint_authority
            AccountMeta::new(payer.pubkey(), true),              // payer
            AccountMeta::new_readonly(authority.pubkey(), true), // update_authority
            AccountMeta::new_readonly(SYSTEM_ID, false),
        ],
        data: {
            let mut d = vec![33u8];
            // DataV2
            "Collection".serialize(&mut d).unwrap(); // name
            "COLL".serialize(&mut d).unwrap(); // symbol
            "https://example.com/collection.json"
                .serialize(&mut d)
                .unwrap(); // uri
            d.extend_from_slice(&0u16.to_le_bytes()); // seller_fee_basis_points
            d.push(0); // creators: Option<Vec<Creator>> = None
            d.push(0); // collection: Option<Collection> = None
            d.push(0); // uses: Option<Uses> = None
            d.push(1); // is_mutable = true
                       // collection_details: Option<CollectionDetails> = Some(V1 { size: 0 })
            d.push(1);
            d.push(0); // CollectionDetails::V1 variant
            d.extend_from_slice(&0u64.to_le_bytes()); // size
            d
        },
    };

    // 4. CreateMasterEditionV3 (disc 17) with max_supply = Some(0).
    let create_master_edition = Instruction {
        program_id: TOKEN_METADATA_ID,
        accounts: vec![
            AccountMeta::new(master_edition, false),
            AccountMeta::new(mint.pubkey(), false),
            AccountMeta::new_readonly(authority.pubkey(), true), // update_authority
            AccountMeta::new_readonly(authority.pubkey(), true), // mint_authority
            AccountMeta::new(payer.pubkey(), true),              // payer
            AccountMeta::new(metadata, false),
            AccountMeta::new_readonly(TOKEN_ID, false),
            AccountMeta::new_readonly(SYSTEM_ID, false),
        ],
        data: {
            let mut d = vec![17u8];
            d.push(1); // max_supply: Option<u64> = Some
            d.extend_from_slice(&0u64.to_le_bytes());
            d
        },
    };

    send_transaction_from_instructions(
        svm,
        vec![create_metadata, create_master_edition],
        &[payer, authority],
        &payer.pubkey(),
    )
    .expect("collection metadata + master edition should succeed");

    (mint, metadata, master_edition)
}

#[test]
fn test_cutils_mint_and_verify() {
    let mut svm = anchor_v2_testing::svm();

    // Load the cutils program and the mainnet fixtures.
    svm.add_program(
        CUTILS_ID,
        include_bytes!("../../../target/deploy/cutils.so"),
    )
    .unwrap();
    svm.add_program(
        BUBBLEGUM_ID,
        include_bytes!("../../../tests/fixtures/mpl_bubblegum.so"),
    )
    .unwrap();
    svm.add_program(
        COMPRESSION_ID,
        include_bytes!("../../../tests/fixtures/spl_account_compression.so"),
    )
    .unwrap();
    svm.add_program(
        NOOP_ID,
        include_bytes!("../../../tests/fixtures/spl_noop.so"),
    )
    .unwrap();
    svm.add_program(
        TOKEN_METADATA_ID,
        include_bytes!("../../../tests/fixtures/mpl_token_metadata.so"),
    )
    .unwrap();

    // Fund payer (also the collection authority / tree delegate / leaf owner).
    let payer = create_wallet(&mut svm, 1_000 * 1_000_000_000).unwrap();
    let leaf_owner = create_wallet(&mut svm, 10 * 1_000_000_000).unwrap();

    // Build the verified collection NFT (payer is the collection authority).
    let (collection_mint, collection_metadata, collection_master_edition) =
        create_collection_nft(&mut svm, &payer, &payer);

    // Create the Merkle tree account (owned by the compression program).
    let merkle_tree = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(TREE_ACCOUNT_SIZE);
    let create_acc = Instruction {
        program_id: SYSTEM_ID,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(merkle_tree.pubkey(), true),
        ],
        data: {
            let mut d = 0u32.to_le_bytes().to_vec();
            d.extend_from_slice(&rent.to_le_bytes());
            d.extend_from_slice(&(TREE_ACCOUNT_SIZE as u64).to_le_bytes());
            d.extend_from_slice(COMPRESSION_ID.as_ref());
            d
        },
    };

    let (tree_config, _) =
        Pubkey::find_program_address(&[merkle_tree.pubkey().as_ref()], &BUBBLEGUM_ID);

    // create_tree_config(max_depth, max_buffer_size, public=None)
    let create_tree_ix = Instruction {
        program_id: BUBBLEGUM_ID,
        accounts: vec![
            AccountMeta::new(tree_config, false),
            AccountMeta::new(merkle_tree.pubkey(), false),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(payer.pubkey(), true), // tree_creator
            AccountMeta::new_readonly(NOOP_ID, false),
            AccountMeta::new_readonly(COMPRESSION_ID, false),
            AccountMeta::new_readonly(SYSTEM_ID, false),
        ],
        data: {
            let mut d = CREATE_TREE_CONFIG_DISC.to_vec();
            d.extend_from_slice(&MAX_DEPTH.to_le_bytes());
            d.extend_from_slice(&MAX_BUFFER_SIZE.to_le_bytes());
            d.push(0); // Option<bool>::None
            d
        },
    };

    send_transaction_from_instructions(
        &mut svm,
        vec![create_acc, create_tree_ix],
        &[&payer, &merkle_tree],
        &payer.pubkey(),
    )
    .expect("create_tree_config should succeed");

    // ---- Call cutils `mint` -------------------------------------------------
    //
    // Account order mirrors the Mint<'info> struct in mint.rs:
    //   payer, tree_authority, leaf_owner, leaf_delegate, merkle_tree,
    //   tree_delegate, collection_authority, collection_authority_record_pda,
    //   collection_mint, collection_metadata, edition_account, bubblegum_signer,
    //   log_wrapper, compression_program, token_metadata_program,
    //   bubblegum_program, system_program.
    //
    // When there is no collection-authority-record PDA, that account must be
    // the Bubblegum program id. bubblegum_signer is the `collection_cpi` PDA.
    let (bubblegum_signer, _) = Pubkey::find_program_address(&[b"collection_cpi"], &BUBBLEGUM_ID);

    let uri = "https://example.com/burger.json".to_string();
    let mint_ix = Instruction {
        program_id: CUTILS_ID,
        accounts: vec![
            AccountMeta::new_readonly(payer.pubkey(), true), // payer (signer)
            AccountMeta::new(tree_config, false),
            AccountMeta::new_readonly(leaf_owner.pubkey(), false),
            AccountMeta::new_readonly(leaf_owner.pubkey(), false), // leaf_delegate
            AccountMeta::new(merkle_tree.pubkey(), false),
            AccountMeta::new_readonly(payer.pubkey(), true), // tree_delegate (signer)
            AccountMeta::new_readonly(payer.pubkey(), true), // collection_authority (signer)
            AccountMeta::new_readonly(BUBBLEGUM_ID, false),  // collection_authority_record_pda
            AccountMeta::new_readonly(collection_mint.pubkey(), false),
            AccountMeta::new(collection_metadata, false),
            AccountMeta::new(collection_master_edition, false), // edition_account (mut: set_and_verify writes details)
            AccountMeta::new_readonly(bubblegum_signer, false),
            AccountMeta::new_readonly(NOOP_ID, false), // log_wrapper
            AccountMeta::new_readonly(COMPRESSION_ID, false),
            AccountMeta::new_readonly(TOKEN_METADATA_ID, false),
            AccountMeta::new_readonly(BUBBLEGUM_ID, false),
            AccountMeta::new_readonly(SYSTEM_ID, false),
        ],
        data: {
            let mut d = anchor_disc("mint").to_vec();
            // MintParams { uri: String }
            uri.serialize(&mut d).unwrap();
            d
        },
    };

    send_transaction_from_instructions(&mut svm, vec![mint_ix], &[&payer], &payer.pubkey())
        .expect("cutils mint should succeed");

    // ---- Recompute the stored leaf's data_hash / creator_hash ---------------
    //
    // Mirror exactly what cutils mint hardcodes; after MintToCollectionV1 the
    // collection is stored *verified = true*.
    let creator = Creator {
        address: payer.pubkey().to_bytes(),
        verified: false,
        share: 100,
    };
    let metadata = MetadataArgs {
        name: "BURGER".to_string(),
        symbol: "BURG".to_string(),
        uri,
        seller_fee_basis_points: 0,
        primary_sale_happened: false,
        is_mutable: false,
        edition_nonce: Some(0),
        token_standard: Some(0), // TokenStandard::NonFungible
        collection: Some(Collection {
            verified: true, // verified by MintToCollectionV1
            key: collection_mint.pubkey().to_bytes(),
        }),
        uses: None,
        token_program_version: TokenProgramVersion::Original,
        creators: vec![creator.clone()],
    };

    let data_hash = hash_metadata(&metadata);
    let creator_hash = hash_creators(&metadata.creators);

    // Proof for leaf index 0 in an otherwise-empty tree: empty-node siblings.
    let proof = [empty_node(0), empty_node(1), empty_node(2)];

    // Read the live root from the onchain tree account.
    let tree_data = svm.get_account(&merkle_tree.pubkey()).unwrap().data;
    let root = read_current_root(&tree_data);

    // Sanity: the leaf we computed must equal what the program will recompute,
    // and the proof must rebuild the onchain root.
    let asset_id = get_asset_id(&merkle_tree.pubkey(), 0);
    let leaf = leaf_schema_v1_hash(
        &asset_id,
        &leaf_owner.pubkey(),
        &leaf_owner.pubkey(),
        0,
        &data_hash,
        &creator_hash,
    );
    let mut node = leaf;
    let mut idx = 0u32;
    for sibling in proof.iter() {
        node = if idx & 1 == 0 {
            hashv(&[&node, sibling]).to_bytes()
        } else {
            hashv(&[sibling, &node]).to_bytes()
        };
        idx >>= 1;
    }
    assert_eq!(
        node, root,
        "locally recomputed root must match the onchain tree root"
    );

    // ---- Call cutils `verify` ----------------------------------------------
    //
    // Accounts per Verify<'info>: leaf_owner (signer), leaf_delegate,
    // merkle_tree, compression_program, then proof nodes as remaining accounts.
    let build_verify = |dh: [u8; 32]| -> Instruction {
        let mut accounts = vec![
            AccountMeta::new_readonly(leaf_owner.pubkey(), true),
            AccountMeta::new_readonly(leaf_owner.pubkey(), false), // leaf_delegate
            AccountMeta::new_readonly(merkle_tree.pubkey(), false),
            AccountMeta::new_readonly(COMPRESSION_ID, false),
        ];
        for sibling in proof.iter() {
            accounts.push(AccountMeta::new_readonly(
                Pubkey::new_from_array(*sibling),
                false,
            ));
        }
        Instruction {
            program_id: CUTILS_ID,
            accounts,
            data: {
                let mut d = anchor_disc("verify").to_vec();
                // VerifyParams { root, data_hash, creator_hash, nonce, index }
                d.extend_from_slice(&root);
                d.extend_from_slice(&dh);
                d.extend_from_slice(&creator_hash);
                d.extend_from_slice(&0u64.to_le_bytes()); // nonce
                d.extend_from_slice(&0u32.to_le_bytes()); // index
                d
            },
        }
    };

    send_transaction_from_instructions(
        &mut svm,
        vec![build_verify(data_hash)],
        &[&leaf_owner],
        &leaf_owner.pubkey(),
    )
    .expect("cutils verify should succeed for the minted leaf");

    // A tampered data_hash must fail verification.
    let mut bad = data_hash;
    bad[0] ^= 0xff;
    let bad_result = send_transaction_from_instructions(
        &mut svm,
        vec![build_verify(bad)],
        &[&leaf_owner],
        &leaf_owner.pubkey(),
    );
    assert!(
        bad_result.is_err(),
        "verify must fail for an incorrect data_hash"
    );
}

#[test]
fn test_empty_node_matches_manual() {
    let e1 = empty_node(1);
    let manual = hashv(&[&[0u8; 32], &[0u8; 32]]).to_bytes();
    assert_eq!(e1, manual);
}
