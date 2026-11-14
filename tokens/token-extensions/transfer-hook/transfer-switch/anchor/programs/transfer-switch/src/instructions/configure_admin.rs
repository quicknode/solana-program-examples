use {crate::state::AdminConfig, anchor_lang::prelude::*};

#[derive(Accounts)]
pub struct ConfigureAdminAccountConstraints {
    // Bootstrapping the config passes the same address as both `admin` and
    // `new_admin`. Anchor v2 catches aliasing through the borrow each account
    // wrapper takes on the account's data, and `Signer` and `UncheckedAccount`
    // take none, so the two fields may name the same account.
    #[account(mut)]
    pub admin: Signer,

    /// CHECK: the new admin
    #[account(mut)]
    pub new_admin: UncheckedAccount,

    /// To hold the address of the admin that controls switches
    #[account(
        init_if_needed,
        payer=admin,
        space = AdminConfig::DISCRIMINATOR.len() + AdminConfig::INIT_SPACE,
        seeds = [b"admin-config"],
        bump
    )]
    pub admin_config: BorshAccount<AdminConfig>,

    pub system_program: Program<System>,
}

pub fn handle_is_admin(accounts: &mut ConfigureAdminAccountConstraints) -> Result<()> {
    // check if we are not creating the account for the first time,
    // ensure it's the admin that is making the change
    //
    if accounts.admin_config.is_initialised {
        // make sure it's the admin
        //
        require_keys_eq!(*accounts.admin.address(), accounts.admin_config.admin,);

        // make sure the admin is not reentering their key
        //
        require_keys_neq!(accounts.admin.address(), accounts.new_admin.address());
    }
    Ok(())
}

pub fn handle_configure_admin(
    accounts: &mut ConfigureAdminAccountConstraints,
    bump: u8,
) -> Result<()> {
    *accounts.admin_config = AdminConfig {
        admin: *accounts.new_admin.address(), // set the admin pubkey that can switch transfers on/off
        is_initialised: true,                 // let us know an admin has been set
        bump,                                 // canonical bump for the admin-config PDA
    };
    Ok(())
}
