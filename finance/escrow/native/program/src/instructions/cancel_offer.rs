use {
    crate::{error::*, state::*, utils::*},
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo, entrypoint::ProgramResult, program::invoke_signed,
        program_error::ProgramError, program_pack::Pack, pubkey::Pubkey,
    },
    spl_token_interface::{
        instruction as token_instruction,
        state::{Account as TokenAccount, Mint},
    },
};

// Cancel an outstanding offer. Without this handler, an abandoned offer would
// keep the maker's mint A tokens locked in the vault forever (and the offer
// account's rent unclaimed). Only the maker can cancel: the vault tokens flow
// back to the maker's token A account, and the vault and offer accounts are
// closed with their rent refunded to the maker.
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct CancelOffer {}

impl CancelOffer {
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo<'_>]) -> ProgramResult {
        let [offer_info, token_mint_a, maker_token_account_a, vault, maker, token_program, system_program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Only the maker may cancel their offer.
        if !maker.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let offer = Offer::try_from_slice(&offer_info.data.borrow()[..])?;

        // Validate the passed accounts against the stored offer state.
        if &offer.maker != maker.key {
            return Err(EscrowError::MakerMismatch.into());
        }
        if &offer.token_mint_a != token_mint_a.key {
            return Err(EscrowError::MintMismatch.into());
        }

        // Validate the offer account with its signer seeds.
        let offer_signer_seeds = &[
            Offer::SEED_PREFIX,
            maker.key.as_ref(),
            &offer.id.to_le_bytes(),
            &[offer.bump],
        ];

        let offer_key = Pubkey::create_program_address(offer_signer_seeds, program_id)?;

        if *offer_info.key != offer_key {
            return Err(EscrowError::OfferKeyMismatch.into());
        };

        // The receiving account is the maker's own token A account, and the
        // vault is the offer PDA's associated token account for mint A.
        assert_is_associated_token_account(maker_token_account_a.key, maker.key, token_mint_a.key)?;
        assert_is_associated_token_account(vault.key, offer_info.key, token_mint_a.key)?;

        let vault_amount_a = TokenAccount::unpack(&vault.data.borrow())?.amount;
        let maker_amount_a_before_transfer =
            TokenAccount::unpack(&maker_token_account_a.data.borrow())?.amount;

        // `transfer` is deprecated in favour of `transfer_checked`, which also
        // verifies the mint and its decimals. Read the decimals from the mint
        // account the caller passed in.
        let mint_a_decimals = Mint::unpack(&token_mint_a.data.borrow())?.decimals;

        // The vault returns its mint A tokens to the maker, signed by the
        // offer PDA.
        invoke_signed(
            &token_instruction::transfer_checked(
                token_program.key,
                vault.key,
                token_mint_a.key,
                maker_token_account_a.key,
                offer_info.key,
                &[offer_info.key],
                vault_amount_a,
                mint_a_decimals,
            )?,
            &[
                token_mint_a.clone(),
                vault.clone(),
                maker_token_account_a.clone(),
                offer_info.clone(),
                token_program.clone(),
            ],
            &[offer_signer_seeds],
        )?;

        // Conservation check: the maker got back exactly what the vault held.
        let maker_amount_a = TokenAccount::unpack(&maker_token_account_a.data.borrow())?.amount;
        let expected_maker_amount_a = maker_amount_a_before_transfer
            .checked_add(vault_amount_a)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if maker_amount_a != expected_maker_amount_a {
            return Err(EscrowError::TokenConservationViolation.into());
        }

        // Close the vault and the offer account. The maker paid the rent for
        // both in make_offer, so both refunds go to the maker.
        invoke_signed(
            &token_instruction::close_account(
                token_program.key,
                vault.key,
                maker.key,
                offer_info.key,
                &[],
            )?,
            &[vault.clone(), maker.clone(), offer_info.clone()],
            &[offer_signer_seeds],
        )?;

        close_offer_account(offer_info, maker, system_program)?;

        Ok(())
    }
}
