//! `mpl-token-metadata` 5.x is built against an older `solana-program`, so its
//! instruction builders return that crate's `Instruction`/`Pubkey` types. These
//! helpers bridge them to the `solana-program` version this program is compiled
//! with. (Both `Pubkey`s are 32-byte arrays, so the conversion is a byte copy.)
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

pub type MplPubkey = mpl_solana_program::pubkey::Pubkey;

pub fn to_mpl(key: &Pubkey) -> MplPubkey {
    MplPubkey::new_from_array(key.to_bytes())
}

pub fn bridge_instruction(ix: mpl_solana_program::instruction::Instruction) -> Instruction {
    Instruction {
        program_id: Pubkey::new_from_array(ix.program_id.to_bytes()),
        accounts: ix
            .accounts
            .into_iter()
            .map(|meta| AccountMeta {
                pubkey: Pubkey::new_from_array(meta.pubkey.to_bytes()),
                is_signer: meta.is_signer,
                is_writable: meta.is_writable,
            })
            .collect(),
        data: ix.data,
    }
}
