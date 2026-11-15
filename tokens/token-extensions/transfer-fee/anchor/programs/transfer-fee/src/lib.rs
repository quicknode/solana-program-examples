use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("4evptdGtALCNT8uTxJhbWBRZpBE8w5oNtmgfSyfQu7td");

/// The most token accounts one `harvest` instruction takes fees from. Anchor
/// v2 builds each CPI's account list in a fixed-capacity buffer, sized by the
/// `CpiContext` in `harvest` to the mint plus this many token accounts.
pub const MAX_HARVEST_SOURCES: usize = 16;

#[error_code]
pub enum TransferFeeError {
    #[msg("A harvest passed more than MAX_HARVEST_SOURCES token accounts")]
    TooManyHarvestSources,
}

#[program]
pub mod transfer_fee {
    use super::*;

    pub fn initialize(
        context: &mut Context<InitializeAccountConstraints>,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
    ) -> Result<()> {
        handle_process_initialize(context, transfer_fee_basis_points, maximum_fee)
    }

    pub fn transfer(context: &mut Context<TransferAccountConstraints>, amount: u64) -> Result<()> {
        handle_process_transfer(context, amount)
    }

    pub fn harvest(context: &mut Context<HarvestAccountConstraints>) -> Result<()> {
        process_harvest(context)
    }

    pub fn withdraw(context: &mut Context<WithdrawAccountConstraints>) -> Result<()> {
        handle_process_withdraw(context)
    }

    pub fn update_fee(
        context: &mut Context<UpdateFeeAccountConstraints>,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
    ) -> Result<()> {
        handle_process_update_fee(context, transfer_fee_basis_points, maximum_fee)
    }
}
