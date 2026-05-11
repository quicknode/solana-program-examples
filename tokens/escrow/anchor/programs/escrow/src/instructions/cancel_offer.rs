use anchor_lang::prelude::*;

use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
        TransferChecked,
    },
};

use crate::Offer;

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
    let seeds = &[
        b"offer".as_ref(),
        maker_key.as_ref(),
        id_bytes.as_ref(),
        &[context.accounts.offer.bump],
    ];
    let signer_seeds = [&seeds[..]];

    // Move all tokens back from the vault to the maker.
    let vault_amount = context.accounts.vault.amount;
    let transfer_accounts = TransferChecked {
        from: context.accounts.vault.to_account_info(),
        mint: context.accounts.token_mint_a.to_account_info(),
        to: context.accounts.maker_token_account_a.to_account_info(),
        authority: context.accounts.offer.to_account_info(),
    };
    let cpi_context = CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        transfer_accounts,
        &signer_seeds,
    );
    transfer_checked(
        cpi_context,
        vault_amount,
        context.accounts.token_mint_a.decimals,
    )?;

    // Close the vault, sending its rent lamports back to the maker.
    let close_accounts = CloseAccount {
        account: context.accounts.vault.to_account_info(),
        destination: context.accounts.maker.to_account_info(),
        authority: context.accounts.offer.to_account_info(),
    };
    let cpi_context = CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        close_accounts,
        &signer_seeds,
    );
    close_account(cpi_context)?;

    // The offer account itself is closed by the `close = maker` constraint
    // above, which refunds its rent to the maker.
    Ok(())
}
