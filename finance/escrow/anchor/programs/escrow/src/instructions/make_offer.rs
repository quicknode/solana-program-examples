use anchor_lang::prelude::*;
use anchor_spl::mint;

use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::Offer;

use super::transfer_tokens;

// See https://www.anchor-lang.com/docs/references/account-constraints#instruction-attribute
#[derive(Accounts)]
#[instruction(id: u64)]
pub struct MakeOfferAccountConstraints {
    #[account(mut)]
    pub maker: Signer,

    #[account(mint::token_program = token_program)]
    pub token_mint_a: InterfaceAccount<Mint>,

    #[account(mint::token_program = token_program)]
    pub token_mint_b: InterfaceAccount<Mint>,

    #[account(
        mut,
        associated_token::mint = token_mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_token_account_a: InterfaceAccount<TokenAccount>,

    // The maker's token-B ATA is initialized here, paid by the maker, so the
    // rent burden lives with the party who chose to open the offer (take_offer
    // requires this account to already exist).
    #[account(
        init_if_needed,
        payer = maker,
        associated_token::mint = token_mint_b,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_token_account_b: InterfaceAccount<TokenAccount>,

    #[account(
        init,
        payer = maker,
        space = Offer::DISCRIMINATOR.len() + Offer::INIT_SPACE,
        seeds = [b"offer", maker.address().as_ref(), id.to_le_bytes().as_ref()],
        bump
    )]
    pub offer: BorshAccount<Offer>,

    #[account(
        init,
        payer = maker,
        associated_token::mint = token_mint_a,
        associated_token::authority = offer,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

// Move the tokens from the maker's ATA to the vault
pub fn handle_send_offered_tokens_to_vault(
    context: &Context<MakeOfferAccountConstraints>,
    token_a_offered_amount: u64,
) -> Result<()> {
    transfer_tokens(
        &context.accounts.maker_token_account_a,
        &context.accounts.vault,
        &token_a_offered_amount,
        &context.accounts.token_mint_a,
        &context.accounts.maker.cpi_handle_mut(),
        &context.accounts.token_program,
        None,
    )
}

// Save the details of the offer to the offer account
pub fn handle_save_offer(
    context: &mut Context<MakeOfferAccountConstraints>,
    id: u64,
    token_b_wanted_amount: u64,
) -> Result<()> {
    *context.accounts.offer = (Offer {
        id,
        maker: *context.accounts.maker.address(),
        token_mint_a: *context.accounts.token_mint_a.address(),
        token_mint_b: *context.accounts.token_mint_b.address(),
        token_b_wanted_amount,
        bump: context.bumps.offer,
    });
    Ok(())
}
