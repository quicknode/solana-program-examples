use anchor_lang::{
    prelude::*, solana_program::program::invoke, solana_program::system_instruction::transfer,
};
use anchor_spl::{
    token_2022::Token2022,
    token_interface::{
        spl_token_metadata_interface::state::Field, token_metadata_initialize,
        token_metadata_update_field, Mint, TokenMetadataInitialize, TokenMetadataUpdateField,
    },
};

use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{get_extra_account_metas, get_meta_list_size, Mode, META_LIST_ACCOUNT_SEED};

#[derive(Accounts)]
#[instruction(args: InitMintArgs)]
pub struct InitMintAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        init,
        payer = payer,
        mint::token_program = token_program,
        mint::decimals = args.decimals(),
        mint::authority = payer,
        mint::freeze_authority = args.freeze_authority,
        extensions::permanent_delegate::delegate = args.permanent_delegate,
        extensions::transfer_hook::authority = args.transfer_hook_authority,
        extensions::transfer_hook::program_id = crate::id(),
        extensions::metadata_pointer::authority = payer.address(),
        extensions::metadata_pointer::metadata_address = mint.address(),
    )]
    pub mint: Box<InterfaceAccount<Mint>>,

    #[account(
        init,
        space = get_meta_list_size()?,
        seeds = [META_LIST_ACCOUNT_SEED, mint.address().as_ref()],
        bump,
        payer = payer,
    )]
    /// CHECK: extra metas account
    pub extra_metas_account: UncheckedAccount,

    pub system_program: Program<System>,

    pub token_program: Program<Token2022>,
}

impl InitMintAccountConstraints<'_> {
    pub fn init_mint(&mut self, args: InitMintArgs) -> Result<()> {
        let cpi_accounts = TokenMetadataInitialize {
            program_id: self.token_program.cpi_handle_mut(),
            mint: self.mint.cpi_handle_mut(),
            metadata: self.mint.cpi_handle_mut(), // metadata account is the mint, since data is stored in mint
            mint_authority: self.payer.cpi_handle_mut(),
            update_authority: self.payer.cpi_handle_mut(),
        };
        let cpi_ctx = CpiContext::new(self.token_program.address(), cpi_accounts);
        token_metadata_initialize(cpi_ctx, args.name, args.symbol, args.uri)?;

        let cpi_accounts = TokenMetadataUpdateField {
            metadata: self.mint.cpi_handle_mut(),
            update_authority: self.payer.cpi_handle_mut(),
            program_id: self.token_program.cpi_handle_mut(),
        };

        let cpi_ctx = CpiContext::new(self.token_program.address(), cpi_accounts);

        token_metadata_update_field(cpi_ctx, Field::Key("AB".to_string()), args.mode.to_string())?;

        if args.mode == Mode::Mixed {
            let cpi_accounts = TokenMetadataUpdateField {
                metadata: self.mint.cpi_handle_mut(),
                update_authority: self.payer.cpi_handle_mut(),
                program_id: self.token_program.cpi_handle_mut(),
            };
            let cpi_ctx = CpiContext::new(self.token_program.address(), cpi_accounts);

            token_metadata_update_field(
                cpi_ctx,
                Field::Key("threshold".to_string()),
                args.threshold.to_string(),
            )?;
        }

        let data = self.mint.cpi_handle_mut().data_len();
        let min_balance = Rent::get()?.try_minimum_balance(data)?;
        if min_balance > self.mint.cpi_handle_mut().get_lamports() {
            invoke(
                &transfer(
                    &self.payer.address(),
                    &self.mint.cpi_handle_mut().address(),
                    min_balance - self.mint.cpi_handle_mut().get_lamports(),
                ),
                &[
                    self.payer.cpi_handle_mut(),
                    self.mint.cpi_handle_mut(),
                    self.system_program.cpi_handle_mut(),
                ],
            )?;
        }

        // initialize the extra metas account
        let extra_metas_account = &self.extra_metas_account;
        let metas = get_extra_account_metas()?;
        let mut data = extra_metas_account.try_borrow_mut_data()?;
        ExtraAccountMetaList::init::<ExecuteInstruction>(&mut data, &metas)
            .map_err(|_| ProgramError::InvalidAccountData)?;

        Ok(())
    }
}

#[derive(IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct InitMintArgs {
    pub decimals: u8,
    pub mint_authority: Address,
    pub freeze_authority: Address,
    pub permanent_delegate: Address,
    pub transfer_hook_authority: Address,
    pub mode: Mode,
    pub threshold: u64,
    pub name: String,
    pub symbol: String,
    pub uri: String,
}
