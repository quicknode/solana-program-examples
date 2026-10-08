use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::interest_bearing_mint::InterestBearingConfig;
use anchor_spl::token_interface::spl_pod::optional_keys::OptionalNonZeroPubkey;

mod instructions;
use instructions::*;

declare_id!("DMQdkzRJz8uQSN8Kx2QYmQJn6xLKhsu3LcPYxs314MgC");

#[program]
pub mod interest_bearing {

    use super::*;

    pub fn initialize(context: Context<InitializeAccountConstraints>, rate: i16) -> Result<()> {
        instructions::initialize::handler(context, rate)
    }

    pub fn update_rate(context: Context<UpdateRateAccountConstraints>, rate: i16) -> Result<()> {
        instructions::update_rate::handler(context, rate)
    }
}

/// Assert the extension names `authority_key` as the account allowed to change
/// the rate. Both callers read the extension with anchor-spl's
/// `get_mint_extension_data`, which parses the mint's TLV data.
pub fn check_rate_authority(config: &InterestBearingConfig, authority_key: &Pubkey) -> Result<()> {
    assert_eq!(
        config.rate_authority,
        OptionalNonZeroPubkey::try_from(Some(*authority_key))?
    );

    msg!("{:?}", config);
    Ok(())
}
