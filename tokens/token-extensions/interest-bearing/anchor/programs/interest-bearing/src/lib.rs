use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    extension::{
        interest_bearing_mint::InterestBearingConfig, BaseStateWithExtensions, StateWithExtensions,
    },
    state::Mint as MintState,
};
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

pub fn check_mint_data(mint_account_info: &AccountView, authority_key: &Address) -> Result<()> {
    let mint_data = mint_account_info.try_borrow()?;
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let extension_data = mint_with_extension.get_extension::<InterestBearingConfig>()?;

    assert_eq!(
        extension_data.rate_authority,
        OptionalNonZeroPubkey::try_from(Some(*authority_key))?
    );

    msg!("{:?}", extension_data);
    Ok(())
}
