use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};

#[derive(Accounts)]
pub struct CreateNewAccountAccountConstraints {
    #[account(mut)]
    new_account: Signer,

    #[account(
        mut,
        seeds = [
            b"rent_vault",
        ],
        bump,
    )]
    rent_vault: SystemAccount,
    system_program: Program<System>,
}

pub fn handle_create_new_account(
    context: &mut Context<CreateNewAccountAccountConstraints>,
) -> Result<()> {
    // PDA signer seeds
    let signer_seeds: &[&[&[u8]]] = &[&[b"rent_vault", &[context.bumps.rent_vault]]];

    // The minimum lamports for rent exemption
    let lamports = Rent::get()?.try_minimum_balance(0)?;

    // Create the new account, transferring lamports from the rent vault to the new account
    create_account(
        CpiContext::new(
            context.accounts.system_program.address(),
            CreateAccount {
                from: context.accounts.rent_vault.cpi_handle_mut(), // From pubkey
                to: context.accounts.new_account.cpi_handle_mut(),  // To pubkey
            },
        )
        .with_signer(signer_seeds),
        lamports,                                  // Lamports
        0,                                         // Space
        context.accounts.system_program.address(), // Owner Program
    )?;
    Ok(())
}
