use quasar_lang::prelude::*;

/// Accounts for the hand program's pull_lever instruction.
/// The lever_program uses `Program<LeverProgram>` with a custom marker type
/// that implements `Id` — this lets Quasar verify the program address and
/// the executable flag during account parsing.
#[derive(Accounts)]
pub struct PullLever {
    #[account(mut)]
    pub power: UncheckedAccount,
    pub lever_program: Program<crate::LeverProgram>,
}

#[inline(always)]
pub fn handle_pull_lever(accounts: &PullLever, name: &str) -> Result<(), ProgramError> {
    log("Hand is pulling the lever!");

    // Build the switch_power instruction data for the lever program.
    //
    // Wire format: [discriminator = 1] [name: u8 length prefix + bytes].
    //
    // The lever's switch_power instruction takes `String<50>`, which Quasar
    // serialises with a single-byte length prefix (matching every other
    // Quasar program: account-data, close-account, rent, realloc,
    // repository-layout). An earlier version of this builder used a u32
    // length prefix, which sent a malformed payload on every CPI call.
    //
    // 128 bytes is enough for any reasonable name (max 50 + 1 + 1 = 52).
    let mut data = [0u8; 128];
    let name_bytes = name.as_bytes();
    let data_len = 1 + 1 + name_bytes.len();

    data[0] = 1;
    data[1] = name_bytes.len() as u8;

    let mut i = 0;
    while i < name_bytes.len() {
        data[2 + i] = name_bytes[i];
        i += 1;
    }

    let mut cpi = CpiDynamic::<1, 128>::new(accounts.lever_program.address());
    cpi.push_account(accounts.power.to_account_view(), false, true)?;
    cpi.set_data(&data[..data_len])?;
    cpi.invoke()
}
