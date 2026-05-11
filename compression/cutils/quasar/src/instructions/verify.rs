use crate::bubblegum_types::{get_asset_id, leaf_schema_v1_hash};
use crate::*;
use quasar_lang::{cpi::{InstructionAccount, InstructionView}, remaining::RemainingAccounts};

/// Maximum proof nodes for the merkle tree.
const MAX_PROOF_NODES: usize = 24;

/// 1 fixed account (merkle_tree) + proof nodes.
const MAX_CPI_ACCOUNTS: usize = 1 + MAX_PROOF_NODES;

/// spl-account-compression verify_leaf discriminator: sha256("global:verify_leaf")[..8]
const VERIFY_LEAF_DISCRIMINATOR: [u8; 8] = [0x7c, 0xdc, 0x16, 0xdf, 0x68, 0x0a, 0xfa, 0xe0];

/// Accounts for verifying a compressed NFT leaf in the merkle tree.
#[derive(Accounts)]
pub struct Verify {
    pub leaf_owner: Signer,
    /// Leaf delegate.
    pub leaf_delegate: UncheckedAccount,
    /// Merkle tree to verify against.
    pub merkle_tree: UncheckedAccount,
    /// SPL Account Compression program.
    #[account(address = SPL_ACCOUNT_COMPRESSION_ID)]
    pub compression_program: UncheckedAccount,
}

pub fn handle_verify(accounts: &mut Verify, data: &[u8], remaining: RemainingAccounts<'_>) -> Result<(), ProgramError> {
    // Parse verify params from instruction data:
    // root(32) + data_hash(32) + creator_hash(32) + nonce(8) + index(4) = 108 bytes
    if data.len() < 108 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let root: [u8; 32] = data[0..32].try_into().unwrap();
    let data_hash: [u8; 32] = data[32..64].try_into().unwrap();
    let creator_hash: [u8; 32] = data[64..96].try_into().unwrap();
    let nonce = u64::from_le_bytes(data[96..104].try_into().unwrap());
    let index = u32::from_le_bytes(data[104..108].try_into().unwrap());

    // Compute asset ID and leaf hash
    let asset_id = get_asset_id(accounts.merkle_tree.address(), nonce);
    let leaf_hash = leaf_schema_v1_hash(
        &asset_id,
        accounts.leaf_owner.address(),
        accounts.leaf_delegate.address(),
        nonce,
        &data_hash,
        &creator_hash,
    );

    // Build verify_leaf instruction data: discriminator(8) + root(32) + leaf(32) + index(4) = 76
    let mut ix_data = [0u8; 76];
    ix_data[0..8].copy_from_slice(&VERIFY_LEAF_DISCRIMINATOR);
    ix_data[8..40].copy_from_slice(&root);
    ix_data[40..72].copy_from_slice(&leaf_hash);
    ix_data[72..76].copy_from_slice(&index.to_le_bytes());

    // Collect proof nodes.
    //
    // `remaining.iter()` yields `Result<RemainingAccount, _>` in newer
    // quasar-lang. Reach the inner `AccountView` via the unchecked accessor
    // — we only read addresses/views to forward to the compression CPI as
    // proof nodes; no aliased data access.
    let placeholder = accounts.compression_program.to_account_view().clone();
    let mut proof_views: [AccountView; MAX_PROOF_NODES] =
        core::array::from_fn(|_| placeholder.clone());
    let mut proof_count = 0usize;
    for result in remaining.iter() {
        if proof_count >= MAX_PROOF_NODES {
            break;
        }
        let account = result?;
        // SAFETY: Only reads address and forwards an immutable view to CPI.
        proof_views[proof_count] = unsafe { account.as_account_view_unchecked() }.clone();
        proof_count += 1;
    }

    let total_accounts = 1 + proof_count;

    // Build instruction accounts: merkle_tree + proof nodes
    let tree_addr = accounts.merkle_tree.address();
    let mut ix_accounts: [InstructionAccount; MAX_CPI_ACCOUNTS] =
        core::array::from_fn(|_| InstructionAccount::readonly(tree_addr));

    ix_accounts[0] = InstructionAccount::readonly(accounts.merkle_tree.address());
    for i in 0..proof_count {
        ix_accounts[1 + i] = InstructionAccount::readonly(proof_views[i].address());
    }

    // Build account views
    let tree_view = accounts.merkle_tree.to_account_view().clone();
    let mut views: [AccountView; MAX_CPI_ACCOUNTS] =
        core::array::from_fn(|_| tree_view.clone());

    views[0] = accounts.merkle_tree.to_account_view().clone();
    for i in 0..proof_count {
        views[1 + i] = proof_views[i].clone();
    }

    let instruction = InstructionView {
        program_id: accounts.compression_program.address(),
        data: &ix_data,
        accounts: &ix_accounts[..total_accounts],
    };

    solana_instruction_view::cpi::invoke_with_bounds::<MAX_CPI_ACCOUNTS, AccountView>(
        &instruction,
        &views[..total_accounts],
    )
}
