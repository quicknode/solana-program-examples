use crate::state::{RentalOrder, RentalOrderStatus};
use {
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        pubkey::Pubkey,
    },
};

pub fn return_car(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let rental_order_account = next_account_info(accounts_iter)?;
    let car_account = next_account_info(accounts_iter)?;
    let payer = next_account_info(accounts_iter)?;

    let (rental_order_account_pda, _) =
        RentalOrder::find_pda(program_id, car_account.key, payer.key);
    assert!(&rental_order_account_pda == rental_order_account.key);

    let rental_order = &mut RentalOrder::try_from_slice(&rental_order_account.data.borrow())?;
    rental_order.status = RentalOrderStatus::Returned;
    rental_order.serialize(&mut &mut rental_order_account.data.borrow_mut()[..])?;

    Ok(())
}
