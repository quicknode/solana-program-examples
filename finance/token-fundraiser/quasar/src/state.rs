use {crate::error::FundraiserError, quasar_lang::prelude::*};

/// Number of seconds in one day. `Fundraiser::duration` is denominated in
/// days; deadline math converts it to seconds with this factor.
pub const SECONDS_PER_DAY: i64 = 86_400;

/// State for the fundraiser: records the maker, target mint, vault, amounts,
/// and timing.
#[account(discriminator = 1, set_inner)]
#[seeds(b"fundraiser", maker: Address)]
pub struct Fundraiser {
    pub maker: Address,
    pub mint_to_raise: Address,
    /// The token account holding contributions. Stored so every later
    /// instruction can bind the passed vault to this fundraiser via
    /// `has_one(vault)`.
    pub vault: Address,
    pub amount_to_raise: u64,
    pub current_amount: u64,
    /// Clock unix timestamp captured when the fundraiser was created.
    pub time_started: i64,
    /// Fundraising window length in days, counted from `time_started`.
    pub duration: u16,
    pub bump: u8,
}

/// Tracks how much a specific contributor has given to a specific fundraiser.
/// The seeds bind this record to one (fundraiser, contributor) pair, so it
/// can never be spent by another signer or against another fundraiser.
#[account(discriminator = 2, set_inner)]
#[seeds(b"contributor", fundraiser: Address, contributor: Address)]
pub struct Contributor {
    pub amount: u64,
    pub bump: u8,
}

/// The unix timestamp at which the fundraising window closes. Contributions
/// are allowed while `now < deadline`; refunds are allowed once
/// `now >= deadline`.
pub fn fundraiser_deadline(time_started: i64, duration_days: u16) -> Result<i64, ProgramError> {
    let window_seconds = (duration_days as i64)
        .checked_mul(SECONDS_PER_DAY)
        .ok_or(FundraiserError::MathOverflow)?;
    Ok(time_started
        .checked_add(window_seconds)
        .ok_or(FundraiserError::MathOverflow)?)
}
