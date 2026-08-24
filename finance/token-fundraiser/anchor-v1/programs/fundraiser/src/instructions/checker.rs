use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
        TransferChecked,
    },
};

use crate::{state::Fundraiser, FundraiserError};

#[derive(Accounts)]
pub struct CheckContributionsAccountConstraints<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,

    pub mint_to_raise: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        seeds = [b"fundraiser".as_ref(), maker.key().as_ref()],
        bump = fundraiser.bump,
        close = maker,
    )]
    pub fundraiser: Account<'info, Fundraiser>,

    #[account(
        mut,
        associated_token::mint = mint_to_raise,
        associated_token::authority = fundraiser,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = maker,
        associated_token::mint = mint_to_raise,
        associated_token::authority = maker,
        associated_token::token_program = token_program,
    )]
    pub maker_ata: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,

    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handle_check_contributions(
    accounts: &mut CheckContributionsAccountConstraints,
) -> Result<()> {
    // Compare the state-tracked total, not the vault balance, so tokens
    // donated directly to the vault cannot trigger an early release.
    require!(
        accounts.fundraiser.current_amount >= accounts.fundraiser.amount_to_raise,
        FundraiserError::TargetNotMet
    );

    // The vault is owned by the fundraiser PDA, so both CPIs are signed with
    // its seeds.
    let signer_seeds: [&[&[u8]]; 1] = [&[
        b"fundraiser".as_ref(),
        accounts.maker.to_account_info().key.as_ref(),
        &[accounts.fundraiser.bump],
    ]];

    // Drain the whole vault (including any direct donations) to the maker.
    let transfer_accounts = TransferChecked {
        from: accounts.vault.to_account_info(),
        mint: accounts.mint_to_raise.to_account_info(),
        to: accounts.maker_ata.to_account_info(),
        authority: accounts.fundraiser.to_account_info(),
    };
    let transfer_context = CpiContext::new_with_signer(
        accounts.token_program.key(),
        transfer_accounts,
        &signer_seeds,
    );
    transfer_checked(
        transfer_context,
        accounts.vault.amount,
        accounts.mint_to_raise.decimals,
    )?;

    // Close the empty vault so its rent goes back to the maker.
    let close_accounts = CloseAccount {
        account: accounts.vault.to_account_info(),
        destination: accounts.maker.to_account_info(),
        authority: accounts.fundraiser.to_account_info(),
    };
    let close_context = CpiContext::new_with_signer(
        accounts.token_program.key(),
        close_accounts,
        &signer_seeds,
    );
    close_account(close_context)?;

    Ok(())
}
