use crate::{
    error::CarRentalError,
    state::{RentalOrder, RentalOrderStatus},
};
use {
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        pubkey::Pubkey,
    },
};

pub fn pick_up_car(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let rental_order_account = next_account_info(accounts_iter)?;
    let car_account = next_account_info(accounts_iter)?;
    let payer = next_account_info(accounts_iter)?;

    // The rental PDA is derived from the payer's key, so the payer must sign:
    // otherwise anyone could pick up (and later return) someone else's rental
    // just by naming the victim as `payer`.
    if !payer.is_signer {
        return Err(CarRentalError::PayerSignatureMissing.into());
    }

    // Only deserialize accounts this program owns.
    if rental_order_account.owner != program_id {
        return Err(CarRentalError::RentalAccountNotOwnedByProgram.into());
    }

    let (rental_order_account_pda, _) =
        RentalOrder::find_pda(program_id, car_account.key, payer.key);
    if &rental_order_account_pda != rental_order_account.key {
        return Err(CarRentalError::RentalAccountAddressMismatch.into());
    }

    let rental_order = &mut RentalOrder::try_from_slice(&rental_order_account.data.borrow())?;

    // Valid lifecycle: Created -> PickedUp -> Returned.
    if rental_order.status != RentalOrderStatus::Created {
        return Err(CarRentalError::RentalNotInCreatedStatus.into());
    }

    rental_order.status = RentalOrderStatus::PickedUp;
    rental_order.serialize(&mut &mut rental_order_account.data.borrow_mut()[..])?;

    Ok(())
}
