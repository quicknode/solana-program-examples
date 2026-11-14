use anchor_lang::{
    cpi::InstructionAccount, prelude::Address, solana_program::instruction::AccountMeta,
    CpiInstruction,
};

/// An instruction for a program this crate has no CPI client for, with its
/// account metas and data held in `Vec`s.
///
/// Anchor v2's `invoke` and `invoke_signed` take any `CpiInstruction`. Onchain,
/// Anchor provides that only for pinocchio's borrowed `InstructionView`, so
/// this type builds the `InstructionAccount` list the CPI needs from its own
/// metas when it is invoked.
pub struct OwnedInstruction {
    pub program_id: Address,
    pub accounts: Vec<AccountMeta>,
    pub data: Vec<u8>,
}

impl CpiInstruction for OwnedInstruction {
    fn program_id(&self) -> &Address {
        &self.program_id
    }

    fn data(&self) -> &[u8] {
        &self.data
    }

    fn with_instruction_accounts<R>(
        &self,
        visit: impl FnOnce(&[InstructionAccount<'_>]) -> R,
    ) -> R {
        let instruction_accounts: Vec<InstructionAccount<'_>> = self
            .accounts
            .iter()
            .map(|meta| InstructionAccount::new(&meta.pubkey, meta.is_writable, meta.is_signer))
            .collect();
        visit(&instruction_accounts)
    }
}
