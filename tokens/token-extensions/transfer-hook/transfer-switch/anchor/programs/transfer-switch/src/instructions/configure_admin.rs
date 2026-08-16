use {crate::state::AdminConfig, anchor_lang::prelude::*};

#[derive(Accounts)]
pub struct ConfigureAdminAccountConstraints {
    // Bootstrapping the config passes the same key as both `admin` and
    // `new_admin`, so the two slots legitimately alias. v2 rejects an account
    // that appears twice while any of its slots is in the mutable mask, and it
    // flags *both* indices — so both carry `unsafe(dup)`, which keeps them
    // writable while taking them out of that mask.
    #[account(unsafe(dup))]
    pub admin: Signer,

    /// CHECK: the new admin
    #[account(unsafe(dup))]
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
    *accounts.admin_config = (AdminConfig {
        admin: *accounts.new_admin.address(), // set the admin pubkey that can switch transfers on/off
        is_initialised: true,                 // let us know an admin has been set
        bump,                                 // canonical bump for the admin-config PDA
    });
    Ok(())
}
