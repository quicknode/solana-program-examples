use anchor_lang::prelude::*;

use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::Offer;

use super::{close_token_account, transfer_tokens};

// Cancel an outstanding offer. Without this handler, an abandoned offer would
// keep the maker's token-A locked in the vault forever (and the offer
// account's rent unclaimed). The maker signs, the vault tokens flow back to
// the maker, and both the vault and the offer accounts are closed.
#[derive(Accounts)]
pub struct CancelOffer<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,

    pub token_mint_a: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = token_mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program,
    )]
    pub maker_token_account_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        close = maker,
        has_one = maker,
        has_one = token_mint_a,
        seeds = [b"offer", maker.key().as_ref(), offer.id.to_le_bytes().as_ref()],
        bump = offer.bump,
    )]
    pub offer: Account<'info, Offer>,

    #[account(
        mut,
        associated_token::mint = token_mint_a,
        associated_token::authority = offer,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_cancel_offer(context: Context<CancelOffer>) -> Result<()> {
    let maker_key = context.accounts.maker.key();
    let id_bytes = context.accounts.offer.id.to_le_bytes();
    let bump = [context.accounts.offer.bump];
    let offer_seeds: &[&[u8]] = &[b"offer", maker_key.as_ref(), id_bytes.as_ref(), &bump];

    // Move all tokens back from the vault to the maker.
    transfer_tokens(
        &context.accounts.vault,
        &context.accounts.maker_token_account_a,
        &context.accounts.vault.amount,
        &context.accounts.token_mint_a,
        &context.accounts.offer.to_account_info(),
        &context.accounts.token_program,
        Some(offer_seeds),
    )?;

    // Close the vault, sending its rent lamports back to the maker.
    close_token_account(
        &context.accounts.vault,
        &context.accounts.maker.to_account_info(),
        &context.accounts.offer.to_account_info(),
        &context.accounts.token_program,
        Some(offer_seeds),
    )?;

    // The offer account itself is closed by the `close = maker` constraint
    // above, which refunds its rent to the maker.
    Ok(())
}
