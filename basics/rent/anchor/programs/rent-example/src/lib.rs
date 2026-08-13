use anchor_lang::prelude::*;
use anchor_lang::system_program;

declare_id!("ED6f4gweAE7hWPQPXMt4kWxzDJne8VQEm9zkb1tMpFNB");

#[program]
pub mod rent_example {
    use super::*;

    pub fn create_system_account(
        context: &mut Context<CreateSystemAccountAccountConstraints>,
        address_data: AddressData,
    ) -> Result<()> {
        msg!("Program invoked. Creating a system account...");
        msg!(
            "  New public key will be: {}",
            context.accounts.new_account.address()
        );

        // Determine the necessary minimum rent by calculating the account's size
        //
        // v2 encodes instruction data with wincode rather than borsh. `BorshConfig`
        // is wincode's borsh-compatible wire format (fixed u32 little-endian length
        // prefixes), so the span matches the bytes borsh would have produced.
        let account_span = address_data.serialized_span()?;
        let lamports_required = Rent::get()?.try_minimum_balance(account_span)?;

        msg!("Account span: {}", account_span);
        msg!("Lamports required: {}", lamports_required);

        system_program::create_account(
            CpiContext::new(
                context.accounts.system_program.address(),
                system_program::CreateAccount {
                    from: context.accounts.payer.cpi_handle_mut(),
                    to: context.accounts.new_account.cpi_handle_mut(),
                },
            ),
            lamports_required,
            account_span as u64,
            context.accounts.system_program.address(),
        )?;

        msg!("Account created successfully.");
        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateSystemAccountAccountConstraints {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub new_account: Signer,
    pub system_program: Program<System>,
}

#[derive(Clone, Debug, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct AddressData {
    pub name: String,
    pub address: String,
}

impl AddressData {
    /// Bytes this struct occupies on the wire, which is what the new account
    /// has to be sized — and therefore rent-funded — for.
    fn serialized_span(&self) -> Result<usize> {
        <Self as wincode::SchemaWrite<anchor_lang::BorshConfig>>::size_of(self)
            .map_err(|_| ProgramError::InvalidInstructionData.into())
    }
}
