use anchor_lang::prelude::*;

declare_id!("Bi5N7SUQhpGknVcqPTzdFFVueQoxoUu8YTLz75J6fT8A");

// automatically generate module using program idl found in ./idls
declare_program!(lever);
use lever::accounts::PowerStatus;
use lever::cpi::accounts::SwitchPower;
use lever::cpi::switch_power;
use lever::program::Lever;

/// The longest `name` `pull_lever` passes on to the lever program, in bytes.
pub const MAX_NAME_LEN: usize = 32;

/// Room for `switch_power`'s instruction data: an 8-byte discriminator, the
/// name's 4-byte length prefix and the name itself. Anchor v2 serializes CPI
/// instruction data into a fixed-capacity buffer the caller supplies.
#[cfg(any(target_os = "solana", target_arch = "bpf"))]
const SWITCH_POWER_DATA_CAPACITY: usize = 8 + 4 + MAX_NAME_LEN;

#[program]
pub mod hand {
    use super::*;

    pub fn pull_lever(
        context: &mut Context<PullLeverAccountConstraints>,
        name: String,
    ) -> Result<()> {
        require!(name.len() <= MAX_NAME_LEN, HandError::NameTooLong);
        let cpi_ctx = CpiContext::<_, 1>::new(
            context.accounts.lever_program.address(),
            SwitchPower {
                power: context.accounts.power.cpi_handle_mut(),
            },
        );
        // Onchain, the generated CPI function writes the instruction data into
        // a buffer this handler supplies; the host build used for the IDL and
        // clippy has no buffer argument.
        #[cfg(any(target_os = "solana", target_arch = "bpf"))]
        {
            let mut data = anchor_lang::FixedVec::<u8, SWITCH_POWER_DATA_CAPACITY>::new();
            switch_power(cpi_ctx, &name, &mut data)?;
        }
        #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
        switch_power(cpi_ctx, &name)?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct PullLeverAccountConstraints {
    #[account(mut)]
    pub power: BorshAccount<PowerStatus>,
    pub lever_program: Program<Lever>,
}

#[error_code]
pub enum HandError {
    #[msg("The name is longer than MAX_NAME_LEN bytes")]
    NameTooLong,
}
