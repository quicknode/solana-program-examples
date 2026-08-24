use anchor_lang::prelude::*;

// Real failure modes for the carnival examples. Previously this file was empty
// and the rejection branches in the instruction handlers just logged a message
// and returned Ok(()), which made it impossible for callers (or tests) to
// distinguish "the ride happened" from "the ride refused service".
#[error_code]
pub enum CarnivalError {
    // The rider/gamer/eater did not bring enough tickets for the requested
    // attraction.
    #[msg("Not enough tickets for the requested attraction")]
    NotEnoughTickets,

    // The rider is below the minimum height required for the ride.
    #[msg("Rider is below the minimum height for this ride")]
    RiderTooShort,
}
