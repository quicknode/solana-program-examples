use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("4evptdGtALCNT8uTxJhbWBRZpBE8w5oNtmgfSyfQu7td");

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

    pub fn harvest<'info>(
        context: &mut Context<'info, HarvestAccountConstraints<'info>>,
    ) -> Result<()> {
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
