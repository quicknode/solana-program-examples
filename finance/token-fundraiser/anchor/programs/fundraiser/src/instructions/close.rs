use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
        TransferChecked,
    },
};

use crate::{state::Fundraiser, FundraiserError, SECONDS_TO_DAYS};

#[derive(Accounts)]
pub struct CloseFundraiserAccountConstraints {
    #[account(mut)]
    pub maker: Signer,

    pub mint_to_raise: InterfaceAccount<Mint>,

    #[account(
        mut,
        has_one = mint_to_raise,
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

/// Retires a failed fundraiser so the maker can raise again.
///
/// The fundraiser PDA is derived from the maker's key alone, so while a
/// failed fundraiser's account exists the maker can never initialize
/// another one. This handler closes it once the deadline has passed, the
/// target was missed, and every contribution has been refunded.
pub fn handle_close_fundraiser(accounts: &mut CloseFundraiserAccountConstraints) -> Result<()> {
    // Closing is allowed only after the fundraiser has ended:
    // elapsed_days >= duration.
    let current_time = Clock::get()?.unix_timestamp;
    let elapsed_days = current_time
        .checked_sub(accounts.fundraiser.time_started)
        .ok_or(FundraiserError::MathOverflow)?
        .checked_div(SECONDS_TO_DAYS)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        elapsed_days >= accounts.fundraiser.duration as i64,
        FundraiserError::FundraiserNotEnded
    );

    // A successful fundraiser exits through check_contributions, which
    // already closes these accounts.
    require!(
        accounts.fundraiser.current_amount < accounts.fundraiser.amount_to_raise,
        FundraiserError::TargetMet
    );

    // Closing the vault while contributions remain would strand the
    // refunds, so every contributor must have taken theirs first.
    require!(
        accounts.fundraiser.current_amount == 0,
        FundraiserError::RefundsOutstanding
    );

    // Read these before any of the CPI handles below take their borrows.
    let maker_address = *accounts.maker.address();
    let vault_amount = accounts.vault.amount();
    let mint_decimals = accounts.mint_to_raise.decimals();

    // The vault is owned by the fundraiser PDA, so both CPIs are signed with
    // its seeds.
    let signer_seeds: [&[&[u8]]; 1] = [&[
        b"fundraiser".as_ref(),
        maker_address.as_ref(),
        &[accounts.fundraiser.bump],
    ]];

    // Refunds have already drained every tracked contribution, so anything
    // left in the vault is a direct donation; sweep it to the maker rather
    // than burn it with the account.
    if accounts.vault.amount() > 0 {
        let transfer_accounts = TransferChecked {
            from: accounts.vault.cpi_handle_mut(),
            mint: accounts.mint_to_raise.cpi_handle(),
            to: accounts.maker_ata.cpi_handle_mut(),
            authority: accounts.fundraiser.cpi_handle(),
        };
        let transfer_context = CpiContext::new_with_signer(
            accounts.token_program.address(),
            transfer_accounts,
            &signer_seeds,
        );
        transfer_checked(transfer_context, vault_amount, mint_decimals)?;
    }

    // Close the empty vault so its rent goes back to the maker. The
    // fundraiser account itself is closed by its close = maker constraint.
    let close_accounts = CloseAccount {
        account: accounts.vault.cpi_handle_mut(),
        destination: accounts.maker.cpi_handle_mut(),
        authority: accounts.fundraiser.cpi_handle(),
    };
    let close_context = CpiContext::new_with_signer(
        accounts.token_program.address(),
        close_accounts,
        &signer_seeds,
    );
    close_account(close_context)?;

    Ok(())
}
