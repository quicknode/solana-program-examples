//! quasar-test integration tests for the cnft-vault Quasar program.
//!
//! Ported from the Anchor twin's LiteSVM suite. The test world loads the
//! program plus the three mainnet fixtures (mpl-bubblegum,
//! spl-account-compression, spl-noop) from `../anchor/tests/fixtures/`, then:
//!   1. Initializes the vault PDA via `initialize_vault`, storing the
//!      withdraw authority.
//!   2. Creates a Bubblegum Merkle tree (max_depth=3, max_buffer_size=8,
//!      canopy=0) via `create_tree_config`. The pre-allocated tree account is
//!      installed as a compression-program-owned account, standing in for the
//!      system `create_account` step.
//!   3. Mints a cNFT whose leaf owner is the vault PDA via `mint_v1`.
//!   4. Recomputes `data_hash` / `creator_hash` exactly as Bubblegum does and
//!      builds the proof for leaf 0 (all empty-node siblings).
//!   5. Calls the program's withdraw handlers, which CPI Bubblegum `Transfer`
//!      signed by the vault PDA.
//!
//! Coverage:
//!   - withdraw by the stored authority succeeds (single and two-cNFT)
//!   - withdraw by a non-authority signer fails with
//!     `VaultError::InvalidWithdrawAuthority`
//!   - replaying a withdraw with the now-stale root fails
//!   - a two-cNFT withdraw whose proof lengths do not match the supplied
//!     proof accounts fails with `VaultError::ProofLengthMismatch`

extern crate std;
use {
    crate::{
        cpi::{InitializeVaultInstruction, WithdrawCnftInstruction, WithdrawTwoCnftsInstruction},
        error::VaultError,
        state::Vault,
    },
    borsh::BorshSerialize,
    quasar_test::prelude::*,
    solana_keccak_hasher::hashv,
    std::{string::ToString, vec, vec::Vec},
};

// ---- Program IDs ----------------------------------------------------------

const BUBBLEGUM_ID: Pubkey = Pubkey::from_str_const("BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY");
const COMPRESSION_ID: Pubkey =
    Pubkey::from_str_const("cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK");
const NOOP_ID: Pubkey = Pubkey::from_str_const("noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV");

// ---- Bubblegum instruction discriminators ---------------------------------

const CREATE_TREE_CONFIG_DISC: [u8; 8] = [165, 83, 136, 142, 89, 202, 47, 220];
const MINT_V1_DISC: [u8; 8] = [145, 98, 192, 118, 184, 147, 118, 104];

// ---- Tree parameters ------------------------------------------------------

const MAX_DEPTH: u32 = 3;
const MAX_BUFFER_SIZE: u32 = 8;

/// Lamports for the prefabricated tree accounts; comfortably above rent
/// exemption for every account size used here.
const FUNDING_LAMPORTS: u64 = 1_000_000_000;

// Deterministic addresses avoid Pubkey::new_unique(), whose global counter
// produces different values depending on test binary layout / discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const AUTHORITY: Pubkey = Pubkey::new_from_array([2; 32]);
const RECIPIENT: Pubkey = Pubkey::new_from_array([3; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([4; 32]);
const MERKLE_TREE_1: Pubkey = Pubkey::new_from_array([5; 32]);
const MERKLE_TREE_2: Pubkey = Pubkey::new_from_array([6; 32]);

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
    name: std::string::String,
    symbol: std::string::String,
    uri: std::string::String,
    seller_fee_basis_points: u16,
    primary_sale_happened: bool,
    is_mutable: bool,
    edition_nonce: Option<u8>,
    token_standard: Option<u8>, // TokenStandard enum, encoded by variant index
    collection: Option<u8>,     // None - Collection, kept absent
    uses: Option<u8>,           // None - Uses, kept absent
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

// ---- Fixture setup ----------------------------------------------------------

/// One Bubblegum tree holding a single cNFT owned by the vault PDA, plus
/// everything needed to withdraw it (root, hashes, proof).
struct TreeWithVaultCnft {
    merkle_tree: Pubkey,
    tree_config: Pubkey,
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    proof: [[u8; 32]; MAX_DEPTH as usize],
}

/// Load the external program fixtures (shared with the Anchor twin's LiteSVM
/// suite), fund the actors, and initialize the vault PDA with `AUTHORITY` as
/// its stored withdraw authority. Returns the vault PDA.
fn setup_vault(test: &mut Test) -> Pubkey {
    test.add(Program::new(
        BUBBLEGUM_ID,
        &std::fs::read("../anchor/tests/fixtures/mpl_bubblegum.so").unwrap(),
    ));
    test.add(Program::new(
        COMPRESSION_ID,
        &std::fs::read("../anchor/tests/fixtures/spl_account_compression.so").unwrap(),
    ));
    test.add(Program::new(
        NOOP_ID,
        &std::fs::read("../anchor/tests/fixtures/spl_noop.so").unwrap(),
    ));
    test.add(Wallet::new().at(PAYER));
    test.add(Wallet::new().at(AUTHORITY));

    let vault = test.derive_pda(Vault::seeds());

    // The vault PDA and system program are canonical derivations, so the
    // generated instruction only asks for the authority (who also pays).
    test.send(InitializeVaultInstruction {
        authority: AUTHORITY,
    })
    .succeeds();

    vault
}

/// Create a Bubblegum tree at `merkle_tree` and mint one cNFT into the vault.
fn create_tree_with_vault_cnft(
    test: &mut Test,
    vault: Pubkey,
    merkle_tree: Pubkey,
) -> TreeWithVaultCnft {
    // The allocated-but-uninitialized tree account the system program would
    // have created in the `create_account` step (a foreign-program account,
    // so prefabricating it is fine).
    test.set_account(Account::new(
        merkle_tree,
        COMPRESSION_ID,
        FUNDING_LAMPORTS,
        vec![0; TREE_ACCOUNT_SIZE],
    ));

    // tree_authority (a.k.a tree_config) PDA = [merkle_tree] under bubblegum.
    let (tree_config, _) = Pubkey::find_program_address(&[merkle_tree.as_ref()], &BUBBLEGUM_ID);

    // create_tree_config(max_depth, max_buffer_size, public=None)
    let create_tree_instruction = Instruction {
        program_id: BUBBLEGUM_ID,
        accounts: vec![
            AccountMeta::new(tree_config, false),
            AccountMeta::new(merkle_tree, false),
            AccountMeta::new(PAYER, true),
            AccountMeta::new_readonly(PAYER, true), // tree_creator
            AccountMeta::new_readonly(NOOP_ID, false),
            AccountMeta::new_readonly(COMPRESSION_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: {
            let mut d = CREATE_TREE_CONFIG_DISC.to_vec();
            d.extend_from_slice(&MAX_DEPTH.to_le_bytes());
            d.extend_from_slice(&MAX_BUFFER_SIZE.to_le_bytes());
            d.push(0); // Option<bool>::None
            d
        },
    };
    test.send(create_tree_instruction).succeeds();

    // Build the MetadataArgs for the single cNFT we mint. The leaf owner /
    // delegate are the vault PDA, so the vault holds the cNFT.
    let creator = Creator {
        address: PAYER.to_bytes(),
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
        creators: vec![creator],
    };

    // mint_v1 - leaf_owner and leaf_delegate are the vault PDA.
    let mint_instruction = Instruction {
        program_id: BUBBLEGUM_ID,
        accounts: vec![
            AccountMeta::new(tree_config, false),
            AccountMeta::new_readonly(vault, false),
            AccountMeta::new_readonly(vault, false), // leaf_delegate
            AccountMeta::new(merkle_tree, false),
            AccountMeta::new_readonly(PAYER, true),
            AccountMeta::new_readonly(PAYER, true), // tree_creator_or_delegate
            AccountMeta::new_readonly(NOOP_ID, false),
            AccountMeta::new_readonly(COMPRESSION_ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: {
            let mut d = MINT_V1_DISC.to_vec();
            d.extend_from_slice(&borsh::to_vec(&metadata).unwrap());
            d
        },
    };
    test.send(mint_instruction).succeeds();

    // Recompute data_hash and creator_hash exactly as Bubblegum does.
    let data_hash = hash_metadata(&metadata);
    let creator_hash = hash_creators(&metadata.creators);

    // Proof for leaf index 0 in an otherwise-empty tree: empty-node siblings.
    let proof = [empty_node(0), empty_node(1), empty_node(2)];

    // Read the current root from the onchain tree account.
    let tree_data = test.account(merkle_tree).unwrap().data;
    let root = read_current_root(&tree_data);

    TreeWithVaultCnft {
        merkle_tree,
        tree_config,
        root,
        data_hash,
        creator_hash,
        proof,
    }
}

// ---- Instruction builders for the program under test ------------------------

/// Proof-node addresses enter the transaction as readonly metas; the runtime
/// materializes the missing accounts as empty system accounts.
fn proof_metas(nodes: &[[u8; 32]]) -> Vec<AccountMeta> {
    nodes
        .iter()
        .map(|node| AccountMeta::new_readonly(Pubkey::new_from_array(*node), false))
        .collect()
}

fn build_withdraw_cnft_instruction(
    signer: Pubkey,
    tree: &TreeWithVaultCnft,
    recipient: Pubkey,
) -> Instruction {
    // The vault PDA and system program are canonical derivations, so the
    // generated instruction omits them.
    // Leaf 0 in a fresh tree has nonce 0 and index 0. The Transfer args are
    // typed instruction arguments in 0.1.0 (`ctx.data` no longer carries a
    // raw tail); only the proof stays dynamic, as remaining accounts.
    WithdrawCnftInstruction {
        authority: signer,
        tree_authority: tree.tree_config,
        new_leaf_owner: recipient,
        merkle_tree: tree.merkle_tree,
        log_wrapper: NOOP_ID,
        compression_program: COMPRESSION_ID,
        bubblegum_program: BUBBLEGUM_ID,
        root: tree.root,
        data_hash: tree.data_hash,
        creator_hash: tree.creator_hash,
        nonce: 0,
        index: 0,
        remaining_accounts: proof_metas(&tree.proof),
    }
    .into()
}

fn build_withdraw_two_cnfts_instruction(
    signer: Pubkey,
    tree1: &TreeWithVaultCnft,
    tree2: &TreeWithVaultCnft,
    recipient: Pubkey,
    proof_1_length: u8,
    proof_2_length: u8,
) -> Instruction {
    let mut remaining_accounts = proof_metas(&tree1.proof);
    remaining_accounts.extend(proof_metas(&tree2.proof));
    WithdrawTwoCnftsInstruction {
        authority: signer,
        tree_authority1: tree1.tree_config,
        new_leaf_owner1: recipient,
        merkle_tree1: tree1.merkle_tree,
        tree_authority2: tree2.tree_config,
        new_leaf_owner2: recipient,
        merkle_tree2: tree2.merkle_tree,
        log_wrapper: NOOP_ID,
        compression_program: COMPRESSION_ID,
        bubblegum_program: BUBBLEGUM_ID,
        root1: tree1.root,
        data_hash1: tree1.data_hash,
        creator_hash1: tree1.creator_hash,
        nonce1: 0,
        index1: 0,
        proof_1_length,
        root2: tree2.root,
        data_hash2: tree2.data_hash,
        creator_hash2: tree2.creator_hash,
        nonce2: 0,
        index2: 0,
        proof_2_length,
        remaining_accounts,
    }
    .into()
}

// ---- Tests ------------------------------------------------------------------

#[quasar_test]
fn initialize_vault_stores_authority(test: &mut Test) {
    let vault = setup_vault(test);

    let (_, expected_bump) = test.derive_pda_with_bump(Vault::seeds());
    let state = test.read::<Vault>(vault);
    assert_eq!(state.authority, AUTHORITY);
    assert_eq!(state.bump, expected_bump);
}

#[quasar_test]
fn withdraw_cnft_by_authority_succeeds_and_replay_fails(test: &mut Test) {
    let vault = setup_vault(test);
    let tree = create_tree_with_vault_cnft(test, vault, MERKLE_TREE_1);

    let instruction = build_withdraw_cnft_instruction(AUTHORITY, &tree, RECIPIENT);

    // The stored authority signs, so the withdraw succeeds (the vault PDA
    // signs the Bubblegum CPI via invoke_signed inside the program).
    test.send(instruction.clone()).succeeds();

    // After transfer, leaf 0's owner changed (vault -> recipient), so the root
    // moved. A second withdraw replaying the same (root, hashes) must fail: the
    // cached root is stale and the leaf no longer hashes to it for the vault.
    assert!(
        test.send(instruction).is_err(),
        "second withdraw must fail: leaf already transferred out of the vault"
    );
}

#[quasar_test]
fn withdraw_cnft_rejected_for_non_authority(test: &mut Test) {
    let vault = setup_vault(test);
    let tree = create_tree_with_vault_cnft(test, vault, MERKLE_TREE_1);

    // An attacker signs their own withdraw attempt; the vault's stored
    // authority did not sign.
    test.add(Wallet::new().at(ATTACKER));
    let instruction = build_withdraw_cnft_instruction(ATTACKER, &tree, RECIPIENT);

    test.send(instruction)
        .fails_with(VaultError::InvalidWithdrawAuthority);
}

#[quasar_test]
fn withdraw_two_cnfts_by_authority_succeeds_and_replay_fails(test: &mut Test) {
    let vault = setup_vault(test);
    let tree1 = create_tree_with_vault_cnft(test, vault, MERKLE_TREE_1);
    let tree2 = create_tree_with_vault_cnft(test, vault, MERKLE_TREE_2);

    let instruction = build_withdraw_two_cnfts_instruction(
        AUTHORITY,
        &tree1,
        &tree2,
        RECIPIENT,
        MAX_DEPTH as u8,
        MAX_DEPTH as u8,
    );

    test.send(instruction).succeeds();

    // Both trees' roots moved, so both cNFTs left the vault: replaying the
    // single-tree withdraw against tree1 with the cached root fails.
    let replay_instruction = build_withdraw_cnft_instruction(AUTHORITY, &tree1, RECIPIENT);
    assert!(
        test.send(replay_instruction).is_err(),
        "cNFT#1 already left the vault, replay must fail"
    );
}

#[quasar_test]
fn withdraw_two_cnfts_rejects_out_of_range_proof_length(test: &mut Test) {
    let vault = setup_vault(test);
    let tree1 = create_tree_with_vault_cnft(test, vault, MERKLE_TREE_1);
    let tree2 = create_tree_with_vault_cnft(test, vault, MERKLE_TREE_2);

    // Claim one more proof node for tree1 than the instruction supplies in
    // total: the bounds check must return ProofLengthMismatch instead of
    // splitting past the end of the supplied proof accounts.
    let supplied_proof_nodes = 2 * MAX_DEPTH as u8;
    let out_of_range_proof_1_length = supplied_proof_nodes + 1;

    let instruction = build_withdraw_two_cnfts_instruction(
        AUTHORITY,
        &tree1,
        &tree2,
        RECIPIENT,
        out_of_range_proof_1_length,
        0,
    );

    test.send(instruction)
        .fails_with(VaultError::ProofLengthMismatch);
}

#[quasar_test]
fn withdraw_two_cnfts_rejects_inconsistent_proof_lengths(test: &mut Test) {
    let vault = setup_vault(test);
    let tree1 = create_tree_with_vault_cnft(test, vault, MERKLE_TREE_1);
    let tree2 = create_tree_with_vault_cnft(test, vault, MERKLE_TREE_2);

    // proof_1_length is in range but the two lengths do not add up to the
    // supplied proof accounts, so the split would misattribute proof nodes.
    let instruction = build_withdraw_two_cnfts_instruction(
        AUTHORITY,
        &tree1,
        &tree2,
        RECIPIENT,
        MAX_DEPTH as u8 - 1,
        MAX_DEPTH as u8,
    );

    test.send(instruction)
        .fails_with(VaultError::ProofLengthMismatch);
}
