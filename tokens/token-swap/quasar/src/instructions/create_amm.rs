use {
    crate::{state::{Amm, AmmInner}, AmmPda},
    quasar_lang::prelude::*,
};

/// Accounts for creating a new AMM.
///
/// The Anchor version derives the AMM PDA from an `id` instruction argument.
/// In Quasar, we use a simpler fixed seed `["amm"]` since the Quasar derive
/// macro seeds reference account addresses, not instruction data.
#[derive(Accounts)]
pub struct CreateAmm {
    #[account(mut, init, payer = payer, address = AmmPda::seeds())]
    pub amm: Account<Amm>,
    /// Admin authority for the AMM.
    pub admin: UncheckedAccount,
    #[account(mut)]
    pub payer: Signer,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_create_amm(accounts: &mut CreateAmm, id: Address, fee: u16) -> Result<(), ProgramError> {
    if fee >= 10000 {
        return Err(ProgramError::InvalidArgument);
    }
    accounts.amm.set_inner(AmmInner {
        id,
        admin: *accounts.admin.address(),
        fee: fee.into(),
    });
    Ok(())
}
