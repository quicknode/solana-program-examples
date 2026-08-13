use anchor_lang::prelude::*;

use crate::PowerStatus;

#[derive(Accounts)]
pub struct SetPowerStatusAccountConstraints {
    #[account(mut)]
    pub power: BorshAccount<PowerStatus>,
}

pub fn handler(
    context: &mut Context<SetPowerStatusAccountConstraints>,
    name: String,
) -> Result<()> {
    let power = &mut context.accounts.power;
    power.is_on = !power.is_on;

    msg!("{} is pulling the power switch!", &name);

    match power.is_on {
        true => msg!("The power is now on."),
        false => msg!("The power is now off!"),
    };

    Ok(())
}
