use anchor_lang::prelude::*;
// Our program's address!
// This matches the key in the target/deploy directory
declare_id!("ww9C83noARSQVBnqmCUmaVdbJjmiwcV9j2LkXYMoUCV");

// Our Solana program!
#[program]
pub mod favorites {
    use super::*;

    // Our instruction handler! It sets the user's favorite number and color
    pub fn set_favorites(
        context: &mut Context<SetFavoritesAccountConstraints>,
        number: u64,
        color: String,
        hobbies: Vec<String>,
    ) -> Result<()> {
        msg!("Greetings from {}", context.program_id);
        let user_public_key = context.accounts.user.address();
        msg!(
            "User {user_public_key}'s favorite number is {number}, favorite color is: {color}, and their hobbies are {hobbies:?}",
        );

        *context.accounts.favorites = Favorites {
            number,
            color,
            hobbies,
            bump: context.bumps.favorites,
        };
        Ok(())
    }

    // We can also add a get_favorites instruction handler to return the user's favorite number and color
}

// What we will put inside the Favorites PDA
// `borsh` because the struct holds a `String` and a `Vec`: v2's default
// `#[account]` backing is zero-copy and needs a `Pod` (fixed-layout) type.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Favorites {
    pub number: u64,

    #[max_len(50)]
    pub color: String,

    #[max_len(5, 50)]
    pub hobbies: Vec<String>,

    /// Canonical bump for this PDA. Stored so later instructions can
    /// re-derive/validate the PDA without recomputing via `find_program_address`.
    pub bump: u8,
}
// When people call the set_favorites instruction, they will need to provide the accounts that will be modifed. This keeps Solana fast!
#[derive(Accounts)]
pub struct SetFavoritesAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        init_if_needed,
        payer = user,
        space = Favorites::DISCRIMINATOR.len() + Favorites::INIT_SPACE,
        seeds=[b"favorites", user.address().as_ref()],
        bump
    )]
    pub favorites: BorshAccount<Favorites>,

    pub system_program: Program<System>,
}
