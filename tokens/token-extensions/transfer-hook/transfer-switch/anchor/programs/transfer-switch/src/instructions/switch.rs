use {
    crate::state::{AdminConfig, TransferSwitch},
    anchor_lang::prelude::*,
};

#[derive(Accounts)]
pub struct SwitchAccountConstraints {
    /// admin that controls the switch
    #[account(mut, address = admin_config.admin)]
    pub admin: Signer,

    /// CHECK: wallet - transfer sender
    #[account(mut)]
    pub wallet: UncheckedAccount,

    /// admin config
    #[account(seeds=[b"admin-config"],
        bump = admin_config.bump)]
    pub admin_config: BorshAccount<AdminConfig>,

    /// the wallet (sender) transfer switch
    #[account(
        init_if_needed,
        payer=admin,
        space = TransferSwitch::DISCRIMINATOR.len() + TransferSwitch::INIT_SPACE,
        seeds = [wallet.address().as_ref()],
        bump,
    )]
    pub wallet_switch: BorshAccount<TransferSwitch>,

    pub system_program: Program<System>,
}

pub fn handle_switch(accounts: &mut SwitchAccountConstraints, on: bool, bump: u8) -> Result<()> {
    // toggle switch on/off for the given wallet
    //
    *accounts.wallet_switch = TransferSwitch {
        wallet: *accounts.wallet.address(),
        on,
        bump, // canonical bump for this wallet's PDA
    };
    Ok(())
}

// admin_config is validated via `seeds=[b"admin-config"], bump =
// admin_config.bump`. `configure_admin` stores the canonical bump at creation
// (from `context.bumps`), so reusing it here turns the check into a single
// `create_program_address` instead of the search a bare `bump` would run.
