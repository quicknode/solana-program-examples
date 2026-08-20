use anchor_lang::prelude::*;

use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::Offer;

use super::{close_token_account, transfer_tokens};

#[derive(Accounts)]
pub struct TakeOfferAccountConstraints {
    #[account(mut)]
    pub taker: Signer,

    #[account(mut, address = offer.maker)]
    pub maker: SystemAccount,

    #[account(address = offer.token_mint_a)]
    pub token_mint_a: InterfaceAccount<Mint>,

    #[account(address = offer.token_mint_b)]
    pub token_mint_b: InterfaceAccount<Mint>,

    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = token_mint_a,
        associated_token::authority = taker,
        associated_token::token_program = token_program,
    )]
    pub taker_token_account_a: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = token_mint_b,
        associated_token::authority = taker,
        associated_token::token_program = token_program,
    )]
    pub taker_token_account_b: Box<InterfaceAccount<TokenAccount>>,

    // The maker's token-B ATA is initialized in make_offer, paid by the maker.
    #[account(
        mut,
        associated_token::mint = token_mint_b,
        associated_token::authority = maker,
        associated_token::token_program = token_program,
    )]
    pub maker_token_account_b: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        close = maker,
        seeds = [b"offer", maker.address().as_ref(), offer.id.to_le_bytes()],
        bump = offer.bump,
    )]
    offer: BorshAccount<Offer>,

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

pub fn handle_send_wanted_tokens_to_maker(
    context: &mut Context<TakeOfferAccountConstraints>,
) -> Result<()> {
    let wanted_amount = context.accounts.offer.token_b_wanted_amount;
    let taker_view = *context.accounts.taker.account();
    transfer_tokens(
        &mut context.accounts.taker_token_account_b,
        &mut context.accounts.maker_token_account_b,
        &wanted_amount,
        &context.accounts.token_mint_b,
        taker_view,
        &context.accounts.token_program,
        None,
    )
}

pub fn handle_withdraw_and_close_vault(
    context: &mut Context<TakeOfferAccountConstraints>,
) -> Result<()> {
    let maker_key = context.accounts.maker.address();
    let id_bytes = context.accounts.offer.id.to_le_bytes();
    let bump = [context.accounts.offer.bump];
    let offer_seeds: &[&[u8]] = &[b"offer", maker_key.as_ref(), id_bytes.as_ref(), &bump];

    // Read the balance before taking the mutable borrow of the vault.
    let vault_amount = context.accounts.vault.amount();

    // `offer` signs both CPIs below. It is a data account, so it holds a live
    // borrow on its buffer; the runtime rejects a CPI that borrows it again.
    context.accounts.offer.release_borrow()?;
    let offer_view = *context.accounts.offer.account();

    transfer_tokens(
        &mut context.accounts.vault,
        &mut context.accounts.taker_token_account_a,
        &vault_amount,
        &context.accounts.token_mint_a,
        offer_view,
        &context.accounts.token_program,
        Some(offer_seeds),
    )?;

    // The maker paid the vault's rent in make_offer, so the vault closes back
    // to the maker (the offer account does the same via `close = maker`).
    close_token_account(
        &mut context.accounts.vault,
        *context.accounts.maker.account(),
        offer_view,
        &context.accounts.token_program,
        Some(offer_seeds),
    )?;

    // Take the borrow back so the derive's exit path (and `close = maker`) can
    // serialize and close the account.
    context.accounts.offer.reacquire_borrow_mut()
}
