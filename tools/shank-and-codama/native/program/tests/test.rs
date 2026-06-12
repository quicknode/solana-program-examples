// In-process integration test for the car rental service program.
//
// Runs entirely in CI with no network: the program .so is loaded into a
// LiteSVM instance and exercised through the Codama-generated Rust client
// (clients/rust). It walks the full rental lifecycle (add_car, book_rental,
// pick_up_car, return_car), asserting onchain account state after each step,
// and verifies the program's account validation: a non-signing payer, a
// rental account owned by the wrong program, and an invalid status transition
// are all rejected.

use car_rental_service_client::generated::{
    accounts::{Car, RentalOrder},
    instructions::{
        AddCar, AddCarInstructionArgs, BookRental, BookRentalInstructionArgs, PickUpCar, ReturnCar,
    },
    programs::CAR_RENTAL_SERVICE_ID,
    types::RentalOrderStatus,
};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_pubkey::Pubkey;
use solana_transaction::Transaction;

// The .so is built into program/target/deploy by
// `cargo build-sbf --manifest-path=./program/Cargo.toml` (run from the
// project root; `pnpm build` runs the same command). Rebuild after every
// program change: the binary is embedded at test-compile time, so a stale
// .so silently tests old code.
const PROGRAM_SO: &[u8] = include_bytes!("../target/deploy/car_rental_service.so");

// Custom error codes from program/src/error.rs (CarRentalError). The enum
// starts at 6000, matching Anchor's custom-error offset.
const ERROR_PAYER_SIGNATURE_MISSING: u32 = 6002;
const ERROR_RENTAL_ACCOUNT_NOT_OWNED_BY_PROGRAM: u32 = 6003;
const ERROR_RENTAL_NOT_IN_PICKED_UP_STATUS: u32 = 6005;

const SYSTEM_PROGRAM_ID: Pubkey = Pubkey::new_from_array([0u8; 32]);

fn load_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    svm.add_program(CAR_RENTAL_SERVICE_ID, PROGRAM_SO).unwrap();
    svm
}

fn funded_signer(svm: &mut LiteSVM) -> Keypair {
    let signer = Keypair::new();
    svm.airdrop(&signer.pubkey(), 10_000_000_000).unwrap();
    signer
}

fn car_pda(make: &str, model: &str) -> Pubkey {
    Pubkey::find_program_address(
        &[b"car", make.as_bytes(), model.as_bytes()],
        &CAR_RENTAL_SERVICE_ID,
    )
    .0
}

fn rental_pda(car: &Pubkey, payer: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"rental_order", car.as_ref(), payer.as_ref()],
        &CAR_RENTAL_SERVICE_ID,
    )
    .0
}

fn send_instruction(svm: &mut LiteSVM, payer: &Keypair, instruction: Instruction) {
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );
    if let Err(failure) = svm.send_transaction(transaction) {
        panic!(
            "transaction failed: {:?}\n{}",
            failure.err,
            failure.meta.logs.join("\n")
        );
    }
}

// Assert that sending `instruction` fails with the given custom error code.
// The runtime logs custom errors as hex, e.g. "custom program error: 0x1772".
fn send_expecting_custom_error(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: Instruction,
    error_code: u32,
) {
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );
    match svm.send_transaction(transaction) {
        Ok(_) => panic!("expected custom program error {error_code}, transaction succeeded"),
        Err(failure) => {
            let error_code_hex = format!("0x{error_code:x}");
            let logs = failure.meta.logs.join("\n");
            assert!(
                logs.contains(&error_code_hex),
                "expected custom program error {error_code} ({error_code_hex}) in logs, got:\n{logs}"
            );
        }
    }
}

fn fetch_rental_order(svm: &LiteSVM, rental_account: &Pubkey) -> RentalOrder {
    let account = svm.get_account(rental_account).unwrap();
    RentalOrder::from_bytes(&account.data).unwrap()
}

fn add_car_instruction(payer: &Keypair, make: &str, model: &str, year: u16) -> Instruction {
    AddCar {
        car_account: car_pda(make, model),
        payer: payer.pubkey(),
        system_program: SYSTEM_PROGRAM_ID,
    }
    .instruction(AddCarInstructionArgs {
        year,
        make: make.to_string(),
        model: model.to_string(),
    })
}

fn book_rental_instruction(
    payer: &Keypair,
    car_account: &Pubkey,
    name: &str,
    price: u64,
) -> Instruction {
    BookRental {
        rental_account: rental_pda(car_account, &payer.pubkey()),
        car_account: *car_account,
        payer: payer.pubkey(),
        system_program: SYSTEM_PROGRAM_ID,
    }
    .instruction(BookRentalInstructionArgs {
        name: name.to_string(),
        pick_up_date: "01/28/2023 8:00 AM".to_string(),
        return_date: "01/28/2023 10:00 PM".to_string(),
        price,
    })
}

#[test]
fn full_lifecycle_add_book_pick_up_return() {
    let mut svm = load_svm();
    let payer = funded_signer(&mut svm);

    // 1. add_car
    let make = "BMW";
    let model = "iX1";
    let car_account = car_pda(make, model);
    send_instruction(
        &mut svm,
        &payer,
        add_car_instruction(&payer, make, model, 2020),
    );

    let car = Car::from_bytes(&svm.get_account(&car_account).unwrap().data).unwrap();
    assert_eq!(car.year, 2020);
    assert_eq!(car.make, make);
    assert_eq!(car.model, model);

    // 2. book_rental
    let rental_account = rental_pda(&car_account, &payer.pubkey());
    send_instruction(
        &mut svm,
        &payer,
        book_rental_instruction(&payer, &car_account, "Fred Flintstone", 300),
    );
    let rental = fetch_rental_order(&svm, &rental_account);
    assert_eq!(rental.name, "Fred Flintstone");
    assert_eq!(rental.car, car_account);
    assert_eq!(rental.price, 300);
    assert_eq!(rental.status, RentalOrderStatus::Created);

    // 3. pick_up_car
    send_instruction(
        &mut svm,
        &payer,
        PickUpCar {
            rental_account,
            car_account,
            payer: payer.pubkey(),
        }
        .instruction(),
    );
    assert_eq!(
        fetch_rental_order(&svm, &rental_account).status,
        RentalOrderStatus::PickedUp
    );

    // 4. return_car
    send_instruction(
        &mut svm,
        &payer,
        ReturnCar {
            rental_account,
            car_account,
            payer: payer.pubkey(),
        }
        .instruction(),
    );
    assert_eq!(
        fetch_rental_order(&svm, &rental_account).status,
        RentalOrderStatus::Returned
    );
}

#[test]
fn pick_up_car_rejects_a_payer_that_did_not_sign() {
    let mut svm = load_svm();
    let victim = funded_signer(&mut svm);
    let attacker = funded_signer(&mut svm);

    let make = "Tesla";
    let model = "Model 3";
    let car_account = car_pda(make, model);
    send_instruction(
        &mut svm,
        &victim,
        add_car_instruction(&victim, make, model, 2024),
    );

    let rental_account = rental_pda(&car_account, &victim.pubkey());
    send_instruction(
        &mut svm,
        &victim,
        book_rental_instruction(&victim, &car_account, "Wilma Flintstone", 250),
    );

    // The attacker names the victim as `payer` but cannot produce the victim's
    // signature, so the account meta is demoted to a plain writable account.
    let mut instruction = PickUpCar {
        rental_account,
        car_account,
        payer: victim.pubkey(),
    }
    .instruction();
    for account in &mut instruction.accounts {
        if account.pubkey == victim.pubkey() {
            *account = AccountMeta::new(account.pubkey, false);
        }
    }
    send_expecting_custom_error(
        &mut svm,
        &attacker,
        instruction,
        ERROR_PAYER_SIGNATURE_MISSING,
    );

    // The rental is untouched.
    assert_eq!(
        fetch_rental_order(&svm, &rental_account).status,
        RentalOrderStatus::Created
    );
}

#[test]
fn pick_up_car_rejects_a_rental_account_not_owned_by_the_program() {
    let mut svm = load_svm();
    let payer = funded_signer(&mut svm);

    let make = "Volvo";
    let model = "EX30";
    let car_account = car_pda(make, model);
    send_instruction(
        &mut svm,
        &payer,
        add_car_instruction(&payer, make, model, 2025),
    );

    // Plant an account with plausible rental data at the correct PDA address,
    // but owned by the system program instead of the rental program.
    let rental_account = rental_pda(&car_account, &payer.pubkey());
    let planted_data_length = 165;
    svm.set_account(
        rental_account,
        Account {
            lamports: 10_000_000,
            data: vec![0u8; planted_data_length],
            owner: SYSTEM_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    send_expecting_custom_error(
        &mut svm,
        &payer,
        PickUpCar {
            rental_account,
            car_account,
            payer: payer.pubkey(),
        }
        .instruction(),
        ERROR_RENTAL_ACCOUNT_NOT_OWNED_BY_PROGRAM,
    );
}

#[test]
fn return_car_rejects_a_rental_that_was_never_picked_up() {
    let mut svm = load_svm();
    let payer = funded_signer(&mut svm);

    let make = "Kia";
    let model = "EV9";
    let car_account = car_pda(make, model);
    send_instruction(
        &mut svm,
        &payer,
        add_car_instruction(&payer, make, model, 2023),
    );

    let rental_account = rental_pda(&car_account, &payer.pubkey());
    send_instruction(
        &mut svm,
        &payer,
        book_rental_instruction(&payer, &car_account, "Barney Rubble", 400),
    );

    // Created -> Returned skips PickedUp and must be rejected.
    send_expecting_custom_error(
        &mut svm,
        &payer,
        ReturnCar {
            rental_account,
            car_account,
            payer: payer.pubkey(),
        }
        .instruction(),
        ERROR_RENTAL_NOT_IN_PICKED_UP_STATUS,
    );
    assert_eq!(
        fetch_rental_order(&svm, &rental_account).status,
        RentalOrderStatus::Created
    );
}
