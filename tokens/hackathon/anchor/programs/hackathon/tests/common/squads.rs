// Hand-rolled builders for the Squads v4 multisig program. We use the onchain
// program (loaded as a `.so` fixture) rather than the `squads-multisig` SDK
// crate because that SDK pulls in Solana 1.17 deps that conflict with our
// Anchor 1.0 / Solana 3.x stack on `zeroize`.
//
// What we build by hand:
// - The Anchor 8-byte instruction discriminator (`sha256("global:<name>")[..8]`).
// - The Borsh wire format for each instruction's arg struct.
// - PDA derivations for `multisig`, `vault`, `transaction`, `proposal`.
// - A forged `ProgramConfig` account injected directly into LiteSVM, so we
//   don't have to run the Squads admin instruction first (which is gated on a
//   key we don't control).
//
// References (Squads v4 main branch):
//   programs/squads_multisig_program/src/instructions/{multisig_create,
//   vault_transaction_create, proposal_create, proposal_vote,
//   vault_transaction_execute}.rs
//   programs/squads_multisig_program/src/state/{multisig,seeds,program_config}.rs

use std::str::FromStr;

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::system_program;
use borsh::{BorshDeserialize, BorshSerialize};
use litesvm::types::FailedTransactionMetadata;
use litesvm::LiteSVM;
use sha2::{Digest, Sha256};
use solana_keypair::Keypair;
use solana_signer::Signer;

// Squads v4 deployed program id on mainnet. Matches the `.so` we dump into
// `tests/fixtures/squads_multisig.so`.
pub fn squads_program_id() -> Pubkey {
    Pubkey::from_str("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf").unwrap()
}

// Squads v4 PDA seed strings, copied verbatim from the upstream
// `state/seeds.rs` so a future Squads-side rename will fail loudly here too.
pub const SEED_PREFIX: &[u8] = b"multisig";
pub const SEED_PROGRAM_CONFIG: &[u8] = b"program_config";
pub const SEED_MULTISIG: &[u8] = b"multisig";
pub const SEED_PROPOSAL: &[u8] = b"proposal";
pub const SEED_TRANSACTION: &[u8] = b"transaction";
pub const SEED_VAULT: &[u8] = b"vault";

// Squads `Permissions` bitmask (Initiate | Vote | Execute = 7). Every committee
// member in our tests holds all three so they can both propose and execute.
pub const PERMISSION_ALL: u8 = 0b0000_0111;

// Build the 8-byte Anchor discriminator: sha256("global:<snake_case>")[..8].
fn anchor_discriminator(name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("global:{}", name).as_bytes());
    let full = hasher.finalize();
    let mut out = [0u8; 8];
    out.copy_from_slice(&full[..8]);
    out
}

// ----- PDA helpers -------------------------------------------------------

pub fn program_config_pda() -> Pubkey {
    Pubkey::find_program_address(&[SEED_PREFIX, SEED_PROGRAM_CONFIG], &squads_program_id()).0
}

pub fn multisig_pda(create_key: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[SEED_PREFIX, SEED_MULTISIG, create_key.as_ref()],
        &squads_program_id(),
    )
}

pub fn vault_pda(multisig: &Pubkey, vault_index: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            SEED_PREFIX,
            multisig.as_ref(),
            SEED_VAULT,
            &vault_index.to_le_bytes(),
        ],
        &squads_program_id(),
    )
}

pub fn transaction_pda(multisig: &Pubkey, transaction_index: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            SEED_PREFIX,
            multisig.as_ref(),
            SEED_TRANSACTION,
            &transaction_index.to_le_bytes(),
        ],
        &squads_program_id(),
    )
}

pub fn proposal_pda(multisig: &Pubkey, transaction_index: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            SEED_PREFIX,
            multisig.as_ref(),
            SEED_TRANSACTION,
            &transaction_index.to_le_bytes(),
            SEED_PROPOSAL,
        ],
        &squads_program_id(),
    )
}

// ----- Forged ProgramConfig --------------------------------------------

// The onchain `multisig_create_v2` instruction reads `program_config` to
// learn the treasury pubkey and the creation fee. On mainnet this is a real
// account written by a Squads admin instruction we cannot run here. Instead
// we build the same byte layout in-process and `set_account` it directly,
// with `multisig_creation_fee = 0` so the transfer-to-treasury branch is a
// no-op.
//
// Layout (from `state/program_config.rs`, Anchor #[account] #[derive(InitSpace)]):
//   8  bytes  Anchor account discriminator
//   32 bytes  authority
//   8  bytes  multisig_creation_fee (u64)
//   32 bytes  treasury
//   64 bytes  _reserved
pub fn forge_program_config(svm: &mut LiteSVM, treasury: &Pubkey) {
    let discriminator = anchor_discriminator_for_account("ProgramConfig");
    let authority = Keypair::new().pubkey();
    let mut data = Vec::with_capacity(8 + 32 + 8 + 32 + 64);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(authority.as_ref());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(treasury.as_ref());
    data.extend_from_slice(&[0u8; 64]);

    let lamports = svm.minimum_balance_for_rent_exemption(data.len());
    let account = solana_account::Account {
        lamports,
        data,
        owner: squads_program_id(),
        executable: false,
        rent_epoch: 0,
    };
    svm.set_account(program_config_pda(), account).unwrap();
}

// Anchor account discriminator: sha256("account:<TypeName>")[..8].
fn anchor_discriminator_for_account(type_name: &str) -> [u8; 8] {
    let mut hasher = Sha256::new();
    hasher.update(format!("account:{}", type_name).as_bytes());
    let full = hasher.finalize();
    let mut out = [0u8; 8];
    out.copy_from_slice(&full[..8]);
    out
}

// ----- Instruction builders --------------------------------------------

// Squads `Member` struct: pubkey + 1-byte permissions mask.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Member {
    pub key: Pubkey,
    pub permissions_mask: u8,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct MultisigCreateArgsV2 {
    config_authority: Option<Pubkey>,
    threshold: u16,
    members: Vec<Member>,
    time_lock: u32,
    rent_collector: Option<Pubkey>,
    memo: Option<String>,
}

pub fn multisig_create_v2_instruction(
    create_key: &Pubkey,
    creator: &Pubkey,
    treasury: &Pubkey,
    members: Vec<Member>,
    threshold: u16,
) -> Instruction {
    let (multisig, _bump) = multisig_pda(create_key);

    let args = MultisigCreateArgsV2 {
        config_authority: None,
        threshold,
        members,
        time_lock: 0,
        rent_collector: None,
        memo: None,
    };

    let mut data = anchor_discriminator("multisig_create_v2").to_vec();
    args.serialize(&mut data).unwrap();

    Instruction {
        program_id: squads_program_id(),
        accounts: vec![
            AccountMeta::new_readonly(program_config_pda(), false),
            AccountMeta::new(*treasury, false),
            AccountMeta::new(multisig, false),
            AccountMeta::new_readonly(*create_key, true),
            AccountMeta::new(*creator, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

// ----- Compiled inner-transaction-message wire format ------------------
//
// `vault_transaction_create` takes an opaque `transaction_message: Vec<u8>`
// which is a Borsh-serialized `TransactionMessage`. The fields use
// `SmallVec<u8, T>` — i.e. a 1-byte length prefix followed by the items
// concatenated. We reproduce that layout exactly.

// `MessageAddressTableLookup` — kept as documentation of the upstream
// `TransactionMessage` schema even though we always serialise an empty list
// of lookups in tests.
#[derive(BorshSerialize)]
struct MessageAddressTableLookup {
    account_key: Pubkey,
    writable_indexes: Vec<u8>, // SmallVec<u8, u8>
    readonly_indexes: Vec<u8>, // SmallVec<u8, u8>
}

pub struct CompiledInstruction {
    pub program_id_index: u8,
    pub account_indexes: Vec<u8>,
    pub data: Vec<u8>,
}

// Hand-encode the upstream `TransactionMessage` layout. We do this manually
// (rather than via derive) because `SmallVec<u8, T>` uses a single-byte length
// prefix where standard Borsh `Vec` uses four; deriving would produce the
// wrong wire format.
pub fn serialize_transaction_message(
    num_signers: u8,
    num_writable_signers: u8,
    num_writable_non_signers: u8,
    account_keys: &[Pubkey],
    instructions: &[CompiledInstruction],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(num_signers);
    out.push(num_writable_signers);
    out.push(num_writable_non_signers);

    // SmallVec<u8, Pubkey>
    out.push(account_keys.len() as u8);
    for key in account_keys {
        out.extend_from_slice(key.as_ref());
    }

    // SmallVec<u8, CompiledInstruction>
    out.push(instructions.len() as u8);
    for ix in instructions {
        out.push(ix.program_id_index);
        // SmallVec<u8, u8>
        out.push(ix.account_indexes.len() as u8);
        out.extend_from_slice(&ix.account_indexes);
        // SmallVec<u16, u8> for data
        let data_len: u16 = ix.data.len().try_into().unwrap();
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&ix.data);
    }

    // SmallVec<u8, MessageAddressTableLookup> — empty in all our tests.
    out.push(0);

    out
}

#[derive(BorshSerialize)]
struct VaultTransactionCreateArgs {
    vault_index: u8,
    ephemeral_signers: u8,
    transaction_message: Vec<u8>,
    memo: Option<String>,
}

pub fn vault_transaction_create_instruction(
    multisig: &Pubkey,
    transaction_index: u64,
    vault_index: u8,
    creator: &Pubkey,
    rent_payer: &Pubkey,
    transaction_message: Vec<u8>,
) -> Instruction {
    let (transaction, _bump) = transaction_pda(multisig, transaction_index);

    let args = VaultTransactionCreateArgs {
        vault_index,
        ephemeral_signers: 0,
        transaction_message,
        memo: None,
    };

    let mut data = anchor_discriminator("vault_transaction_create").to_vec();
    args.serialize(&mut data).unwrap();

    Instruction {
        program_id: squads_program_id(),
        accounts: vec![
            AccountMeta::new(*multisig, false),
            AccountMeta::new(transaction, false),
            AccountMeta::new_readonly(*creator, true),
            AccountMeta::new(*rent_payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

#[derive(BorshSerialize)]
struct ProposalCreateArgs {
    transaction_index: u64,
    draft: bool,
}

pub fn proposal_create_instruction(
    multisig: &Pubkey,
    transaction_index: u64,
    creator: &Pubkey,
    rent_payer: &Pubkey,
) -> Instruction {
    let (proposal, _bump) = proposal_pda(multisig, transaction_index);

    let args = ProposalCreateArgs {
        transaction_index,
        draft: false,
    };

    let mut data = anchor_discriminator("proposal_create").to_vec();
    args.serialize(&mut data).unwrap();

    Instruction {
        program_id: squads_program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*multisig, false),
            AccountMeta::new(proposal, false),
            AccountMeta::new_readonly(*creator, true),
            AccountMeta::new(*rent_payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

#[derive(BorshSerialize)]
struct ProposalVoteArgs {
    memo: Option<String>,
}

pub fn proposal_approve_instruction(
    multisig: &Pubkey,
    transaction_index: u64,
    member: &Pubkey,
) -> Instruction {
    let (proposal, _bump) = proposal_pda(multisig, transaction_index);

    let args = ProposalVoteArgs { memo: None };
    let mut data = anchor_discriminator("proposal_approve").to_vec();
    args.serialize(&mut data).unwrap();

    Instruction {
        program_id: squads_program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*multisig, false),
            AccountMeta::new(*member, true),
            AccountMeta::new(proposal, false),
        ],
        data,
    }
}

// `vault_transaction_execute` takes no instruction args (just the
// discriminator). The accounts vector is:
//   1. multisig
//   2. proposal
//   3. transaction
//   4. member (signer)
//   ...then remaining_accounts in the order: account_keys (we always have
//   zero address-table lookups), with writable/signer bits inferred from
//   the compiled message.
pub fn vault_transaction_execute_instruction(
    multisig: &Pubkey,
    transaction_index: u64,
    member: &Pubkey,
    remaining_accounts: Vec<AccountMeta>,
) -> Instruction {
    let (proposal, _bump) = proposal_pda(multisig, transaction_index);
    let (transaction, _bump) = transaction_pda(multisig, transaction_index);

    let data = anchor_discriminator("vault_transaction_execute").to_vec();

    let mut accounts = vec![
        AccountMeta::new_readonly(*multisig, false),
        AccountMeta::new(proposal, false),
        AccountMeta::new_readonly(transaction, false),
        AccountMeta::new_readonly(*member, true),
    ];
    accounts.extend(remaining_accounts);

    Instruction {
        program_id: squads_program_id(),
        accounts,
        data,
    }
}

// ----- Convenience wrappers --------------------------------------------

pub struct Committee {
    pub alice: Keypair,
    pub bob: Keypair,
    pub carol: Keypair,
    pub create_key: Keypair,
    pub multisig: Pubkey,
    pub vault: Pubkey,
    // Vault PDA bump. We don't use it directly in tests yet, but the Squads
    // program reads it on every execute, and exposing it makes future
    // sign-as-vault helpers trivial to add.
    pub vault_bump: u8,
}

impl Committee {
    pub fn members_sorted(&self) -> Vec<Member> {
        let mut members = vec![
            Member {
                key: self.alice.pubkey(),
                permissions_mask: PERMISSION_ALL,
            },
            Member {
                key: self.bob.pubkey(),
                permissions_mask: PERMISSION_ALL,
            },
            Member {
                key: self.carol.pubkey(),
                permissions_mask: PERMISSION_ALL,
            },
        ];
        members.sort_by_key(|m| m.key);
        members
    }
}

pub fn install_squads_program(svm: &mut LiteSVM) {
    let bytes = include_bytes!("../fixtures/squads_multisig.so");
    svm.add_program(squads_program_id(), bytes).unwrap();
}

// Re-export so test files can compose their own send_transaction error
// reporting.
pub type SvmError = FailedTransactionMetadata;
