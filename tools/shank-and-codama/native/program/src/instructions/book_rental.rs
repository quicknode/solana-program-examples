use crate::state::{RentalOrder, RentalOrderStatus};
use {
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        program::invoke_signed,
        pubkey::Pubkey,
        rent::Rent,
        sysvar::Sysvar,
    },
    solana_system_interface::instruction as system_instruction,
};

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug)]
pub struct BookRentalArgs {
    pub name: String,
    pub pick_up_date: String,
    pub return_date: String,
    pub price: u64,
}

pub fn book_rental(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: BookRentalArgs,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let rental_order_account = next_account_info(accounts_iter)?;
    let car_account = next_account_info(accounts_iter)?;
    let payer = next_account_info(accounts_iter)?;
    let system_program = next_account_info(accounts_iter)?;

    let (rental_order_account_pda, rental_order_account_bump) =
        RentalOrder::find_pda(program_id, car_account.key, payer.key);
    assert!(&rental_order_account_pda == rental_order_account.key);

    let rental_order_data = RentalOrder {
        car: *car_account.key,
        name: args.name,
        pick_up_date: args.pick_up_date,
        return_date: args.return_date,
        price: args.price,
        status: RentalOrderStatus::Created,
    };

    let account_span = borsh::to_vec(&rental_order_data)?.len();
    let lamports_required = (Rent::get()?).minimum_balance(account_span);

    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            rental_order_account.key,
            lamports_required,
            account_span as u64,
            program_id,
        ),
        &[
            payer.clone(),
            rental_order_account.clone(),
            system_program.clone(),
        ],
        &[&[
            RentalOrder::SEED_PREFIX.as_bytes(),
            car_account.key.as_ref(),
            payer.key.as_ref(),
            &[rental_order_account_bump],
        ]],
    )?;

    rental_order_data.serialize(&mut &mut rental_order_account.data.borrow_mut()[..])?;

    Ok(())
}
