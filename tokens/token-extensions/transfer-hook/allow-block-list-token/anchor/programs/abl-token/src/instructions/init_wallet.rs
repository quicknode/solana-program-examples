use anchor_lang::prelude::*;

use crate::{ABWallet, Config, AB_WALLET_SEED, CONFIG_SEED};

#[derive(Accounts)]
pub struct InitWalletAccountConstraints {
    #[account(mut, address = config.authority)]
    pub authority: Signer,

    #[account(seeds = [CONFIG_SEED],
        bump = config.bump)]
    pub config: Box<BorshAccount<Config>>,

    pub wallet: SystemAccount,

    #[account(
        init,
        payer = authority,
        space = ABWallet::DISCRIMINATOR.len() + ABWallet::INIT_SPACE,
        seeds = [AB_WALLET_SEED, wallet.address().as_ref()],
        bump,
    )]
    pub ab_wallet: BorshAccount<ABWallet>,

    pub system_program: Program<System>,
}

impl InitWalletAccountConstraints<'_> {
    pub fn init_wallet(&mut self, args: InitWalletArgs, bump: u8) -> Result<()> {
        let ab_wallet = &mut self.ab_wallet;
        ab_wallet.wallet = self.wallet.address();
        ab_wallet.allowed = args.allowed;
        ab_wallet.bump = bump;
        Ok(())
    }
}

#[derive(IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct InitWalletArgs {
    pub allowed: bool,
}
