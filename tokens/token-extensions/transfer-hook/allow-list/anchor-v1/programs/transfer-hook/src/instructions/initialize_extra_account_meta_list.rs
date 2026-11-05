use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{
    handle_extra_account_metas, handle_extra_account_metas_count, AllowList, ALLOW_LIST_SEED,
};

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaListAccountConstraints<'info> {
    /// The mint's mint authority, who becomes the allow list's authority. Only
    /// the issuer can set up a mint's allow list, so nobody can claim it first.
    #[account(mut)]
    payer: Signer<'info>,

    /// CHECK: ExtraAccountMetaList Account, must use these seeds
    #[account(
        init,
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump,
        // size_of returns Result with spl's ProgramError - unwrap is safe for known-good input
        space = ExtraAccountMetaList::size_of(
            handle_extra_account_metas_count()
        ).unwrap(),
        payer = payer
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,
    #[account(mint::authority = payer)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        seeds = [ALLOW_LIST_SEED, mint.key().as_ref()],
        bump,
        payer = payer,
        space = AllowList::DISCRIMINATOR.len() + AllowList::INIT_SPACE
    )]
    pub allow_list: Account<'info, AllowList>,
    pub system_program: Program<'info, System>,
}

pub fn handler(context: Context<InitializeExtraAccountMetaListAccountConstraints>) -> Result<()> {
    context.accounts.allow_list.authority = context.accounts.payer.key();
    context.accounts.allow_list.bump = context.bumps.allow_list;

    let extra_account_metas = handle_extra_account_metas()?;

    // initialize ExtraAccountMetaList account with extra accounts
    // .map_err() needed because spl-tlv-account-resolution uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut context
            .accounts
            .extra_account_meta_list
            .try_borrow_mut_data()?,
        &extra_account_metas,
    )
    .map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(())
}
