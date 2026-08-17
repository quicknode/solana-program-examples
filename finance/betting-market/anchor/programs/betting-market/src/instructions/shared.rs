use anchor_lang::prelude::*;

use crate::state::Event;

use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

// Move tokens from a wallet-owned account into the vault. The authority is a
// plain Signer (the bettor), so no PDA seeds are needed.
pub fn transfer_tokens_to_vault(
    from: &mut InterfaceAccount<TokenAccount>,
    to: &mut InterfaceAccount<TokenAccount>,
    amount: u64,
    mint: &InterfaceAccount<Mint>,
    authority: &Signer,
    token_program: &Interface<'static, TokenInterface>,
) -> Result<()> {
    let decimals = mint.decimals();
    let transfer_accounts = TransferChecked {
        from: from.cpi_handle_mut(),
        mint: mint.cpi_handle(),
        to: to.cpi_handle_mut(),
        authority: authority.cpi_handle(),
    };
    let cpi_context = CpiContext::new(token_program.address(), transfer_accounts);
    transfer_checked(cpi_context, amount, decimals)
}

/// Everything needed to sign for the Event PDA, copied out of the account
/// while its borrow is still live.
///
/// `transfer_tokens_from_vault` runs a CPI the event signs for, and the runtime
/// rejects a CPI that borrows an account the program is holding, so the caller
/// has to `release_borrow()` first. `BorshAccount` derefs into the loaded copy,
/// and that deref panics once the borrow is released, so the two fields have to
/// be read before that happens. Gathering them here makes the ordering a matter
/// of calling `new` before `release_borrow`.
pub struct EventSigner {
    view: AccountView,
    id: u64,
    bump: u8,
}

impl EventSigner {
    /// Read the event's signing material. Call this **before**
    /// `event.release_borrow()`.
    pub fn new(event: &BorshAccount<Event>) -> Self {
        Self {
            view: *event.account(),
            id: event.event_id,
            bump: event.bump,
        }
    }
}

// Move tokens out of the vault, signed by the Event PDA. The event vault's
// authority is the Event account, so the program signs with the event's seeds.
pub fn transfer_tokens_from_vault(
    vault: &mut InterfaceAccount<TokenAccount>,
    to: &mut InterfaceAccount<TokenAccount>,
    amount: u64,
    mint: &InterfaceAccount<Mint>,
    event: &EventSigner,
    token_program: &Interface<'static, TokenInterface>,
) -> Result<()> {
    let event_id_bytes = event.id.to_le_bytes();
    let seeds = &[b"event".as_ref(), event_id_bytes.as_ref(), &[event.bump]];
    let signer_seeds = [&seeds[..]];

    let decimals = mint.decimals();
    let transfer_accounts = TransferChecked {
        from: vault.cpi_handle_mut(),
        mint: mint.cpi_handle(),
        to: to.cpi_handle_mut(),
        authority: CpiHandle::readonly(&event.view),
    };
    let cpi_context =
        CpiContext::new_with_signer(token_program.address(), transfer_accounts, &signer_seeds);
    transfer_checked(cpi_context, amount, decimals)
}
