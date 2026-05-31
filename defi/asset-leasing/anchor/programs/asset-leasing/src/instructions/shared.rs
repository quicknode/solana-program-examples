use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
    TransferChecked,
};

/// Transfer tokens from a user-controlled account to a program-controlled
/// vault (or any other account the signer owns). Authority is a plain signer.
pub fn transfer_tokens_from_user<'info>(
    from: &InterfaceAccount<'info, TokenAccount>,
    to: &InterfaceAccount<'info, TokenAccount>,
    amount: u64,
    mint: &InterfaceAccount<'info, Mint>,
    authority: &Signer<'info>,
    token_program: &Interface<'info, TokenInterface>,
) -> Result<()> {
    let accounts = TransferChecked {
        from: from.to_account_info(),
        mint: mint.to_account_info(),
        to: to.to_account_info(),
        authority: authority.to_account_info(),
    };
    transfer_checked(
        CpiContext::new(token_program.key(), accounts),
        amount,
        mint.decimals,
    )
}

/// Transfer tokens out of a program-derived address-owned vault using the supplied signer seeds.
/// Used by the program when moving tokens held under its authority.
pub fn transfer_tokens_from_vault<'info>(
    from: &InterfaceAccount<'info, TokenAccount>,
    to: &InterfaceAccount<'info, TokenAccount>,
    amount: u64,
    mint: &InterfaceAccount<'info, Mint>,
    authority: &AccountInfo<'info>,
    token_program: &Interface<'info, TokenInterface>,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let accounts = TransferChecked {
        from: from.to_account_info(),
        mint: mint.to_account_info(),
        to: to.to_account_info(),
        authority: authority.clone(),
    };
    transfer_checked(
        CpiContext::new_with_signer(token_program.key(), accounts, signer_seeds),
        amount,
        mint.decimals,
    )
}

/// Close a program-derived address-owned token vault and forward its rent-exempt lamports to
/// `destination`. The vault is its own token-account authority, so the caller
/// just passes the same vault `AccountInfo` as both the account and the
/// authority, with the vault's signer seeds for the cross-program invocation.
///
/// `destination` is an `AccountInfo` so callers can pass whichever wrapper
/// they hold (`Signer`, `UncheckedAccount`, etc.) via `.to_account_info()`.
pub fn close_vault<'info>(
    vault: &InterfaceAccount<'info, TokenAccount>,
    destination: &AccountInfo<'info>,
    token_program: &Interface<'info, TokenInterface>,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let accounts = CloseAccount {
        account: vault.to_account_info(),
        destination: destination.clone(),
        authority: vault.to_account_info(),
    };
    close_account(CpiContext::new_with_signer(
        token_program.key(),
        accounts,
        signer_seeds,
    ))
}
