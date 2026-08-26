use {
    crate::state::{AdminConfig, TransferSwitch},
    anchor_lang::prelude::*,
};

#[derive(Accounts)]
pub struct SwitchAccountConstraints<'info> {
    /// admin that controls the switch
    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: wallet - transfer sender
    #[account(mut)]
    pub wallet: UncheckedAccount<'info>,

    /// admin config
    #[account(
        has_one=admin,
        seeds=[b"admin-config"],
        bump = admin_config.bump,
    )]
    pub admin_config: Account<'info, AdminConfig>,

    /// the wallet (sender) transfer switch
    #[account(
        init_if_needed,
        payer=admin,
        space = TransferSwitch::DISCRIMINATOR.len() + TransferSwitch::INIT_SPACE,
        seeds = [wallet.key().as_ref()],
        bump,
    )]
    pub wallet_switch: Account<'info, TransferSwitch>,

    pub system_program: Program<'info, System>,
}

pub fn handle_switch(accounts: &mut SwitchAccountConstraints, on: bool, bump: u8) -> Result<()> {
        // toggle switch on/off for the given wallet
        //
        accounts.wallet_switch.set_inner(TransferSwitch {
            wallet: accounts.wallet.key(),
            on,
            bump,  // canonical bump for this wallet's PDA
        });
        Ok(())
    }

// admin_config is validated via `seeds=[b"admin-config"], bump =
// admin_config.bump`. `configure_admin` stores the canonical bump at creation
// (from `context.bumps`), so reusing it here turns the check into a single
// `create_program_address` instead of the search a bare `bump` would run.

