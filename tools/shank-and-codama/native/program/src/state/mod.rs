use {
    borsh::{BorshDeserialize, BorshSerialize},
    shank::ShankAccount,
    solana_program::pubkey::Pubkey,
};

// NOTE on PDAs and Shank's `#[seeds(...)]` attribute:
//
// Older versions of Shank (0.0.x) used `#[seeds(...)]` on a `ShankAccount` to
// generate `shank_pda` / `shank_seeds_with_bump` helper methods. As of Shank
// 0.4.x that PDA code-generation produces unparsable tokens and breaks
// compilation, and the seeds are *not* emitted into the IDL anyway. Shank 0.4
// therefore only uses `ShankAccount` to extract the account's layout for the
// IDL. We keep the PDA derivation explicit here (the seed bytes are identical
// to what the old generated helpers produced).

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, ShankAccount)]
pub struct Car {
    pub year: u16,
    pub make: String,
    pub model: String,
}

impl Car {
    pub const SEED_PREFIX: &'static str = "car";

    /// Derive the PDA for a `Car` account: `["car", make, model]`.
    pub fn find_pda(program_id: &Pubkey, make: &str, model: &str) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::SEED_PREFIX.as_bytes(),
                make.as_bytes(),
                model.as_bytes(),
            ],
            program_id,
        )
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug)]
pub enum RentalOrderStatus {
    Created,
    PickedUp,
    Returned,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, ShankAccount)]
pub struct RentalOrder {
    pub car: Pubkey,
    pub name: String,
    pub pick_up_date: String,
    pub return_date: String,
    pub price: u64,
    pub status: RentalOrderStatus,
}

impl RentalOrder {
    pub const SEED_PREFIX: &'static str = "rental_order";

    /// Derive the PDA for a `RentalOrder` account:
    /// `["rental_order", car, payer]`.
    pub fn find_pda(program_id: &Pubkey, car: &Pubkey, payer: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[Self::SEED_PREFIX.as_bytes(), car.as_ref(), payer.as_ref()],
            program_id,
        )
    }
}
