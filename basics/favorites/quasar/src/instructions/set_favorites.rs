use {
    crate::state::{Favorites, FavoritesInner},
    quasar_lang::{prelude::*, sysvars::Sysvar},
};

/// Accounts for setting user favourites. Uses `init_if_needed` so the same
/// instruction can create or update the favourites PDA.
#[derive(Accounts)]
pub struct SetFavorites {
    #[account(mut)]
    pub user: Signer,
    #[account(mut, init(idempotent), payer = user, address = Favorites::seeds(user.address()))]
    pub favorites: Account<Favorites>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_set_favorites(accounts: &mut SetFavorites, number: u64, color: &str) -> Result<(), ProgramError> {
    let rent = Rent::get()?;
    accounts.favorites.set_inner(
        FavoritesInner { number, color },
        accounts.user.to_account_view(),
        rent.lamports_per_byte(),
        rent.exemption_threshold_raw(),
    )
}
