use {
    crate::{error::*, state::*, utils::assert_is_associated_token_account},
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo,
        entrypoint::ProgramResult,
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        program_pack::Pack,
        pubkey::Pubkey,
        rent::Rent,
        sysvar::Sysvar,
    },
    solana_system_interface::instruction as system_instruction,
    spl_associated_token_account_interface::instruction as associated_token_account_instruction,
    spl_token_interface::{
        instruction as token_instruction,
        state::{Account as TokenAccount, Mint},
    },
};

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct MakeOffer {
    pub id: u64,
    pub token_a_offered_amount: u64,
    pub token_b_wanted_amount: u64,
}

impl MakeOffer {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo<'_>],
        args: MakeOffer,
    ) -> ProgramResult {
        let [
            offer_info,
            token_mint_a,
            token_mint_b,
            maker_token_account_a,
            maker_token_account_b,
            vault,
            maker,
            token_program,
            associated_token_program,
            system_program
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // The maker signs and pays the rent for every account created here
        // (the offer account, the vault, and the maker's token B account).
        // take_offer and cancel_offer later close those accounts back to the
        // maker, so the rent always returns to the party who paid it.
        if !maker.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let offer_seeds = &[
            Offer::SEED_PREFIX,
            maker.key.as_ref(),
            &args.id.to_le_bytes(),
        ];

        let (offer_key, bump) = Pubkey::find_program_address(offer_seeds, program_id);

        if *offer_info.key != offer_key {
            return Err(EscrowError::OfferKeyMismatch.into());
        };

        // The vault is the offer PDA's associated token account for mint A.
        assert_is_associated_token_account(vault.key, offer_info.key, token_mint_a.key)?;

        // The maker's token B account receives tokens when the offer is taken.
        // Create it now (paid by the maker) so take_offer never has to create
        // an account whose rent would fall on the taker.
        assert_is_associated_token_account(maker_token_account_b.key, maker.key, token_mint_b.key)?;

        let offer = Offer {
            bump,
            maker: *maker.key,
            id: args.id,
            token_b_wanted_amount: args.token_b_wanted_amount,
            token_mint_a: *token_mint_a.key,
            token_mint_b: *token_mint_b.key,
        };

        let size = borsh::to_vec::<Offer>(&offer)?.len();
        let lamports_required = (Rent::get()?).minimum_balance(size);

        // Create the offer account, rent paid by the maker.
        invoke_signed(
            &system_instruction::create_account(
                maker.key,
                offer_info.key,
                lamports_required,
                size as u64,
                program_id,
            ),
            &[maker.clone(), offer_info.clone(), system_program.clone()],
            &[&[
                Offer::SEED_PREFIX,
                maker.key.as_ref(),
                args.id.to_le_bytes().as_ref(),
                &[bump],
            ]],
        )?;

        // Create the vault token account, rent paid by the maker.
        invoke(
            &associated_token_account_instruction::create_associated_token_account(
                maker.key,
                offer_info.key,
                token_mint_a.key,
                token_program.key,
            ),
            &[
                token_mint_a.clone(),
                vault.clone(),
                offer_info.clone(),
                maker.clone(),
                system_program.clone(),
                token_program.clone(),
                associated_token_program.clone(),
            ],
        )?;

        // Create the maker's token B account if it does not exist yet, rent
        // paid by the maker.
        if maker_token_account_b.lamports() == 0 {
            invoke(
                &associated_token_account_instruction::create_associated_token_account(
                    maker.key,
                    maker.key,
                    token_mint_b.key,
                    token_program.key,
                ),
                &[
                    token_mint_b.clone(),
                    maker_token_account_b.clone(),
                    maker.clone(),
                    maker.clone(),
                    system_program.clone(),
                    token_program.clone(),
                    associated_token_program.clone(),
                ],
            )?;
        }

        // Move the offered mint A tokens into the vault.
        //
        // `transfer` is deprecated in favour of `transfer_checked`, which also
        // verifies the mint and its decimals. Read the decimals from the mint
        // account the caller passed in.
        let mint_a_decimals = Mint::unpack(&token_mint_a.data.borrow())?.decimals;
        invoke(
            &token_instruction::transfer_checked(
                token_program.key,
                maker_token_account_a.key,
                token_mint_a.key,
                vault.key,
                maker.key,
                &[maker.key],
                args.token_a_offered_amount,
                mint_a_decimals,
            )?,
            &[
                token_program.clone(),
                maker_token_account_a.clone(),
                token_mint_a.clone(),
                vault.clone(),
                maker.clone(),
            ],
        )?;

        // Conservation check: the vault must now hold exactly the offered
        // amount.
        let vault_token_amount = TokenAccount::unpack(&vault.data.borrow())?.amount;
        if vault_token_amount != args.token_a_offered_amount {
            return Err(EscrowError::TokenConservationViolation.into());
        }

        offer.serialize(&mut *offer_info.data.borrow_mut())?;

        Ok(())
    }
}
