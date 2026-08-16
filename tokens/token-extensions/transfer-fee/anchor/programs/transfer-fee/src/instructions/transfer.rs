use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_2022::spl_token_2022::{
        extension::{
            transfer_fee::TransferFeeConfig, BaseStateWithExtensions, StateWithExtensions,
        },
        state::Mint as MintState,
    },
    token_interface::{
        transfer_checked_with_fee, Mint, Token2022, TokenAccount, TransferCheckedWithFee,
    },
};

#[derive(Accounts)]
pub struct TransferAccountConstraints {
    #[account(mut)]
    pub sender: Signer,
    pub recipient: SystemAccount,

    // Read-only: `transfer_checked_with_fee` accrues the withheld fee on the
    // destination token account, not the mint. It also has to be read-only for
    // the extension read below — a mutable data account holds an exclusive
    // borrow, so a second `try_borrow()` on it is rejected.
    pub mint_account: InterfaceAccount<Mint>,
    #[account(
        mut,
        associated_token::mint = mint_account,
        associated_token::authority = sender,
        associated_token::token_program = token_program
    )]
    pub sender_token_account: InterfaceAccount<TokenAccount>,
    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = mint_account,
        associated_token::authority = recipient,
        associated_token::token_program = token_program
    )]
    pub recipient_token_account: InterfaceAccount<TokenAccount>,
    pub token_program: Program<Token2022>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}

// transfer fees are automatically deducted from the transfer amount
// recipients receives (transfer amount - fees)
// transfer fees are stored directly on the recipient token account and must be "harvested"
pub fn handle_process_transfer(
    context: &mut Context<TransferAccountConstraints>,
    amount: u64,
) -> Result<()> {
    // Read the mint's extension data in its own scope: the `Ref` has to drop
    // before the CPI below, or the runtime rejects the CPI's borrow of the same
    // account with AccountBorrowFailed.
    let epoch = Clock::get()?.epoch;
    let fee = {
        let mint_data = context.accounts.mint_account.account().try_borrow()?;
        let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
        let extension_data = mint_with_extension.get_extension::<TransferFeeConfig>()?;
        extension_data.calculate_epoch_fee(epoch, amount).unwrap()
    };

    // mint account decimals
    let decimals = context.accounts.mint_account.decimals();

    transfer_checked_with_fee(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferCheckedWithFee {
                source: context.accounts.sender_token_account.cpi_handle_mut(),
                // Read-only slots take the wrapper's own handle: on a data account
                // it relaxes the runtime borrow check that a hand-built handle
                // over a copy of the view would still trip.
                mint: context.accounts.mint_account.cpi_handle(),
                destination: context.accounts.recipient_token_account.cpi_handle_mut(),
                authority: context.accounts.sender.cpi_handle(),
            },
        ),
        amount,   // transfer amount
        decimals, // decimals
        fee,      // fee
    )?;

    msg!("transfer amount {}", amount);
    msg!("fee amount {}", fee);

    Ok(())
}
