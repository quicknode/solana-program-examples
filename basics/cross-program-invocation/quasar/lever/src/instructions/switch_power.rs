use {
    crate::state::PowerStatus,
    quasar_lang::prelude::*,
};

/// Accounts for toggling the power switch.
#[derive(Accounts)]
pub struct SwitchPowerAccountConstraints {
    #[account(mut)]
    pub power: Account<PowerStatus>,
}

#[inline(always)]
pub fn handle_switch_power(accounts: &mut SwitchPowerAccountConstraints, name: &str) -> Result<(), ProgramError> {
    let current: bool = accounts.power.is_on.into();
    let new_state = !current;
    accounts.power.is_on = PodBool::from(new_state);

    // Quasar's log() takes &str - no format! in no_std.
    // Logging the name verifies the wire format end-to-end: a stale u32
    // length prefix would surface here as a corrupted name (e.g. the
    // first three bytes parsed as zeros, leaving "\0\0\0Al" instead of
    // "Alice").
    log("Someone is pulling the power switch!");
    log(name);

    if new_state {
        log("The power is now on.");
    } else {
        log("The power is now off!");
    }

    Ok(())
}
