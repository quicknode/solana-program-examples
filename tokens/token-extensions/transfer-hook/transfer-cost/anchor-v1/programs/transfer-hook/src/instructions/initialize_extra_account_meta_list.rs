use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{handle_extra_account_metas, handle_extra_account_metas_count, CounterAccount};

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaListAccountConstraints<'info> {
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
    #[account(init, seeds = [b"counter"], bump, payer = payer, space = CounterAccount::DISCRIMINATOR.len() + CounterAccount::INIT_SPACE)]
    pub counter_account: Account<'info, CounterAccount>,
    pub system_program: Program<'info, System>,
}

pub fn handler(mut context: Context<InitializeExtraAccountMetaListAccountConstraints>) -> Result<()> {
    let extra_account_metas = handle_extra_account_metas()?;

    // initialize ExtraAccountMetaList account with extra accounts
    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut context.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &extra_account_metas,
    )
    .map_err(|_| ProgramError::InvalidAccountData)?;

    context.accounts.counter_account.bump = context.bumps.counter_account;

    Ok(())
}
