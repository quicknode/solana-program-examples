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
        bump)]
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
    *accounts.wallet_switch = (TransferSwitch {
        wallet: *accounts.wallet.address(),
        on,
        bump, // canonical bump for this wallet's PDA
    });
    Ok(())
}

// admin_config is validated via `seeds=[b"admin-config"], bump` - Anchor
// re-derives it and fails if it doesn't match, so storing AdminConfig.bump
// isn't strictly needed to validate `admin_config` inside `Switch` (the
// bump field on AdminConfig is still populated on creation to satisfy the
// 'every PDA struct stores its bump' rule and save derivation cost in any
// future call sites).
