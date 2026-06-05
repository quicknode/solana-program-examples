//! LiteSVM integration test for the cnft-vault Anchor program.
//!
//! Full flow exercised:
//!   1. Load the cnft-vault program plus the three mainnet fixtures
//!      (mpl-bubblegum, spl-account-compression, spl-noop) into LiteSVM.
//!   2. Allocate + initialize a Bubblegum Merkle tree (max_depth=3,
//!      max_buffer_size=8, canopy=0) via `create_tree_config`.
//!   3. Mint a single cNFT whose leaf_owner is the vault PDA (so the vault
//!      holds it) via `mint_v1`.
//!   4. Recompute `data_hash` / `creator_hash` exactly as Bubblegum does.
//!   5. Build the Merkle proof for leaf 0 (all empty-node siblings) and read
//!      the current root from the on-chain tree account.
//!   6. Call our program's `withdraw_cnft`, which CPIs Bubblegum `Transfer`
//!      signed by the vault PDA (`invoke_signed`), to move the cNFT to a
//!      recipient. Assert the transaction succeeds and that a second withdraw
//!      with the now-stale root fails (the leaf moved, so the root changed).

use {
    borsh::BorshSerialize,
    litesvm::LiteSVM,
    solana_instruction::{account_meta::AccountMeta, Instruction},
    solana_keccak_hasher::hashv,
    solana_keypair::Keypair,
    solana_message::Message,
    solana_pubkey::{pubkey, Pubkey},
    solana_signer::Signer,
    solana_transaction::Transaction,
};

// ---- Program IDs ----------------------------------------------------------

const CNFT_VAULT_ID: Pubkey = pubkey!("Fd4iwpPWaCU8BNwGQGtvvrcvG4Tfizq3RgLm8YLBJX6D");
const BUBBLEGUM_ID: Pubkey = pubkey!("BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY");
const COMPRESSION_ID: Pubkey = pubkey!("cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK");
const NOOP_ID: Pubkey = pubkey!("noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV");
const SYSTEM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

// ---- Bubblegum instruction discriminators ---------------------------------

const CREATE_TREE_CONFIG_DISC: [u8; 8] = [165, 83, 136, 142, 89, 202, 47, 220];
const MINT_V1_DISC: [u8; 8] = [145, 98, 192, 118, 184, 147, 118, 104];

// ---- Tree parameters ------------------------------------------------------

const MAX_DEPTH: u32 = 3;
const MAX_BUFFER_SIZE: u32 = 8;

// ---- MetadataArgs (mirrors mpl_bubblegum::types::MetadataArgs borsh layout) ----

#[derive(BorshSerialize, Clone)]
struct Creator {
    address: [u8; 32],
    verified: bool,
    share: u8,
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
    token_standard: Option<u8>, // TokenStandard enum, encoded by variant index
    collection: Option<u8>,     // None — Collection, kept absent
    uses: Option<u8>,           // None — Uses, kept absent
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

// ---- Anchor discriminator for withdraw_cnft --------------------------------

fn withdraw_cnft_disc() -> [u8; 8] {
    // sha256("global:withdraw_cnft")[..8]. Implemented inline to avoid pulling
    // a crypto crate that conflicts with the program's solana version.
    let digest = sha256(b"global:withdraw_cnft");
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

// Minimal SHA-256 (FIPS 180-4) — only used to derive the Anchor discriminator.
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

// ---- ConcurrentMerkleTree<3,8> account layout ------------------------------
//
// account_data = header (56 bytes) || zero-copy ConcurrentMerkleTree (1248) || canopy (0)
//
// Header (ConcurrentMerkleTreeHeader): account_type(1) + header-enum-discriminant(1)
//   + V1{ max_buffer_size(4), max_depth(4), authority(32), creation_slot(8),
//         is_batch_initialized(1), _padding[5] } = 56 bytes total.
//
// ConcurrentMerkleTree<3,8> (#[repr(C)]):
//   sequence_number u64 (off 0)
//   active_index    u64 (off 8)
//   buffer_size     u64 (off 16)
//   change_logs [ChangeLog<3>; 8]  (off 24), stride = 136
//       ChangeLog<3> = root[32] + path[3*32] + index u32 + _padding u32 = 136
//   rightmost_proof Path<3>
//
// Current root = change_logs[active_index].root.

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

// ---- Helpers ---------------------------------------------------------------

fn send(
    svm: &mut LiteSVM,
    ixs: Vec<Instruction>,
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<(), Box<litesvm::types::FailedTransactionMetadata>> {
    let msg = Message::new(&ixs, Some(&payer.pubkey()));
    let blockhash = svm.latest_blockhash();
    let mut tx = Transaction::new_unsigned(msg);
    tx.sign(signers, blockhash);
    svm.send_transaction(tx).map(|_| ()).map_err(Box::new)
}

#[test]
fn test_withdraw_cnft() {
    let mut svm = LiteSVM::new();

    // Load the cnft-vault program and the three mainnet fixtures.
    svm.add_program(
        CNFT_VAULT_ID,
        include_bytes!("../../../target/deploy/cnft_vault.so"),
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

    // Fund payer.
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * solana_native_token::LAMPORTS_PER_SOL)
        .unwrap();

    // The vault PDA that owns the cNFT and signs the transfer CPI.
    // seeds = [b"cNFT-vault"] under the cnft-vault program.
    let (vault_pda, _vault_bump) = Pubkey::find_program_address(&[b"cNFT-vault"], &CNFT_VAULT_ID);

    // The recipient of the withdraw.
    let recipient = Keypair::new();

    // Create the Merkle tree account, owned by the compression program.
    let merkle_tree = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(TREE_ACCOUNT_SIZE);
    let create_acc = Instruction {
        program_id: SYSTEM_ID,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(merkle_tree.pubkey(), true),
        ],
        // System CreateAccount: u32 instruction index (0) + lamports u64 + space u64 + owner [32]
        data: {
            let mut d = Vec::new();
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&rent.to_le_bytes());
            d.extend_from_slice(&(TREE_ACCOUNT_SIZE as u64).to_le_bytes());
            d.extend_from_slice(COMPRESSION_ID.as_ref());
            d
        },
    };

    // tree_authority (a.k.a tree_config) PDA = [merkle_tree] under bubblegum.
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

    send(
        &mut svm,
        vec![create_acc, create_tree_ix],
        &payer,
        &[&payer, &merkle_tree],
    )
    .expect("create_tree_config should succeed");

    // Build the MetadataArgs for the single cNFT we mint. The leaf owner /
    // delegate are the vault PDA, so the vault holds the cNFT.
    let creator = Creator {
        address: payer.pubkey().to_bytes(),
        verified: false,
        share: 100,
    };
    let metadata = MetadataArgs {
        name: "Vault cNFT".to_string(),
        symbol: "VCNFT".to_string(),
        uri: "https://example.com/nft.json".to_string(),
        seller_fee_basis_points: 500,
        primary_sale_happened: false,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(0), // TokenStandard::NonFungible
        collection: None,
        uses: None,
        token_program_version: TokenProgramVersion::Original,
        creators: vec![creator.clone()],
    };

    // mint_v1 — leaf_owner and leaf_delegate are the vault PDA.
    let mint_ix = Instruction {
        program_id: BUBBLEGUM_ID,
        accounts: vec![
            AccountMeta::new(tree_config, false),
            AccountMeta::new_readonly(vault_pda, false),
            AccountMeta::new_readonly(vault_pda, false), // leaf_delegate
            AccountMeta::new(merkle_tree.pubkey(), false),
            AccountMeta::new_readonly(payer.pubkey(), true),
            AccountMeta::new_readonly(payer.pubkey(), true), // tree_creator_or_delegate
            AccountMeta::new_readonly(NOOP_ID, false),
            AccountMeta::new_readonly(COMPRESSION_ID, false),
            AccountMeta::new_readonly(SYSTEM_ID, false),
        ],
        data: {
            let mut d = MINT_V1_DISC.to_vec();
            d.extend_from_slice(&borsh::to_vec(&metadata).unwrap());
            d
        },
    };
    send(&mut svm, vec![mint_ix], &payer, &[&payer]).expect("mint_v1 should succeed");

    // Recompute data_hash and creator_hash exactly as Bubblegum does.
    let data_hash = hash_metadata(&metadata);
    let creator_hash = hash_creators(&metadata.creators);

    // Proof for leaf index 0 in an otherwise-empty tree: empty-node siblings.
    let proof = [empty_node(0), empty_node(1), empty_node(2)];

    // Read the current root from the on-chain tree account.
    let tree_data = svm.get_account(&merkle_tree.pubkey()).unwrap().data;
    let root = read_current_root(&tree_data);

    // Build withdraw_cnft via our program. Accounts per Withdraw struct:
    //   tree_authority (mut), leaf_owner (vault PDA), new_leaf_owner (recipient),
    //   merkle_tree (mut), log_wrapper, compression_program, bubblegum_program,
    //   system_program, then proof nodes as remaining accounts.
    let mut withdraw_accounts = vec![
        AccountMeta::new(tree_config, false),
        AccountMeta::new_readonly(vault_pda, false),
        AccountMeta::new_readonly(recipient.pubkey(), false),
        AccountMeta::new(merkle_tree.pubkey(), false),
        AccountMeta::new_readonly(NOOP_ID, false),
        AccountMeta::new_readonly(COMPRESSION_ID, false),
        AccountMeta::new_readonly(BUBBLEGUM_ID, false),
        AccountMeta::new_readonly(SYSTEM_ID, false),
    ];
    for node in proof.iter() {
        withdraw_accounts.push(AccountMeta::new_readonly(
            Pubkey::new_from_array(*node),
            false,
        ));
    }

    let withdraw_data = {
        let mut d = withdraw_cnft_disc().to_vec();
        d.extend_from_slice(&root);
        d.extend_from_slice(&data_hash);
        d.extend_from_slice(&creator_hash);
        d.extend_from_slice(&0u64.to_le_bytes()); // nonce
        d.extend_from_slice(&0u32.to_le_bytes()); // index
        d
    };

    let withdraw_ix = Instruction {
        program_id: CNFT_VAULT_ID,
        accounts: withdraw_accounts.clone(),
        data: withdraw_data.clone(),
    };

    // Withdraw is signed by the payer (the vault PDA signs via invoke_signed
    // inside the program, not as a transaction signer).
    send(&mut svm, vec![withdraw_ix], &payer, &[&payer]).expect("withdraw_cnft should succeed");

    // After transfer, leaf 0's owner changed (vault -> recipient), so the root
    // moved. A second withdraw replaying the same (root, hashes) must fail: the
    // cached root is stale and the leaf no longer hashes to it for the vault.
    let withdraw_ix2 = Instruction {
        program_id: CNFT_VAULT_ID,
        accounts: withdraw_accounts,
        data: withdraw_data,
    };
    let second = send(&mut svm, vec![withdraw_ix2], &payer, &[&payer]);
    assert!(
        second.is_err(),
        "second withdraw must fail: leaf already transferred out of the vault"
    );
}
