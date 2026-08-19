use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::token;
use anchor_spl::{
    token_2022::{
        initialize_account3,
        spl_token_2022::{extension::ExtensionType, pod::PodAccount},
        transfer_checked, InitializeAccount3, TransferChecked,
    },
    token_interface::{Mint, Token2022, TokenAccount},
};

// Note that you cannot initialize or update the CpiGuard extension through a CPI
// https://github.com/solana-labs/solana-program-library/blob/6968859e2ee0a1764da572de340cdb58e2b4586f/token/program-2022/src/extension/cpi_guard/processor.rs#L44-L46
declare_id!("6tU3MEowU6oxxeDZLSxEwzcEZsZrhBJsfUR6xECvShid");

#[program]
pub mod cpi_guard {
    use super::*;

    pub fn cpi_transfer(context: &mut Context<CpiTransferAccountConstraints>) -> Result<()> {
        // The recipient token account is a PDA that is its own authority. v2
        // rejects an `init` constraint naming the account being initialized
        // (`token::authority` has to name a sibling field), so the account is
        // created here instead: the `init_if_needed` semantics become an
        // explicit "create when empty".
        if context.accounts.recipient_token_account.account().data_len() == 0 {
            let space = ExtensionType::try_calculate_account_len::<PodAccount>(&[])?;
            let lamports = Rent::get()?.try_minimum_balance(space)?;
            let signer_seeds: &[&[&[u8]]] =
                &[&[b"pda", &[context.bumps.recipient_token_account]]];

            create_account(
                CpiContext::new(
                    context.accounts.system_program.address(),
                    CreateAccount {
                        from: context.accounts.sender.cpi_handle_mut(),
                        to: context.accounts.recipient_token_account.cpi_handle_mut(),
                    },
                )
                .with_signer(signer_seeds),
                lamports,
                space as u64,
                context.accounts.token_program.address(),
            )?;

            let recipient_handle = context.accounts.recipient_token_account.cpi_handle_mut();
            initialize_account3(
                CpiContext::new(
                    context.accounts.token_program.address(),
                    InitializeAccount3 {
                        account: recipient_handle,
                        mint: context.accounts.mint_account.cpi_handle(),
                        // The account is its own authority.
                        authority: recipient_handle.into_readonly(),
                    },
                ),
            )?;
        }

        transfer_checked(
            CpiContext::new(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.sender_token_account.cpi_handle_mut(),
                    mint: context.accounts.mint_account.cpi_handle(),
                    to: context.accounts.recipient_token_account.cpi_handle_mut(),
                    authority: context.accounts.sender.cpi_handle(),
                },
            ),
            1,
            context.accounts.mint_account.decimals(),
        )?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct CpiTransferAccountConstraints {
    #[account(mut)]
    pub sender: Signer,

    #[account(
        mut,
        token::mint = mint_account
    )]
    pub sender_token_account: InterfaceAccount<TokenAccount>,
    /// CHECK: created and initialized as a token account by this instruction,
    /// with itself as the authority. See `cpi_transfer` above.
    #[account(
        mut,
        seeds = [b"pda"],
        bump,
    )]
    pub recipient_token_account: UncheckedAccount,
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}
