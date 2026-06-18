use anchor_lang::prelude::*;

use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

// Move tokens from a wallet-owned account into the vault. The authority is a
// plain Signer (the bettor), so no PDA seeds are needed.
pub fn transfer_tokens_to_vault<'info>(
    from: &InterfaceAccount<'info, TokenAccount>,
    to: &InterfaceAccount<'info, TokenAccount>,
    amount: u64,
    mint: &InterfaceAccount<'info, Mint>,
    authority: &Signer<'info>,
    token_program: &Interface<'info, TokenInterface>,
) -> Result<()> {
    let transfer_accounts = TransferChecked {
        from: from.to_account_info(),
        mint: mint.to_account_info(),
        to: to.to_account_info(),
        authority: authority.to_account_info(),
    };
    let cpi_context = CpiContext::new(token_program.key(), transfer_accounts);
    transfer_checked(cpi_context, amount, mint.decimals)
}

// Move tokens out of the vault, signed by the Event PDA. The event vault's
// authority is the Event account, so the program signs with the event's seeds.
pub fn transfer_tokens_from_vault<'info>(
    vault: &InterfaceAccount<'info, TokenAccount>,
    to: &InterfaceAccount<'info, TokenAccount>,
    amount: u64,
    mint: &InterfaceAccount<'info, Mint>,
    event: &AccountInfo<'info>,
    token_program: &Interface<'info, TokenInterface>,
    event_id: u64,
    event_bump: u8,
) -> Result<()> {
    let event_id_bytes = event_id.to_le_bytes();
    let seeds = &[b"event".as_ref(), event_id_bytes.as_ref(), &[event_bump]];
    let signer_seeds = [&seeds[..]];

    let transfer_accounts = TransferChecked {
        from: vault.to_account_info(),
        mint: mint.to_account_info(),
        to: to.to_account_info(),
        authority: event.clone(),
    };
    let cpi_context = CpiContext::new_with_signer(
        token_program.key(),
        transfer_accounts,
        &signer_seeds,
    );
    transfer_checked(cpi_context, amount, mint.decimals)
}
