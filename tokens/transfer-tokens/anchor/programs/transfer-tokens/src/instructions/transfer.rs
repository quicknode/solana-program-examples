use {
    anchor_lang::prelude::*,
    anchor_spl::{
        associated_token::AssociatedToken,
        token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
    },
};

#[derive(Accounts)]
pub struct TransferTokensAccountConstraints {
    #[account(mut)]
    pub sender: Signer,

    pub recipient: SystemAccount,

    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,

    #[account(
        mut,
        associated_token::mint = mint_account,
        associated_token::authority = sender,
        associated_token::token_program = token_program,
    )]
    pub sender_token_account: InterfaceAccount<TokenAccount>,

    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = mint_account,
        associated_token::authority = recipient,
        associated_token::token_program = token_program,
    )]
    pub recipient_token_account: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}

/// Transfers `amount` tokens from the sender's to the recipient's associated
/// token account.
///
/// `amount` is in minor units (the raw integer the token program operates
/// on). Clients convert from major units, e.g. 1 token with 9 decimals is
/// `1 * 10u64.pow(9)` minor units. `transfer_checked` carries the mint and
/// decimals through the CPI so a wrong-mint or wrong-decimals account fails
/// the CPI instead of silently moving the wrong quantity.
pub fn handle_transfer_tokens(
    context: &mut Context<TransferTokensAccountConstraints>,
    amount: u64,
) -> Result<()> {
    msg!("Transferring tokens...");
    msg!("Mint: {}", context.accounts.mint_account.address());
    msg!(
        "From Token Address: {}",
        &context.accounts.sender_token_account.address()
    );
    msg!(
        "To Token Address: {}",
        &context.accounts.recipient_token_account.address()
    );

    // Invoke the transfer_checked instruction on the token program
    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.sender_token_account.cpi_handle_mut(),
                // Read-only slots take the wrapper's own handle: on a data account
                // it relaxes the runtime borrow check that a hand-built handle
                // over a copy of the view would still trip.
                mint: context.accounts.mint_account.cpi_handle(),
                to: context.accounts.recipient_token_account.cpi_handle_mut(),
                authority: context.accounts.sender.cpi_handle(),
            },
        ),
        amount,
        context.accounts.mint_account.decimals(),
    )?;

    msg!("Tokens transferred successfully.");

    Ok(())
}
