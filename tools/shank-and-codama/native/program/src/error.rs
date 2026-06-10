use solana_program::program_error::ProgramError;

/// Errors returned by the car rental service program.
/// Codes start at 6000 (the same offset Anchor uses for custom errors), so
/// they never collide with `ProgramError`'s built-in codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarRentalError {
    /// The car account passed in does not match the PDA derived from the
    /// car's make and model.
    CarAccountAddressMismatch = 6000,
    /// The rental account passed in does not match the PDA derived from the
    /// car account and the payer.
    RentalAccountAddressMismatch,
    /// The payer must sign: the rental PDA is derived from the payer's key,
    /// so without this check anyone could act on anyone else's rental.
    PayerSignatureMissing,
    /// The rental account is not owned by this program, so its data cannot
    /// be trusted.
    RentalAccountNotOwnedByProgram,
    /// A car can only be picked up from a rental in `Created` status.
    RentalNotInCreatedStatus,
    /// A car can only be returned from a rental in `PickedUp` status.
    RentalNotInPickedUpStatus,
}

impl From<CarRentalError> for ProgramError {
    fn from(error: CarRentalError) -> Self {
        ProgramError::Custom(error as u32)
    }
}
