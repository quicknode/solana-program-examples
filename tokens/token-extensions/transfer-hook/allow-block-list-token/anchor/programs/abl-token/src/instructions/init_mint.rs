use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, transfer, CreateAccount, Transfer};
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{extension::ExtensionType, pod::PodMint},
        InitializeMint2, Token2022,
    },
    token_2022_extensions::{
        metadata_pointer::{metadata_pointer_initialize, MetadataPointerInitialize},
        permanent_delegate::{permanent_delegate_initialize, PermanentDelegateInitialize},
        transfer_hook::{transfer_hook_initialize, TransferHookInitialize},
    },
    token_interface::{
        spl_token_metadata_interface::state::Field, token_metadata_initialize,
        token_metadata_update_field, TokenMetadataInitialize, TokenMetadataUpdateField,
    },
};

use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{get_extra_account_metas, get_meta_list_size, Mode, META_LIST_ACCOUNT_SEED};

#[derive(Accounts)]
// The leading underscore is for rustc: `#[derive(Accounts)]` expands `_args`
// into a path that never reads it, so the plain name warns as unused.
#[instruction(_args: InitMintArgs)]
pub struct InitMintAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    /// CHECK: created and initialized by this instruction as a Token-2022 mint
    /// carrying the PermanentDelegate, TransferHook and MetadataPointer
    /// extensions. anchor-spl has no init constraints for those, so the mint is
    /// built by hand in `init_mint` below.
    #[account(mut)]
    pub mint: Signer,

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

impl InitMintAccountConstraints {
    pub fn init_mint(&mut self, args: InitMintArgs) -> Result<()> {
        let mint_address = *self.mint.address();
        let payer_address = *self.payer.address();

        // Allocate the mint with room for all three extensions, initialize each
        // extension, then initialize the mint data. Extension initialization has
        // to happen before InitializeMint2.
        let mint_size = ExtensionType::try_calculate_account_len::<PodMint>(&[
            ExtensionType::PermanentDelegate,
            ExtensionType::TransferHook,
            ExtensionType::MetadataPointer,
        ])?;
        let lamports = Rent::get()?.try_minimum_balance(mint_size)?;

        create_account(
            CpiContext::new(
                self.system_program.address(),
                CreateAccount {
                    from: self.payer.cpi_handle_mut(),
                    to: self.mint.cpi_handle_mut(),
                },
            ),
            lamports,
            mint_size as u64,
            self.token_program.address(),
        )?;

        permanent_delegate_initialize(
            CpiContext::new(
                self.token_program.address(),
                PermanentDelegateInitialize {
                    mint: self.mint.cpi_handle_mut(),
                },
            ),
            &args.permanent_delegate,
        )?;

        transfer_hook_initialize(
            CpiContext::new(
                self.token_program.address(),
                TransferHookInitialize {
                    mint: self.mint.cpi_handle_mut(),
                },
            ),
            Some(&args.transfer_hook_authority),
            Some(&crate::ID),
        )?;

        // metadata is stored in the mint itself, so the pointer points at it
        metadata_pointer_initialize(
            CpiContext::new(
                self.token_program.address(),
                MetadataPointerInitialize {
                    mint: self.mint.cpi_handle_mut(),
                },
            ),
            Some(&payer_address),
            Some(&mint_address),
        )?;

        initialize_mint2(
            CpiContext::new(
                self.token_program.address(),
                InitializeMint2 {
                    mint: self.mint.cpi_handle_mut(),
                },
            ),
            args.decimals,
            &payer_address,
            Some(&args.freeze_authority),
        )?;

        // `payer` and `mint` each fill more than one CPI slot below. v2's typed
        // handles enforce borrow exclusivity at compile time, so the read-only
        // slots are built from copies of the `AccountView`, and each copy still
        // points at the same underlying account.
        let payer_view = *self.payer.account();
        let mint_view = *self.mint.account();

        let cpi_accounts = TokenMetadataInitialize {
            // metadata account is the mint, since data is stored in mint
            metadata: self.mint.cpi_handle_mut(),
            update_authority: CpiHandle::readonly(&payer_view),
            mint: CpiHandle::readonly(&mint_view),
            mint_authority: CpiHandle::readonly(&payer_view),
        };
        let cpi_ctx = CpiContext::new(self.token_program.address(), cpi_accounts);
        token_metadata_initialize(cpi_ctx, args.name, args.symbol, args.uri)?;

        let cpi_accounts = TokenMetadataUpdateField {
            metadata: self.mint.cpi_handle_mut(),
            update_authority: CpiHandle::readonly(&payer_view),
        };
        let cpi_ctx = CpiContext::new(self.token_program.address(), cpi_accounts);

        token_metadata_update_field(cpi_ctx, Field::Key("AB".to_string()), args.mode.to_string())?;

        if args.mode == Mode::Mixed {
            let cpi_accounts = TokenMetadataUpdateField {
                metadata: self.mint.cpi_handle_mut(),
                update_authority: CpiHandle::readonly(&payer_view),
            };
            let cpi_ctx = CpiContext::new(self.token_program.address(), cpi_accounts);

            token_metadata_update_field(
                cpi_ctx,
                Field::Key("threshold".to_string()),
                args.threshold.to_string(),
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
                        from: self.payer.cpi_handle_mut(),
                        to: self.mint.cpi_handle_mut(),
                    },
                ),
                min_balance - current,
            )?;
        }

        // initialize the extra metas account
        let metas = get_extra_account_metas()?;
        let mut extra_metas_view = *self.extra_metas_account.account();
        let mut data = extra_metas_view.try_borrow_mut()?;
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
