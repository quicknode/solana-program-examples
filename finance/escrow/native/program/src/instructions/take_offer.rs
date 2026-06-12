use {
    crate::{error::*, state::*, utils::*},
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
    },
    spl_associated_token_account_interface::instruction as associated_token_account_instruction,
    spl_token_interface::{
        instruction as token_instruction,
        state::{Account as TokenAccount, Mint},
    },
};

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct TakeOffer {}

impl TakeOffer {
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo<'_>]) -> ProgramResult {
        let [offer_info, token_mint_a, token_mint_b, maker_token_account_b, taker_token_account_a, taker_token_account_b, vault, maker, taker, token_program, associated_token_program, system_program] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // The taker signs the instruction.
        if !taker.is_signer {
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
        if &offer.token_mint_b != token_mint_b.key {
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

        // Validate receiving addresses, including the vault (the offer PDA's
        // associated token account for mint A).
        assert_is_associated_token_account(maker_token_account_b.key, maker.key, token_mint_b.key)?;
        assert_is_associated_token_account(taker_token_account_a.key, taker.key, token_mint_a.key)?;
        assert_is_associated_token_account(vault.key, offer_info.key, token_mint_a.key)?;

        // Create the taker's token A account if needed. The taker pays this
        // rent: it is the taker's own account.
        if taker_token_account_a.lamports() == 0 {
            invoke(
                &associated_token_account_instruction::create_associated_token_account(
                    taker.key,
                    taker.key,
                    token_mint_a.key,
                    token_program.key,
                ),
                &[
                    token_mint_a.clone(),
                    taker_token_account_a.clone(),
                    taker.clone(),
                    taker.clone(),
                    system_program.clone(),
                    token_program.clone(),
                    associated_token_program.clone(),
                ],
            )?;
        }

        // The maker's token B account was created in make_offer (rent paid by
        // the maker). Require it to exist rather than creating it here, which
        // would make the taker pay rent for the maker's account.
        if maker_token_account_b.lamports() == 0 {
            return Err(EscrowError::MakerTokenAccountBNotInitialized.into());
        }

        let vault_amount_a = TokenAccount::unpack(&vault.data.borrow())?.amount;
        let taker_amount_a_before_transfer =
            TokenAccount::unpack(&taker_token_account_a.data.borrow())?.amount;
        let maker_amount_b_before_transfer =
            TokenAccount::unpack(&maker_token_account_b.data.borrow())?.amount;
        let taker_amount_b = TokenAccount::unpack(&taker_token_account_b.data.borrow())?.amount;

        solana_program::msg!("Vault A Balance Before Transfer: {}", vault_amount_a);
        solana_program::msg!(
            "Taker A Balance Before Transfer: {}",
            taker_amount_a_before_transfer
        );
        solana_program::msg!(
            "Maker B Balance Before Transfer: {}",
            maker_amount_b_before_transfer
        );
        solana_program::msg!("Taker B Balance Before Transfer: {}", taker_amount_b);

        // `transfer` is deprecated in favour of `transfer_checked`, which also
        // verifies the mint and its decimals. Read the decimals from the mint
        // accounts the caller passed in.
        let mint_a_decimals = Mint::unpack(&token_mint_a.data.borrow())?.decimals;
        let mint_b_decimals = Mint::unpack(&token_mint_b.data.borrow())?.decimals;

        // The taker transfers mint B tokens to the maker.
        invoke(
            &token_instruction::transfer_checked(
                token_program.key,
                taker_token_account_b.key,
                token_mint_b.key,
                maker_token_account_b.key,
                taker.key,
                &[taker.key],
                offer.token_b_wanted_amount,
                mint_b_decimals,
            )?,
            &[
                token_program.clone(),
                taker_token_account_b.clone(),
                token_mint_b.clone(),
                maker_token_account_b.clone(),
                taker.clone(),
            ],
        )?;

        // The vault releases its mint A tokens to the taker, signed by the
        // offer PDA.
        invoke_signed(
            &token_instruction::transfer_checked(
                token_program.key,
                vault.key,
                token_mint_a.key,
                taker_token_account_a.key,
                offer_info.key,
                &[offer_info.key, taker.key],
                vault_amount_a,
                mint_a_decimals,
            )?,
            &[
                token_mint_a.clone(),
                vault.clone(),
                taker_token_account_a.clone(),
                offer_info.clone(),
                taker.clone(),
                token_program.clone(),
            ],
            &[offer_signer_seeds],
        )?;

        // Conservation check: the taker gained exactly the vault's mint A
        // balance and the maker gained exactly the wanted mint B amount.
        let taker_amount_a = TokenAccount::unpack(&taker_token_account_a.data.borrow())?.amount;
        let maker_amount_b = TokenAccount::unpack(&maker_token_account_b.data.borrow())?.amount;

        let expected_taker_amount_a = taker_amount_a_before_transfer
            .checked_add(vault_amount_a)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        let expected_maker_amount_b = maker_amount_b_before_transfer
            .checked_add(offer.token_b_wanted_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;

        if taker_amount_a != expected_taker_amount_a {
            return Err(EscrowError::TokenConservationViolation.into());
        }
        if maker_amount_b != expected_maker_amount_b {
            return Err(EscrowError::TokenConservationViolation.into());
        }

        let taker_amount_b = TokenAccount::unpack(&taker_token_account_b.data.borrow())?.amount;
        let vault_amount_a = TokenAccount::unpack(&vault.data.borrow())?.amount;

        solana_program::msg!("Vault A Balance After Transfer: {}", vault_amount_a);
        solana_program::msg!("Taker A Balance After Transfer: {}", taker_amount_a);
        solana_program::msg!("Maker B Balance After Transfer: {}", maker_amount_b);
        solana_program::msg!("Taker B Balance After Transfer: {}", taker_amount_b);

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
