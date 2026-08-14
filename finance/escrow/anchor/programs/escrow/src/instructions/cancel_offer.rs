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
pub struct CancelOfferAccountConstraints {
    #[account(mut)]
    pub maker: Signer,

    pub token_mint_a: InterfaceAccount<Mint>,

    #[account(
        mut,
        associated_token::mint = token_mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program,
    )]
    pub maker_token_account_a: InterfaceAccount<TokenAccount>,

    #[account(
        mut,
        close = maker,
        has_one = maker,
        has_one = token_mint_a,
        seeds = [b"offer", maker.address().as_ref(), offer.id.to_le_bytes()],
        bump = offer.bump,
    )]
    pub offer: BorshAccount<Offer>,

    #[account(
        mut,
        associated_token::mint = token_mint_a,
        associated_token::authority = offer,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_cancel_offer(context: &mut Context<CancelOfferAccountConstraints>) -> Result<()> {
    let maker_key = context.accounts.maker.address();
    let id_bytes = context.accounts.offer.id.to_le_bytes();
    let bump = [context.accounts.offer.bump];
    let offer_seeds: &[&[u8]] = &[b"offer", maker_key.as_ref(), id_bytes.as_ref(), &bump];

    // Read the balance before taking the mutable borrow of the vault.
    let vault_amount = context.accounts.vault.amount();

    // `offer` signs both CPIs below. It is a data account, so it holds a live
    // borrow on its buffer; the runtime rejects a CPI that borrows it again.
    // Release the borrow for the duration of the CPIs and take it back after.
    context.accounts.offer.release_borrow()?;
    let offer_view = *context.accounts.offer.account();

    // Move all tokens back from the vault to the maker.
    transfer_tokens(
        &mut context.accounts.vault,
        &mut context.accounts.maker_token_account_a,
        &vault_amount,
        &context.accounts.token_mint_a,
        offer_view,
        &context.accounts.token_program,
        Some(offer_seeds),
    )?;

    // Close the vault, sending its rent lamports back to the maker.
    close_token_account(
        &mut context.accounts.vault,
        *context.accounts.maker.account(),
        offer_view,
        &context.accounts.token_program,
        Some(offer_seeds),
    )?;

    // The offer account itself is closed by the `close = maker` constraint
    // above, which refunds its rent to the maker.
    Ok(())
}
