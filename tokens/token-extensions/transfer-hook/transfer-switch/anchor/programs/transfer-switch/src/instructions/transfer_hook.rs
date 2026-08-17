use {
    crate::{error::TransferError, state::TransferSwitch},
    anchor_lang::prelude::*,
    anchor_spl::{
        token_2022::spl_token_2022::{
            extension::{
                transfer_hook::TransferHookAccount, BaseStateWithExtensions, PodStateWithExtensions,
            },
            pod::PodAccount,
        },
        token_interface::Mint,
    },
};

#[derive(Accounts)]
pub struct TransferHookAccountConstraints {
    /// CHECK: Sender token account
    #[account()]
    pub source_token_account: UncheckedAccount,

    /// The mint of the token transferring
    #[account()]
    pub token_mint: InterfaceAccount<Mint>,

    /// CHECK: Recipient token account
    #[account()]
    pub receiver_token_account: UncheckedAccount,

    /// CHECK: the transfer sender
    #[account()]
    pub wallet: UncheckedAccount,

    /// CHECK: extra account metas
    #[account(
        seeds = [b"extra-account-metas", token_mint.address().as_ref()],
        bump,
    )]
    pub extra_account_metas_list: UncheckedAccount,

    /// sender transfer switch
    #[account(
        seeds=[wallet.address().as_ref()],
        bump,
    )]
    pub wallet_switch: BorshAccount<TransferSwitch>,
}

pub fn handle_assert_switch_is_on(accounts: &mut TransferHookAccountConstraints) -> Result<()> {
    if !accounts.wallet_switch.on {
        return err!(TransferError::SwitchNotOn);
    }
    Ok(())
}

pub fn handle_assert_is_transferring(accounts: &mut TransferHookAccountConstraints) -> Result<()> {
    // Read-only: the account already holds a shared borrow of its buffer, and a
    // second shared borrow is fine where `try_borrow_mut` would be rejected.
    let account_data_ref = accounts.source_token_account.account().try_borrow()?;
    // .map_err() needed because spl-token-2022 uses solana-program-error 2.x
    // while anchor-lang uses 3.x - structurally identical but different semver types
    let account = PodStateWithExtensions::<PodAccount>::unpack(&account_data_ref)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let account_extension = account
        .get_extension::<TransferHookAccount>()
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if !bool::from(account_extension.transferring) {
        return err!(TransferError::IsNotCurrentlyTransferring);
    }

    Ok(())
}
