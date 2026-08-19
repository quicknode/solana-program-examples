use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("E64FVeubGC4NPNF2UBJYX4AkrVowf74fRJD9q6YhwstN");

#[program]
pub mod lever {
    use super::*;

    pub fn initialize(context: &mut Context<InitializeLeverAccountConstraints>) -> Result<()> {
        instructions::initialize::handler(context)
    }

    pub fn switch_power(
        context: &mut Context<SetPowerStatusAccountConstraints>,
        name: String,
    ) -> Result<()> {
        instructions::switch_power::handler(context, name)
    }
}

// `borsh` rather than v2's zero-copy default. A zero-copy `PowerStatus` would
// have to store `is_on` as `PodBool` (bytemuck rejects `bool`, since only
// `0x00`/`0x01` are valid bit patterns), and the generated IDL renders
// `PodBool` as a plain `bool` alias, so `declare_program!` in the `hand`
// program would regenerate a struct that is no longer `Pod`. Borsh keeps the
// field a real `bool` on both sides of the CPI.
#[account(borsh)]
#[derive(InitSpace)]
pub struct PowerStatus {
    pub is_on: bool,
}
