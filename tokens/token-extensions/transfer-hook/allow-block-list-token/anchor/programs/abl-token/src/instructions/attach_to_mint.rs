use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::{
    token_2022::Token2022,
    token_interface::{transfer_hook_update, Mint, TransferHookUpdate},
};

use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{get_extra_account_metas, get_meta_list_size, META_LIST_ACCOUNT_SEED};

#[derive(Accounts)]
pub struct AttachToMintAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        mut,
        mint::token_program = token_program,
    )]
    pub mint: Box<InterfaceAccount<Mint>>,

    #[account(
        init,
        space = get_meta_list_size()?,
        seeds = [META_LIST_ACCOUNT_SEED, mint.address().as_ref()],
        bump,
        payer = payer,
    )]
    /// CHECK: extra metas account
    pub extra_metas_account: UncheckedAccount,

    pub system_program: Program<System>,

    pub token_program: Program<Token2022>,
}

impl AttachToMintAccountConstraints<'_> {
    pub fn attach_to_mint(&mut self) -> Result<()> {
        let tx_hook_accs = TransferHookUpdate {
            mint: self.mint.cpi_handle_mut(),
            authority: self.payer.cpi_handle_mut(),
        };

        let context = CpiContext::new(self.token_program.address(), tx_hook_accs);

        transfer_hook_update(context, Some(crate::ID_CONST))?;

        // initialize the extra metas account
        let extra_metas_account = &self.extra_metas_account;
        let metas = get_extra_account_metas()?;
        let mut data = extra_metas_account.try_borrow_mut_data()?;
        ExtraAccountMetaList::init::<ExecuteInstruction>(&mut data, &metas)
            .map_err(|_| ProgramError::InvalidAccountData)?;

        Ok(())
    }
}
