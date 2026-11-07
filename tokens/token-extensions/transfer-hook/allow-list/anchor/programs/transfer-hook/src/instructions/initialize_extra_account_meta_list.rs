use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::token_interface::Mint;
use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{
    handle_extra_account_metas, handle_extra_account_metas_count, AllowList, ALLOW_LIST_SEED,
};

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaListAccountConstraints {
    /// The mint's mint authority, who becomes the allow list's authority. Only
    /// the issuer can set up a mint's allow list, so nobody can claim it first.
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
    #[account(mint::authority = payer)]
    pub mint: InterfaceAccount<Mint>,
    #[account(
        init,
        seeds = [ALLOW_LIST_SEED, mint.address().as_ref()],
        bump,
        payer = payer,
        space = AllowList::DISCRIMINATOR.len() + AllowList::INIT_SPACE
    )]
    pub allow_list: BorshAccount<AllowList>,
    pub system_program: Program<System>,
}

pub fn handler(
    context: &mut Context<InitializeExtraAccountMetaListAccountConstraints>,
) -> Result<()> {
    context.accounts.allow_list.authority = *context.accounts.payer.address();
    context.accounts.allow_list.bump = context.bumps.allow_list;

    let extra_account_metas = handle_extra_account_metas()?;

    // initialize ExtraAccountMetaList account with extra accounts
    // .map_err() needed because spl-tlv-account-resolution uses solana-program-error 2.x
    // while anchor-lang v2 uses 3.x - structurally identical but different semver types
    // `AccountView` is Copy, and a copy still points at the same backing
    // buffer, so the borrow writes through to the real account.
    let mut meta_list_view = *context.accounts.extra_account_meta_list.account();
    let mut meta_list_data = meta_list_view.try_borrow_mut()?;
    ExtraAccountMetaList::init::<ExecuteInstruction>(&mut meta_list_data, &extra_account_metas)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(())
}
