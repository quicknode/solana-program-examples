use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::interest_bearing_mint::InterestBearingConfig;
use anchor_spl::token_interface::spl_pod::optional_keys::OptionalNonZeroPubkey;

mod instructions;
use instructions::*;

declare_id!("DMQdkzRJz8uQSN8Kx2QYmQJn6xLKhsu3LcPYxs314MgC");

#[program]
pub mod interest_bearing {

    use super::*;

    pub fn initialize(
        context: &mut Context<InitializeAccountConstraints>,
        rate: i16,
    ) -> Result<()> {
        instructions::initialize::handler(context, rate)
    }

    pub fn update_rate(
        context: &mut Context<UpdateRateAccountConstraints>,
        rate: i16,
    ) -> Result<()> {
        instructions::update_rate::handler(context, rate)
    }
}

/// Assert the extension names `authority_key` as the account allowed to change
/// the rate. The two callers reach the extension by different routes: see
/// `initialize` for a raw TLV read, and `update_rate` for the accessor
/// anchor-spl puts on a typed mint.
pub fn check_rate_authority(config: &InterestBearingConfig, authority_key: &Address) -> Result<()> {
    assert_eq!(
        config.rate_authority,
        OptionalNonZeroPubkey::try_from(Some(*authority_key))?
    );

    msg!("{:?}", config);
    Ok(())
}
