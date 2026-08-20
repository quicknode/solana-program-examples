use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{handle_extra_account_metas, handle_extra_account_metas_count, WhiteList};

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
    pub system_program: Program<System>,
    #[account(init_if_needed, seeds = [b"white_list"], bump, payer = payer, space = WhiteList::DISCRIMINATOR.len() + WhiteList::INIT_SPACE)]
    pub white_list: BorshAccount<WhiteList>,
}

pub fn handler(
    mut context: &mut Context<InitializeExtraAccountMetaListAccountConstraints>,
) -> Result<()> {
    // set authority field on white_list account as payer address
    context.accounts.white_list.authority = *context.accounts.payer.address();
    context.accounts.white_list.bump = context.bumps.white_list;

    let extra_account_metas = handle_extra_account_metas()?;

    // initialize ExtraAccountMetaList account with extra accounts
    // .map_err() needed because spl-tlv-account-resolution uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    // `AccountView` is Copy, and a copy still points at the same backing
    // buffer, so the borrow writes through to the real account.
    let mut meta_list_view = *context.accounts.extra_account_meta_list.account();
    let mut meta_list_data = meta_list_view.try_borrow_mut()?;
    ExtraAccountMetaList::init::<ExecuteInstruction>(&mut meta_list_data, &extra_account_metas)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(())
}
