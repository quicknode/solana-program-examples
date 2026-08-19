use crate::error::VaultError;
use crate::state::Vault;
use crate::*;
use quasar_lang::{
    cpi::{InstructionAccount, InstructionView, Seed, Signer as CpiSigner},
    remaining::RemainingAccounts,
};

/// Maximum proof nodes for the merkle tree.
const MAX_PROOF_NODES: usize = 24;

/// 8 fixed accounts + proof nodes.
const MAX_CPI_ACCOUNTS: usize = 8 + MAX_PROOF_NODES;

/// Transfer args byte length: root(32) + data_hash(32) + creator_hash(32) + nonce(8) + index(4).
pub(crate) const TRANSFER_ARGS_LEN: usize = 108;

/// Bubblegum Transfer arguments, received as typed instruction args.
pub struct TransferArgs {
    pub root: [u8; 32],
    pub data_hash: [u8; 32],
    pub creator_hash: [u8; 32],
    pub nonce: u64,
    pub index: u32,
}

impl TransferArgs {
    /// Serialize into the Bubblegum Transfer wire layout.
    pub(crate) fn to_bytes(&self) -> [u8; TRANSFER_ARGS_LEN] {
        let mut bytes = [0u8; TRANSFER_ARGS_LEN];
        bytes[0..32].copy_from_slice(&self.root);
        bytes[32..64].copy_from_slice(&self.data_hash);
        bytes[64..96].copy_from_slice(&self.creator_hash);
        bytes[96..104].copy_from_slice(&self.nonce.to_le_bytes());
        bytes[104..108].copy_from_slice(&self.index.to_le_bytes());
        bytes
    }
}

/// Accounts for withdrawing a single compressed NFT from the vault.
#[derive(Accounts)]
pub struct WithdrawCnftAccountConstraints {
    /// The stored vault authority. Only this signer may withdraw.
    pub authority: Signer,

    /// Vault PDA that owns the cNFT (as Bubblegum leaf owner) and signs the
    /// transfer via invoke_signed.
    #[account(
        address = Vault::seeds(),
        has_one(authority) @ VaultError::InvalidWithdrawAuthority,
    )]
    pub vault: Account<Vault>,

    /// Tree authority PDA (seeds checked by Bubblegum).
    #[account(mut)]
    pub tree_authority: UncheckedAccount,
    /// New owner to receive the cNFT.
    pub new_leaf_owner: UncheckedAccount,
    /// Merkle tree account.
    #[account(mut)]
    pub merkle_tree: UncheckedAccount,
    /// SPL Noop log wrapper.
    pub log_wrapper: UncheckedAccount,
    /// SPL Account Compression program.
    #[account(address = SPL_ACCOUNT_COMPRESSION_ID)]
    pub compression_program: UncheckedAccount,
    /// mpl-bubblegum program.
    #[account(address = MPL_BUBBLEGUM_ID)]
    pub bubblegum_program: UncheckedAccount,
    pub system_program: Program<SystemProgram>,
}

/// Build mpl-bubblegum Transfer instruction data from raw args.
fn build_transfer_data(args: &[u8]) -> [u8; 8 + TRANSFER_ARGS_LEN] {
    let mut ix_data = [0u8; 8 + TRANSFER_ARGS_LEN];
    ix_data[0..8].copy_from_slice(&TRANSFER_DISCRIMINATOR);
    ix_data[8..].copy_from_slice(args);
    ix_data
}

#[allow(clippy::too_many_arguments)]
pub fn handle_withdraw_cnft(
    accounts: &mut WithdrawCnftAccountConstraints,
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
    index: u32,
    remaining: RemainingAccounts<'_>,
    vault_bump: u8,
) -> Result<(), ProgramError> {
    let args = TransferArgs {
        root,
        data_hash,
        creator_hash,
        nonce,
        index,
    };
    let ix_data = build_transfer_data(&args.to_bytes());

    // Collect proof nodes.
    //
    // `remaining.iter()` yields `Result<RemainingAccount, _>` in newer
    // quasar-lang. Reach the inner `AccountView` via the unchecked accessor
    // - we only read addresses/views to forward to the bubblegum CPI as
    // proof nodes; no aliased data access.
    let placeholder = accounts.system_program.to_account_view().clone();
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

    let total_accounts = 8 + proof_count;

    // Build instruction account metas matching mpl-bubblegum Transfer layout:
    // tree_config, leaf_owner (signer/PDA), leaf_delegate, new_leaf_owner,
    // merkle_tree, log_wrapper, compression_program, system_program, then proofs.
    let sys_addr = accounts.system_program.address();
    let mut ix_accounts: [InstructionAccount; MAX_CPI_ACCOUNTS] =
        core::array::from_fn(|_| InstructionAccount::readonly(sys_addr));

    ix_accounts[0] = InstructionAccount::readonly(accounts.tree_authority.address());
    ix_accounts[1] = InstructionAccount::readonly_signer(accounts.vault.address());
    // leaf_delegate = leaf_owner (the vault), not an additional signer
    ix_accounts[2] = InstructionAccount::readonly(accounts.vault.address());
    ix_accounts[3] = InstructionAccount::readonly(accounts.new_leaf_owner.address());
    ix_accounts[4] = InstructionAccount::writable(accounts.merkle_tree.address());
    ix_accounts[5] = InstructionAccount::readonly(accounts.log_wrapper.address());
    ix_accounts[6] = InstructionAccount::readonly(accounts.compression_program.address());
    ix_accounts[7] = InstructionAccount::readonly(accounts.system_program.address());

    for i in 0..proof_count {
        ix_accounts[8 + i] = InstructionAccount::readonly(proof_views[i].address());
    }

    // Build account views
    let sys_view = accounts.system_program.to_account_view().clone();
    let mut views: [AccountView; MAX_CPI_ACCOUNTS] = core::array::from_fn(|_| sys_view.clone());

    views[0] = accounts.tree_authority.to_account_view().clone();
    views[1] = accounts.vault.to_account_view().clone();
    views[2] = accounts.vault.to_account_view().clone();
    views[3] = accounts.new_leaf_owner.to_account_view().clone();
    views[4] = accounts.merkle_tree.to_account_view().clone();
    views[5] = accounts.log_wrapper.to_account_view().clone();
    views[6] = accounts.compression_program.to_account_view().clone();
    views[7] = accounts.system_program.to_account_view().clone();

    views[8..8 + proof_count].clone_from_slice(&proof_views[..proof_count]);

    let instruction = InstructionView {
        program_id: &MPL_BUBBLEGUM_ID,
        data: &ix_data,
        accounts: &ix_accounts[..total_accounts],
    };

    // PDA signer seeds: ["cNFT-vault", bump]
    let bump_bytes = [vault_bump];
    let seeds: [Seed; 2] = [
        Seed::from(b"cNFT-vault" as &[u8]),
        Seed::from(&bump_bytes as &[u8]),
    ];
    let signer = CpiSigner::from(&seeds as &[Seed]);

    solana_instruction_view::cpi::invoke_signed_with_bounds::<MAX_CPI_ACCOUNTS, AccountView>(
        &instruction,
        &views[..total_accounts],
        &[signer],
    )
}
