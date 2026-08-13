use anchor_lang::prelude::*;

use anchor_spl::token_interface::{
    close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
    TransferChecked,
};

// Transfer tokens from one token account to another.
// When transferring out of a token account owned by a PDA, pass the PDA's
// signer seeds via owning_pda_seeds; otherwise pass None.
pub fn transfer_tokens<'info>(
    from: &InterfaceAccount<TokenAccount>,
    to: &InterfaceAccount<TokenAccount>,
    amount: &u64,
    mint: &InterfaceAccount<Mint>,
    authority: &AccountView,
    token_program: &Interface<'static, TokenInterface>,
    owning_pda_seeds: Option<&[&[u8]]>,
) -> Result<()> {
    let transfer_accounts = TransferChecked {
        from: from.cpi_handle_mut(),
        mint: mint.cpi_handle(),
        to: to.cpi_handle_mut(),
        authority: authority.cpi_handle_mut(),
    };

    let signer_seeds = owning_pda_seeds.map(|seeds| [seeds]);
    let cpi_context = match signer_seeds.as_ref() {
        Some(signer_seeds) => {
            CpiContext::new_with_signer(token_program.address(), transfer_accounts, signer_seeds)
        }
        None => CpiContext::new(token_program.address(), transfer_accounts),
    };

    transfer_checked(cpi_context, *amount, mint.decimals())
}

// Close a token account, sending its rent lamports to destination.
// When the token account is owned by a PDA, pass the PDA's signer seeds via
// owning_pda_seeds; otherwise pass None.
pub fn close_token_account<'info>(
    token_account: &InterfaceAccount<TokenAccount>,
    destination: &AccountView,
    authority: &AccountView,
    token_program: &Interface<'static, TokenInterface>,
    owning_pda_seeds: Option<&[&[u8]]>,
) -> Result<()> {
    let close_accounts = CloseAccount {
        account: token_account.cpi_handle_mut(),
        destination: destination.cpi_handle_mut(),
        authority: authority.cpi_handle_mut(),
    };

    let signer_seeds = owning_pda_seeds.map(|seeds| [seeds]);
    let cpi_context = match signer_seeds.as_ref() {
        Some(signer_seeds) => {
            CpiContext::new_with_signer(token_program.address(), close_accounts, signer_seeds)
        }
        None => CpiContext::new(token_program.address(), close_accounts),
    };

    close_account(cpi_context)
}
