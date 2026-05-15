use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::error::HackathonError;
use crate::state::{Hackathon, Prize};

// Unpermissioned: anyone can trigger payment once a winner is set and the
// vault holds enough tokens. The caller pays only the transaction fee; the
// transferred tokens come from the prize vault.
#[derive(Accounts)]
#[instruction(prize_index: u8)]
pub struct PayWinner<'info> {
    #[account(mut)]
    pub caller: Signer<'info>,

    #[account(
        seeds = [b"hackathon", hackathon.authority.as_ref(), super::name_seed(&hackathon.name).as_ref()],
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

    // Winner's token account. Validated against `prize.winner` in the
    // handler (Anchor cannot express `authority = prize.winner.unwrap()`).
    #[account(
        mut,
        token::mint = mint,
        token::token_program = token_program,
    )]
    pub winner_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_pay_winner(context: Context<PayWinner>, _prize_index: u8) -> Result<()> {
    let prize = &mut context.accounts.prize;
    require!(!prize.paid, HackathonError::AlreadyPaid);
    require!(!prize.cancelled, HackathonError::Cancelled);
    let winner = prize.winner.ok_or(HackathonError::NoWinner)?;
    require_keys_eq!(
        context.accounts.winner_token_account.owner,
        winner,
        HackathonError::WinnerMismatch
    );
    require!(
        context.accounts.vault.amount >= prize.amount,
        HackathonError::Underfunded
    );

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

    let transfer_accounts = TransferChecked {
        from: context.accounts.vault.to_account_info(),
        mint: context.accounts.mint.to_account_info(),
        to: context.accounts.winner_token_account.to_account_info(),
        authority: prize.to_account_info(),
    };
    let cpi_context = CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        transfer_accounts,
        &signer_seeds,
    );
    transfer_checked(cpi_context, prize.amount, context.accounts.mint.decimals)?;

    prize.paid = true;
    Ok(())
}
