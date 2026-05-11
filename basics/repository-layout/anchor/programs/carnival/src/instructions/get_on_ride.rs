use anchor_lang::prelude::*;

use crate::{error::CarnivalError, state::ride};

// Instruction Data

pub struct GetOnRideInstructionData {
    pub rider_name: String,
    pub rider_height: u32,
    pub rider_ticket_count: u32,
    pub ride: String,
}

pub fn get_on_ride(ix: GetOnRideInstructionData) -> Result<()> {
    let rides_list = ride::get_rides();

    for ride in rides_list.iter() {
        if ix.ride.eq(&ride.name) {
            msg!("You're about to ride the {}!", ride.name);

            // Refuse service: failures used to log + return Ok(()), which made
            // them indistinguishable from a successful ride for callers and
            // tests. Return a real error instead.
            if ix.rider_ticket_count < ride.tickets {
                msg!(
                    "  Sorry {}, you need {} tickets to ride the {}!",
                    ix.rider_name,
                    ride.tickets,
                    ride.name
                );
                return Err(CarnivalError::NotEnoughTickets.into());
            };

            if ix.rider_height < ride.min_height {
                msg!(
                    "  Sorry {}, you need to be {}\" tall to ride the {}!",
                    ix.rider_name,
                    ride.min_height,
                    ride.name
                );
                return Err(CarnivalError::RiderTooShort.into());
            };

            msg!("  Welcome aboard the {}!", ride.name);

            if ride.upside_down {
                msg!("  Btw, this ride goes upside down. Hold on tight!");
            };

            return Ok(());
        }
    }

    Err(ProgramError::InvalidInstructionData.into())
}
