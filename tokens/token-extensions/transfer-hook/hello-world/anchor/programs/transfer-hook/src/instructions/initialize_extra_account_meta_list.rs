use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, Token2022},
};
use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{handle_extra_account_metas, handle_extra_account_metas_count};

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaListAccountConstraints {
    #[account(mut)]
    payer: Signer,

    /// CHECK: ExtraAccountMetaList Account, must use these seeds
    #[account(
        init,
        seeds = [b"extra-account-metas", mint.address().as_ref()],
        bump,
        // size_of returns Result with spl's ProgramError - unwrap is safe for known-good input
        space = ExtraAccountMetaList::size_of(
            handle_extra_account_metas_count()
        ).unwrap(),
        payer = payer
    )]
    pub extra_account_meta_list: UncheckedAccount,
    pub mint: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}

pub fn handler(mut context: &mut Context<InitializeExtraAccountMetaListAccountConstraints>) -> Result<()> {
    let extra_account_metas = handle_extra_account_metas()?;

    // initialize ExtraAccountMetaList account with extra accounts
    // .map_err() needed because spl-tlv-account-resolution uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut context.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &extra_account_metas,
    ).map_err(|_| ProgramError::InvalidAccountData)?;

    Ok(())
}
