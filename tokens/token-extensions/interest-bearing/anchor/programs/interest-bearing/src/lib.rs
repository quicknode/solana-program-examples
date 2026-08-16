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
    // The mint is declared `mut`, and v2 marks a mutable data account as
    // exclusively borrowed (pinocchio's `borrow_state == 0`), so `try_borrow()`
    // on it is rejected. Reading through the exclusive borrow we already hold
    // is what the wrapper itself does.
    //
    // SAFETY: the caller holds the mint's exclusive borrow for the whole
    // instruction, and this reads it without handing out a second one.
    let mint_data = unsafe { mint_account_info.borrow_unchecked() };
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let extension_data = mint_with_extension.get_extension::<InterestBearingConfig>()?;

    assert_eq!(
        extension_data.rate_authority,
        OptionalNonZeroPubkey::try_from(Some(*authority_key))?
    );

    msg!("{:?}", extension_data);
    Ok(())
}
