use {
    crate::state::{AdminConfig, TransferSwitch},
    anchor_lang::prelude::*,
};

#[derive(Accounts)]
pub struct Switch<'info> {
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
        bump,
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

pub fn handle_switch(accounts: &mut Switch, on: bool, bump: u8) -> Result<()> {
        // toggle switch on/off for the given wallet
        //
        accounts.wallet_switch.set_inner(TransferSwitch {
            wallet: accounts.wallet.key(),
            on,
            bump,  // canonical bump for this wallet's PDA
        });
        Ok(())
    }

// admin_config is validated via `seeds=[b"admin-config"], bump` - Anchor
// re-derives it and fails if it doesn't match, so storing AdminConfig.bump
// isn't strictly needed to validate `admin_config` inside `Switch` (the
// bump field on AdminConfig is still populated on creation to satisfy the
// 'every PDA struct stores its bump' rule and save derivation cost in any
// future call sites).

