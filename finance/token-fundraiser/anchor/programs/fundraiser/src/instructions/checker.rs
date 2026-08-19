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
pub struct CheckContributionsAccountConstraints {
    #[account(mut)]
    pub maker: Signer,

    pub mint_to_raise: InterfaceAccount<Mint>,

    #[account(
        mut,
        seeds = [b"fundraiser".as_ref(), maker.address().as_ref()],
        bump = fundraiser.bump,
        close = maker,
    )]
    pub fundraiser: BorshAccount<Fundraiser>,

    #[account(
        mut,
        associated_token::mint = mint_to_raise,
        associated_token::authority = fundraiser,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    #[account(
        init_if_needed,
        payer = maker,
        associated_token::mint = mint_to_raise,
        associated_token::authority = maker,
        associated_token::token_program = token_program,
    )]
    pub maker_ata: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,

    pub system_program: Program<System>,

    pub associated_token_program: Program<AssociatedToken>,
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

    // Read these before any of the CPI handles below take their borrows.
    let maker_address = *accounts.maker.address();
    let vault_amount = accounts.vault.amount();
    let mint_decimals = accounts.mint_to_raise.decimals();

    // `fundraiser` signs both CPIs below. It is a data account holding a live
    // borrow on its buffer, so release it across the CPIs. The runtime rejects
    // a CPI that borrows an account we still hold. Take it back after.
    let fundraiser_bump = accounts.fundraiser.bump;
    accounts.fundraiser.release_borrow()?;
    let fundraiser_view = *accounts.fundraiser.account();

    // The vault is owned by the fundraiser PDA, so both CPIs are signed with
    // its seeds.
    let signer_seeds: [&[&[u8]]; 1] = [&[
        b"fundraiser".as_ref(),
        maker_address.as_ref(),
        &[fundraiser_bump],
    ]];

    // Drain the whole vault (including any direct donations) to the maker.
    let transfer_accounts = TransferChecked {
        from: accounts.vault.cpi_handle_mut(),
        mint: accounts.mint_to_raise.cpi_handle(),
        to: accounts.maker_ata.cpi_handle_mut(),
        authority: CpiHandle::readonly(&fundraiser_view),
    };
    let transfer_context = CpiContext::new_with_signer(
        accounts.token_program.address(),
        transfer_accounts,
        &signer_seeds,
    );
    transfer_checked(transfer_context, vault_amount, mint_decimals)?;

    // Close the empty vault so its rent goes back to the maker.
    let close_accounts = CloseAccount {
        account: accounts.vault.cpi_handle_mut(),
        destination: accounts.maker.cpi_handle_mut(),
        authority: CpiHandle::readonly(&fundraiser_view),
    };
    let close_context = CpiContext::new_with_signer(
        accounts.token_program.address(),
        close_accounts,
        &signer_seeds,
    );
    close_account(close_context)?;

    // Take the borrow back before the derive's exit path touches it again.
    accounts.fundraiser.reacquire_borrow_mut()?;

    Ok(())
}
