use quasar_lang::cpi::Seed;
use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::state::EVENT_SEED;

/// Move tokens from a wallet-owned account into a vault. The authority is a
/// plain Signer (the bettor), so no PDA seeds are needed. The token-account and
/// mint params are generic so callers can pass either `Account<Token>` or
/// `InterfaceAccount<Token>`.
#[inline(always)]
pub fn transfer_to_vault(
    token_program: &Program<TokenProgram>,
    from: &impl AsAccountView,
    mint: &impl AsAccountView,
    to: &impl AsAccountView,
    authority: &Signer,
    amount: u64,
    decimals: u8,
) -> Result<(), ProgramError> {
    token_program
        .transfer_checked(from, mint, to, authority, amount, decimals)
        .invoke()
}

/// Move tokens out of an event's vault, signed by the Event PDA (the vault's
/// token authority), using the event's seeds.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub fn transfer_from_vault(
    token_program: &Program<TokenProgram>,
    vault: &impl AsAccountView,
    mint: &impl AsAccountView,
    to: &impl AsAccountView,
    event: &impl AsAccountView,
    amount: u64,
    decimals: u8,
    event_id: u64,
    event_bump: u8,
) -> Result<(), ProgramError> {
    let event_id_bytes = event_id.to_le_bytes();
    let bump = [event_bump];
    let seeds = [
        Seed::from(EVENT_SEED),
        Seed::from(event_id_bytes.as_ref()),
        Seed::from(bump.as_ref()),
    ];
    token_program
        .transfer_checked(vault, mint, to, event, amount, decimals)
        .invoke_signed(&seeds)
}
