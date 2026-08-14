mod error;
mod instructions;
mod state;

use anchor_lang::prelude::*;
use instructions::*;
use spl_discriminator::SplDiscriminate;
use spl_transfer_hook_interface::instruction::{
    ExecuteInstruction, InitializeExtraAccountMetaListInstruction,
};

declare_id!("FjcHckEgXcBhFmSGai3FRpDLiT6hbpV893n8iTxVd81g");

#[program(interface, program_id = ID)]
pub mod transfer_switch {
    use super::*;

    pub fn configure_admin(
        mut context: &mut Context<ConfigureAdminAccountConstraints>,
    ) -> Result<()> {
        let bump = context.bumps.admin_config;
        handle_is_admin(&mut context.accounts)?;
        handle_configure_admin(&mut context.accounts, bump)
    }

    // sha256("spl-transfer-hook-interface:initialize-extra-account-metas")[..8]
    #[discrim = [43, 34, 13, 49, 167, 88, 235, 235]]
    pub fn initialize_extra_account_metas_list(
        mut context: &mut Context<InitializeExtraAccountMetasAccountConstraints>,
    ) -> Result<()> {
        handle_initialize_extra_account_metas_list(&mut context.accounts, context.bumps)
    }

    pub fn switch(mut context: &mut Context<SwitchAccountConstraints>, on: bool) -> Result<()> {
        let bump = context.bumps.wallet_switch;
        handle_switch(&mut context.accounts, on, bump)
    }

    // sha256("spl-transfer-hook-interface:execute")[..8]
    #[discrim = [105, 37, 101, 197, 75, 251, 102, 26]]
    pub fn transfer_hook(
        mut context: &mut Context<TransferHookAccountConstraints>,
        _amount: u64,
    ) -> Result<()> {
        handle_assert_is_transferring(&mut context.accounts)?;
        handle_assert_switch_is_on(&mut context.accounts)
    }
}
