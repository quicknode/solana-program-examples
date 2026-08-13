use anchor_lang::prelude::*;

use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

// Move tokens from a wallet-owned account into the vault. The authority is a
// plain Signer (the bettor), so no PDA seeds are needed.
pub fn transfer_tokens_to_vault<'info>(
    from: &InterfaceAccount<TokenAccount>,
    to: &InterfaceAccount<TokenAccount>,
    amount: u64,
    mint: &InterfaceAccount<Mint>,
    authority: &Signer,
    token_program: &Interface<'static, TokenInterface>,
) -> Result<()> {
    let transfer_accounts = TransferChecked {
        from: from.cpi_handle_mut(),
        mint: mint.cpi_handle(),
        to: to.cpi_handle_mut(),
        authority: authority.cpi_handle(),
    };
    let cpi_context = CpiContext::new(token_program.address(), transfer_accounts);
    transfer_checked(cpi_context, amount, mint.decimals())
}

// Move tokens out of the vault, signed by the Event PDA. The event vault's
// authority is the Event account, so the program signs with the event's seeds.
pub fn transfer_tokens_from_vault<'info>(
    vault: &InterfaceAccount<TokenAccount>,
    to: &InterfaceAccount<TokenAccount>,
    amount: u64,
    mint: &InterfaceAccount<Mint>,
    event: &AccountView,
    token_program: &Interface<'static, TokenInterface>,
    event_id: u64,
    event_bump: u8,
) -> Result<()> {
    let event_id_bytes = event_id.to_le_bytes();
    let seeds = &[b"event".as_ref(), event_id_bytes.as_ref(), &[event_bump]];
    let signer_seeds = [&seeds[..]];

    let transfer_accounts = TransferChecked {
        from: vault.cpi_handle_mut(),
        mint: mint.cpi_handle(),
        to: to.cpi_handle_mut(),
        authority: event.clone(),
    };
    let cpi_context = CpiContext::new_with_signer(
        token_program.address(),
        transfer_accounts,
        &signer_seeds,
    );
    transfer_checked(cpi_context, amount, mint.decimals())
}
