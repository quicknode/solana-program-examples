use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::{
    token_2022::spl_token_2022::{
        extension::{BaseStateWithExtensions, PodStateWithExtensions},
        pod::PodMint,
    },
    token_interface::{token_metadata_update_field, Mint, Token2022, TokenMetadataUpdateField},
};
use spl_token_metadata_interface::state::{Field, TokenMetadata};

#[derive(Accounts)]
pub struct UpdateFieldAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

pub fn process_update_field(
    context: &mut Context<UpdateFieldAccountConstraints>,
    args: UpdateFieldArgs,
) -> Result<()> {
    // `AccountView` is Copy, and a copy still points at the same
    // account — v2's typed handles make the aliasing a compile error.
    let authority_view = *context.accounts.authority.account();
    let UpdateFieldArgs { field, value } = args;

    // Convert to Field type from spl_token_metadata_interface
    let field = field.to_spl_field();
    msg!("Field: {:?}, Value: {}", field, value);

    let (current_lamports, required_lamports) = {
        // Get the current state of the mint account. `AccountView` is Copy and
        // a copy still points at the same backing buffer.
        let mint = *context.accounts.mint_account.account();
        let buffer = mint.try_borrow()?;
        let state = PodStateWithExtensions::<PodMint>::unpack(&buffer)?;

        // Get and update the token metadata
        let mut token_metadata = state.get_variable_len_extension::<TokenMetadata>()?;
        token_metadata.update(field.clone(), value.clone());
        msg!("Updated TokenMetadata: {:?}", token_metadata);

        // Calculate the new account length with the updated metadata
        let new_account_len =
            state.try_get_new_account_len_for_variable_len_extension(&token_metadata)?;

        // Calculate the required lamports for the new account length
        let required_lamports = Rent::get()?.try_minimum_balance(new_account_len)?;
        // Get the current lamports of the mint account
        let current_lamports = mint.lamports();

        msg!("Required lamports: {}", required_lamports);
        msg!("Current lamports: {}", current_lamports);

        (current_lamports, required_lamports)
    };

    // Transfer lamports to mint account for the additional metadata if needed
    if required_lamports > current_lamports {
        let lamport_difference = required_lamports - current_lamports;
        transfer(
            CpiContext::new(
                context.accounts.system_program.address(),
                Transfer {
                    from: context.accounts.authority.cpi_handle_mut(),
                    to: context.accounts.mint_account.cpi_handle_mut(),
                },
            ),
            lamport_difference,
        )?;
        msg!(
            "Transferring {} lamports to metadata account",
            lamport_difference
        );
    }

    // Update token metadata
    token_metadata_update_field(
        CpiContext::new(
            context.accounts.token_program.address(),
            TokenMetadataUpdateField {
                metadata: context.accounts.mint_account.cpi_handle_mut(),
                update_authority: CpiHandle::readonly(&authority_view),
            },
        ),
        field,
        value,
    )?;
    Ok(())
}

// Custom struct to implement AnchorSerialize and AnchorDeserialize
// This is required to pass the struct as an argument to the instruction
#[derive(IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct UpdateFieldArgs {
    /// Field to update in the metadata
    pub field: AnchorField,
    /// Value to write for the field
    pub value: String,
}

// Need to do this so the enum shows up in the IDL
#[derive(Debug, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub enum AnchorField {
    /// The name field, corresponding to `TokenMetadata.name`
    Name,
    /// The symbol field, corresponding to `TokenMetadata.symbol`
    Symbol,
    /// The uri field, corresponding to `TokenMetadata.uri`
    Uri,
    /// A custom field, whose key is given by the associated string
    Key(String),
}

// Convert AnchorField to Field from spl_token_metadata_interface
impl AnchorField {
    fn to_spl_field(&self) -> Field {
        match self {
            AnchorField::Name => Field::Name,
            AnchorField::Symbol => Field::Symbol,
            AnchorField::Uri => Field::Uri,
            AnchorField::Key(s) => Field::Key(s.clone()),
        }
    }
}
