use anchor_lang::prelude::*;

use crate::{error::CarnivalError, state::food};

// Instruction Data

pub struct EatFoodInstructionData {
    pub eater_name: String,
    pub eater_ticket_count: u32,
    pub food_stand: String,
}

pub fn eat_food(ix: EatFoodInstructionData) -> Result<()> {
    let food_stands_list = food::get_food_stands();

    for food_stand in food_stands_list.iter() {
        if ix.food_stand.eq(&food_stand.name) {
            msg!("Welcome to {}! What can I get you?", food_stand.name);

            if ix.eater_ticket_count < food_stand.tickets {
                msg!(
                    "  Sorry {}, our {} is {} tickets!",
                    ix.eater_name,
                    food_stand.food_type,
                    food_stand.tickets
                );
                // Refuse service rather than logging and returning Ok(()), so
                // callers can distinguish a sale from a refusal.
                return Err(CarnivalError::NotEnoughTickets.into());
            }

            msg!("  Enjoy your {}!", food_stand.food_type);
            return Ok(());
        }
    }

    Err(ProgramError::InvalidInstructionData.into())
}
