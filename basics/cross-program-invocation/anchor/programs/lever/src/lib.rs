use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("E64FVeubGC4NPNF2UBJYX4AkrVowf74fRJD9q6YhwstN");

#[program]
pub mod lever {
    use super::*;

    pub fn initialize(context: Context<InitializeLeverAccountConstraints>) -> Result<()> {
        instructions::initialize::handler(context)
    }

    pub fn switch_power(
        context: Context<SetPowerStatusAccountConstraints>,
        name: String,
    ) -> Result<()> {
        instructions::switch_power::handler(context, name)
    }
}

#[account]
#[derive(InitSpace)]
pub struct PowerStatus {
    pub is_on: bool,
}
