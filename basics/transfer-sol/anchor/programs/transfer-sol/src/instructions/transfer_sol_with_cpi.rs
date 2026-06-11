use anchor_lang::prelude::*;
use anchor_lang::system_program;

#[derive(Accounts)]
pub struct TransferSolWithCpiAccountConstraints<'info> {
    #[account(mut)]
    payer: Signer<'info>,
    #[account(mut)]
    recipient: SystemAccount<'info>,
    system_program: Program<'info, System>,
}

pub fn handler(context: Context<TransferSolWithCpiAccountConstraints>, amount: u64) -> Result<()> {
    system_program::transfer(
        CpiContext::new(
            context.accounts.system_program.key(),
            system_program::Transfer {
                from: context.accounts.payer.to_account_info(),
                to: context.accounts.recipient.to_account_info(),
            },
        ),
        amount,
    )?;

    Ok(())
}
