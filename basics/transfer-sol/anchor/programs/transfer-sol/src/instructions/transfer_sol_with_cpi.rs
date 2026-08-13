use anchor_lang::prelude::*;
use anchor_lang::system_program;

#[derive(Accounts)]
pub struct TransferSolWithCpiAccountConstraints {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub recipient: SystemAccount,
    pub system_program: Program<System>,
}

pub fn handler(
    context: &mut Context<TransferSolWithCpiAccountConstraints>,
    amount: u64,
) -> Result<()> {
    system_program::transfer(
        CpiContext::new(
            context.accounts.system_program.address(),
            system_program::Transfer {
                from: context.accounts.payer.cpi_handle_mut(),
                to: context.accounts.recipient.cpi_handle_mut(),
            },
        ),
        amount,
    )?;

    Ok(())
}
