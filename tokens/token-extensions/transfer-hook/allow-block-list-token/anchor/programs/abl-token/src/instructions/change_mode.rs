use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::token_interface::spl_token_metadata_interface::state::TokenMetadata;
use anchor_spl::{
    mint,
    token_2022::{
        spl_token_2022::extension::{BaseStateWithExtensions, StateWithExtensions},
        spl_token_2022::state::Mint,
        Token2022,
    },
    token_interface::{
        spl_token_metadata_interface::state::Field, token_metadata_update_field,
        Mint as MintAccount, TokenMetadataUpdateField,
    },
};

use crate::Mode;

#[derive(Accounts)]
pub struct ChangeModeAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(
        mut,
        mint::token_program = token_program,
    )]
    pub mint: InterfaceAccount<MintAccount>,

    pub token_program: Program<Token2022>,

    pub system_program: Program<System>,
}

#[derive(IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct ChangeModeArgs {
    pub mode: Mode,
    pub threshold: u64,
}

impl ChangeModeAccountConstraints {
    pub fn change_mode(&mut self, args: ChangeModeArgs) -> Result<()> {
        let cpi_accounts = TokenMetadataUpdateField {
            metadata: self.mint.cpi_handle_mut(),
            update_authority: self.authority.cpi_handle(),
        };
        let cpi_program = self.token_program.address();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        token_metadata_update_field(cpi_ctx, Field::Key("AB".to_string()), args.mode.to_string())?;

        if args.mode == Mode::Mixed || self.has_threshold()? {
            let threshold = if args.mode == Mode::Mixed {
                args.threshold
            } else {
                0
            };

            let cpi_accounts = TokenMetadataUpdateField {
                metadata: self.mint.cpi_handle_mut(),
                update_authority: self.authority.cpi_handle(),
            };
            let cpi_program = self.token_program.address();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

            token_metadata_update_field(
                cpi_ctx,
                Field::Key("threshold".to_string()),
                threshold.to_string(),
            )?;
        }

        // Writing the metadata grew the mint, so top it back up to rent exemption.
        let data_len = self.mint.account().data_len();
        let min_balance = Rent::get()?.try_minimum_balance(data_len)?;
        let current = self.mint.account().lamports();
        if min_balance > current {
            transfer(
                CpiContext::new(
                    self.system_program.address(),
                    Transfer {
                        from: self.authority.cpi_handle_mut(),
                        to: self.mint.cpi_handle_mut(),
                    },
                ),
                min_balance - current,
            )?;
        }

        Ok(())
    }

    fn has_threshold(&self) -> Result<bool> {
        let mint_data = self.mint.account().try_borrow()?;
        let mint = StateWithExtensions::<Mint>::unpack(&mint_data)?;
        let metadata = mint.get_variable_len_extension::<TokenMetadata>();
        Ok(metadata.is_ok()
            && metadata
                .unwrap()
                .additional_metadata
                .iter()
                .any(|(key, _)| key == "threshold"))
    }
}
