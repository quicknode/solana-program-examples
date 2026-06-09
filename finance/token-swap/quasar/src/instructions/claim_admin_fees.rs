use {
    crate::{
        state::{Config, PoolConfig, PoolConfigInner},
        ConfigPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Authorisation: `admin` is a `Signer` and must match `Config.admin`. We
/// enforce that explicitly in the handler since quasar doesn't have an
/// Anchor-style `has_one` constraint.
#[derive(Accounts)]
pub struct ClaimAdminFeesAccounts {
    #[account(address = ConfigPda::seeds())]
    pub config: Account<Config>,
    #[account(
        mut,
        address = PoolPda::seeds(config.address(), mint_a.address(), mint_b.address()),
    )]
    pub pool_config: Account<PoolConfig>,
    /// Pool authority PDA — signs the outbound transfers.
    #[account(address = PoolAuthorityPda::seeds(config.address(), mint_a.address(), mint_b.address()))]
    pub pool_authority: UncheckedAccount,
    pub mint_a: Account<Mint>,
    pub mint_b: Account<Mint>,
    /// Pool's token-A reserve. The admin's owed token-A fees are paid out of
    /// this account.
    #[account(mut)]
    pub pool_a: Account<Token>,
    /// Pool's token-B reserve. The admin's owed token-B fees are paid out of
    /// this account.
    #[account(mut)]
    pub pool_b: Account<Token>,
    /// Must equal `Config.admin` (checked in the handler).
    pub admin: Signer,
    /// Admin's token-A receiving account. Must exist; not auto-created
    /// (keeps this handler small).
    #[account(mut)]
    pub admin_token_a: Account<Token>,
    /// Admin's token-B receiving account. Must exist; not auto-created.
    #[account(mut)]
    pub admin_token_b: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_claim_admin_fees(
    accounts: &mut ClaimAdminFeesAccounts,
    bumps: &ClaimAdminFeesAccountsBumps,
) -> Result<(), ProgramError> {
    // Authorisation: only the address stored in `Config.admin` may call this.
    if *accounts.admin.address() != *accounts.config.admin() {
        return Err(ProgramError::Custom(6)); // Unauthorized
    }

    let owed_a = accounts.pool_config.admin_fees_owed_a();
    let owed_b = accounts.pool_config.admin_fees_owed_b();

    // Seed order matches PoolAuthorityPda: [b"authority", config, mint_a, mint_b, bump].
    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(crate::AUTHORITY_SEED),
        Seed::from(accounts.config.address().as_ref()),
        Seed::from(accounts.mint_a.address().as_ref()),
        Seed::from(accounts.mint_b.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    // Effects: zero the accumulators before the transfer CPIs
    // (Checks-Effects-Interactions). If a CPI fails the whole transaction
    // reverts, so resetting the onchain bookkeeping first is safe.
    let config_addr = *accounts.pool_config.config();
    let mint_a_addr = *accounts.pool_config.mint_a();
    let mint_b_addr = *accounts.pool_config.mint_b();
    accounts.pool_config.set_inner(PoolConfigInner {
        config: config_addr,
        mint_a: mint_a_addr,
        mint_b: mint_b_addr,
        admin_fees_owed_a: 0,
        admin_fees_owed_b: 0,
    });

    // Interactions: transfer the owed fees out of the pool reserves.
    if owed_a > 0 {
        accounts
            .token_program
            .transfer(
                &accounts.pool_a,
                &accounts.admin_token_a,
                &accounts.pool_authority,
                owed_a,
            )
            .invoke_signed(seeds)?;
    }

    if owed_b > 0 {
        accounts
            .token_program
            .transfer(
                &accounts.pool_b,
                &accounts.admin_token_b,
                &accounts.pool_authority,
                owed_b,
            )
            .invoke_signed(seeds)?;
    }

    Ok(())
}
