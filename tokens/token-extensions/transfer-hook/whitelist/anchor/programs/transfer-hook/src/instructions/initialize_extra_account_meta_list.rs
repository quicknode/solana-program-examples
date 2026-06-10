use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{handle_extra_account_metas, handle_extra_account_metas_count, WhiteList};

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
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
    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
    #[account(init_if_needed, seeds = [b"white_list"], bump, payer = payer, space = WhiteList::DISCRIMINATOR.len() + WhiteList::INIT_SPACE)]
    pub white_list: Account<'info, WhiteList>,
}

pub fn handler(mut context: Context<InitializeExtraAccountMetaList>) -> Result<()> {
    // set authority field on white_list account as payer address
    context.accounts.white_list.authority = context.accounts.payer.key();
    context.accounts.white_list.bump = context.bumps.white_list;

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
