use {
    anchor_lang::prelude::*,
    anchor_spl::{
        associated_token::AssociatedToken,
        token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
    },
};

#[derive(Accounts)]
pub struct TransferTokensAccountConstraints<'info> {
    #[account(mut)]
    pub sender: Signer<'info>,

    pub recipient: SystemAccount<'info>,

    #[account(mut)]
    pub mint_account: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = mint_account,
        associated_token::authority = sender,
        associated_token::token_program = token_program,
    )]
    pub sender_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = mint_account,
        associated_token::authority = recipient,
        associated_token::token_program = token_program,
    )]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
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
    context: Context<TransferTokensAccountConstraints>,
    amount: u64,
) -> Result<()> {
    msg!("Transferring tokens...");
    msg!(
        "Mint: {}",
        &context.accounts.mint_account.to_account_info().key()
    );
    msg!(
        "From Token Address: {}",
        &context.accounts.sender_token_account.key()
    );
    msg!(
        "To Token Address: {}",
        &context.accounts.recipient_token_account.key()
    );

    // Invoke the transfer_checked instruction on the token program
    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.sender_token_account.to_account_info(),
                mint: context.accounts.mint_account.to_account_info(),
                to: context.accounts.recipient_token_account.to_account_info(),
                authority: context.accounts.sender.to_account_info(),
            },
        ),
        amount,
        context.accounts.mint_account.decimals,
    )?;

    msg!("Tokens transferred successfully.");

    Ok(())
}
