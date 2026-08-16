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

    #[account(mut)]
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
    // `AccountView` is Copy, and a copy still points at the same
    // account — v2's typed handles make the aliasing a compile error.
    let mint_account_view = *context.accounts.mint_account.account();
    // read mint account extension data
    // Read-only: the account already holds a shared borrow of its buffer, and a
    // second shared borrow is fine where a writable handle would be rejected.
    let mint_data = context.accounts.mint_account.account().try_borrow()?;
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let extension_data = mint_with_extension.get_extension::<TransferFeeConfig>()?;

    // calculate expected fee
    let epoch = Clock::get()?.epoch;
    let fee = extension_data.calculate_epoch_fee(epoch, amount).unwrap();

    // mint account decimals
    let decimals = context.accounts.mint_account.decimals();

    transfer_checked_with_fee(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferCheckedWithFee {
                source: context.accounts.sender_token_account.cpi_handle_mut(),
                mint: CpiHandle::readonly(&mint_account_view),
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
