use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
    TransferChecked,
};

use crate::error::HackathonError;
use crate::state::{Hackathon, Prize};

// Cancel an unpaid prize: drain the vault to `refund_token_account`, close
// the vault, and lock the prize so `pay_winner` can no longer run. Useful
// when a prize is funded but never claimed, or when the committee wants to
// reclaim surplus tokens left in a vault after `pay_winner` paid the exact
// `prize.amount`.
#[derive(Accounts)]
#[instruction(prize_index: u8)]
pub struct CancelPrize<'info> {
    // Hackathon admin. Must match `hackathon.authority`.
    pub authority: Signer<'info>,

    // Where the vault's reclaimed rent lamports go. Separate from `authority`
    // so a Squads vault PDA (which cannot directly receive non-account
    // lamports in this context) can still authorise the cancellation while a
    // human keypair takes the rent refund.
    #[account(mut)]
    pub rent_destination: SystemAccount<'info>,

    #[account(
        has_one = authority,
        seeds = [b"hackathon", authority.key().as_ref(), super::name_seed(&hackathon.name).as_ref()],
        bump = hackathon.bump,
    )]
    pub hackathon: Account<'info, Hackathon>,

    #[account(
        mut,
        seeds = [b"prize", hackathon.key().as_ref(), &[prize_index]],
        bump = prize.bump,
        has_one = mint,
        constraint = prize.hackathon == hackathon.key(),
    )]
    pub prize: Account<'info, Prize>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = prize,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    // Token account that receives any tokens currently held in the vault.
    // Must match `mint` but otherwise unconstrained — the committee picks
    // where to send the refund.
    #[account(
        mut,
        token::mint = mint,
        token::token_program = token_program,
    )]
    pub refund_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_cancel_prize(context: Context<CancelPrize>, _prize_index: u8) -> Result<()> {
    let prize = &mut context.accounts.prize;
    require!(!prize.paid, HackathonError::AlreadyPaid);
    require!(!prize.cancelled, HackathonError::Cancelled);

    let hackathon_key = context.accounts.hackathon.key();
    let prize_index_byte = [prize.index];
    let bump = [prize.bump];
    let seeds = &[
        b"prize".as_ref(),
        hackathon_key.as_ref(),
        prize_index_byte.as_ref(),
        bump.as_ref(),
    ];
    let signer_seeds = [&seeds[..]];

    // Drain whatever is in the vault back to the refund target. This may be
    // zero (vault never funded) or more than `prize.amount` (vault was
    // over-funded); either is fine.
    let vault_amount = context.accounts.vault.amount;
    if vault_amount > 0 {
        let transfer_accounts = TransferChecked {
            from: context.accounts.vault.to_account_info(),
            mint: context.accounts.mint.to_account_info(),
            to: context.accounts.refund_token_account.to_account_info(),
            authority: prize.to_account_info(),
        };
        let cpi_context = CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            transfer_accounts,
            &signer_seeds,
        );
        transfer_checked(cpi_context, vault_amount, context.accounts.mint.decimals)?;
    }

    // Close the vault so its rent comes back. Prize account itself stays
    // open: it's an immutable record that this prize was cancelled.
    let close_accounts = CloseAccount {
        account: context.accounts.vault.to_account_info(),
        destination: context.accounts.rent_destination.to_account_info(),
        authority: prize.to_account_info(),
    };
    let cpi_context = CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        close_accounts,
        &signer_seeds,
    );
    close_account(cpi_context)?;

    prize.cancelled = true;
    Ok(())
}
