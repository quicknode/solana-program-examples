// In-process integration test for the car rental service program.
//
// Runs entirely in CI with no network: the program `.so` is loaded into a
// LiteSVM instance and exercised through the Codama-generated client
// (tests/generated). It walks the full rental lifecycle (add_car,
// book_rental, pick_up_car, return_car), asserting onchain account state
// after each step, and verifies the program's account validation: a
// non-signing payer, a rental account owned by the wrong program, and an
// invalid status transition are all rejected.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  AccountRole,
  type Address,
  appendTransactionMessageInstruction,
  createNoopSigner,
  createTransactionMessage,
  generateKeyPairSigner,
  getAddressEncoder,
  getProgramDerivedAddress,
  getUtf8Encoder,
  type Instruction,
  lamports,
  pipe,
  setTransactionMessageFeePayerSigner,
  signTransactionMessageWithSigners,
} from "@solana/kit";
import { FailedTransactionMetadata, LiteSVM } from "litesvm";

import {
  CAR_RENTAL_SERVICE_PROGRAM_ADDRESS,
  decodeCar,
  decodeRentalOrder,
  getAddCarInstruction,
  getBookRentalInstruction,
  getPickUpCarInstruction,
  getReturnCarInstruction,
  RentalOrderStatus,
} from "./generated/src/generated/index.ts";

// Custom error codes from program/src/error.rs (CarRentalError). The enum
// starts at 6000, matching Anchor's custom-error offset.
const ERROR_PAYER_SIGNATURE_MISSING = 6002;
const ERROR_RENTAL_ACCOUNT_NOT_OWNED_BY_PROGRAM = 6003;
const ERROR_RENTAL_NOT_IN_PICKED_UP_STATUS = 6005;

const here = dirname(fileURLToPath(import.meta.url));
const programSoPath = join(here, "..", "program", "target", "so", "car_rental_service.so");

const utf8 = getUtf8Encoder();
const addressEncoder = getAddressEncoder();

function loadSvm(): { svm: LiteSVM; programId: Address } {
  const svm = new LiteSVM();
  const programId = CAR_RENTAL_SERVICE_PROGRAM_ADDRESS;
  svm.addProgram(programId, readFileSync(programSoPath));
  return { svm, programId };
}

async function carPda(programId: Address, make: string, model: string): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: programId,
    seeds: [utf8.encode("car"), utf8.encode(make), utf8.encode(model)],
  });
  return pda;
}

async function rentalPda(programId: Address, car: Address, payer: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: programId,
    seeds: [utf8.encode("rental_order"), addressEncoder.encode(car), addressEncoder.encode(payer)],
  });
  return pda;
}

async function sendIx(
  svm: LiteSVM,
  payer: Awaited<ReturnType<typeof generateKeyPairSigner>>,
  instruction: Instruction,
) {
  const tx = await pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(payer, m),
    (m) => svm.setTransactionMessageLifetimeUsingLatestBlockhash(m),
    (m) => appendTransactionMessageInstruction(instruction, m),
    (m) => signTransactionMessageWithSigners(m),
  );
  const result = svm.sendTransaction(tx);
  if (result instanceof FailedTransactionMetadata) {
    throw new Error(`Transaction failed: ${result.err()}\n${result.meta().logs().join("\n")}`);
  }
  return result;
}

/** Assert that sending `instruction` fails with the given custom error code. */
async function expectCustomError(
  svm: LiteSVM,
  payer: Awaited<ReturnType<typeof generateKeyPairSigner>>,
  instruction: Instruction,
  errorCode: number,
) {
  // The runtime logs custom errors as hex: "custom program error: 0x1772".
  const errorCodeHex = `0x${errorCode.toString(16)}`;
  await assert.rejects(
    sendIx(svm, payer, instruction),
    (thrownObject: Error) => thrownObject.message.includes(errorCodeHex),
    `expected custom program error ${errorCode} (${errorCodeHex})`,
  );
}

async function fundedSigner(svm: LiteSVM) {
  const signer = await generateKeyPairSigner();
  svm.airdrop(signer.address, lamports(10_000_000_000n));
  return signer;
}

test("car rental service: full lifecycle add_car -> book_rental -> pick_up_car -> return_car", async () => {
  const { svm, programId } = loadSvm();
  const payer = await fundedSigner(svm);

  // 1. add_car
  const make = "BMW";
  const model = "iX1";
  const carAccount = await carPda(programId, make, model);

  await sendIx(svm, payer, getAddCarInstruction({ carAccount, payer, year: 2020, make, model }));

  const carRaw = svm.getAccount(carAccount);
  assert.ok(carRaw?.exists, "car account should exist");
  const car = decodeCar(carRaw);
  assert.equal(car.data.year, 2020);
  assert.equal(car.data.make, make);
  assert.equal(car.data.model, model);

  // 2. book_rental
  const rentalAccount = await rentalPda(programId, carAccount, payer.address);
  await sendIx(
    svm,
    payer,
    getBookRentalInstruction({
      rentalAccount,
      carAccount,
      payer,
      name: "Fred Flintstone",
      pickUpDate: "01/28/2023 8:00 AM",
      returnDate: "01/28/2023 10:00 PM",
      price: 300,
    }),
  );

  let rentalRaw = svm.getAccount(rentalAccount);
  assert.ok(rentalRaw?.exists, "rental account should exist");
  let rental = decodeRentalOrder(rentalRaw);
  assert.equal(rental.data.name, "Fred Flintstone");
  assert.equal(rental.data.car, carAccount);
  assert.equal(rental.data.price, 300n);
  assert.equal(rental.data.status, RentalOrderStatus.Created);

  // 3. pick_up_car
  await sendIx(svm, payer, getPickUpCarInstruction({ rentalAccount, carAccount, payer }));

  rentalRaw = svm.getAccount(rentalAccount);
  assert.ok(rentalRaw?.exists, "rental account should still exist");
  rental = decodeRentalOrder(rentalRaw);
  assert.equal(rental.data.status, RentalOrderStatus.PickedUp);

  // 4. return_car
  await sendIx(svm, payer, getReturnCarInstruction({ rentalAccount, carAccount, payer }));

  rentalRaw = svm.getAccount(rentalAccount);
  assert.ok(rentalRaw?.exists, "rental account should still exist");
  rental = decodeRentalOrder(rentalRaw);
  assert.equal(rental.data.status, RentalOrderStatus.Returned);
});

test("pick_up_car rejects a payer that did not sign", async () => {
  const { svm, programId } = loadSvm();
  const victim = await fundedSigner(svm);
  const attacker = await fundedSigner(svm);

  const make = "Tesla";
  const model = "Model 3";
  const carAccount = await carPda(programId, make, model);
  await sendIx(svm, victim, getAddCarInstruction({ carAccount, payer: victim, year: 2024, make, model }));

  const rentalAccount = await rentalPda(programId, carAccount, victim.address);
  await sendIx(
    svm,
    victim,
    getBookRentalInstruction({
      rentalAccount,
      carAccount,
      payer: victim,
      name: "Wilma Flintstone",
      pickUpDate: "02/01/2023 9:00 AM",
      returnDate: "02/01/2023 5:00 PM",
      price: 250,
    }),
  );

  // The attacker names the victim as `payer` but cannot produce the victim's
  // signature, so the account meta is demoted to a plain writable account.
  const instruction = getPickUpCarInstruction({
    rentalAccount,
    carAccount,
    payer: createNoopSigner(victim.address),
  });
  const instructionWithoutVictimSignature: Instruction = {
    ...instruction,
    accounts: instruction.accounts.map((account) =>
      account.address === victim.address ? { address: account.address, role: AccountRole.WRITABLE } : account,
    ),
  };

  await expectCustomError(svm, attacker, instructionWithoutVictimSignature, ERROR_PAYER_SIGNATURE_MISSING);

  // The rental is untouched.
  const rental = decodeRentalOrder(svm.getAccount(rentalAccount)!);
  assert.equal(rental.data.status, RentalOrderStatus.Created);
});

test("pick_up_car rejects a rental account not owned by the program", async () => {
  const { svm, programId } = loadSvm();
  const payer = await fundedSigner(svm);

  const make = "Volvo";
  const model = "EX30";
  const carAccount = await carPda(programId, make, model);
  await sendIx(svm, payer, getAddCarInstruction({ carAccount, payer, year: 2025, make, model }));

  // Plant an account with plausible rental data at the correct PDA address,
  // but owned by the system program instead of the rental program.
  const rentalAccount = await rentalPda(programId, carAccount, payer.address);
  const plantedDataLength = 165;
  svm.setAccount({
    address: rentalAccount,
    lamports: lamports(10_000_000n),
    data: new Uint8Array(plantedDataLength),
    programAddress: "11111111111111111111111111111111" as Address,
    executable: false,
    space: BigInt(plantedDataLength),
  });

  await expectCustomError(
    svm,
    payer,
    getPickUpCarInstruction({ rentalAccount, carAccount, payer }),
    ERROR_RENTAL_ACCOUNT_NOT_OWNED_BY_PROGRAM,
  );
});

test("return_car rejects a rental that was never picked up", async () => {
  const { svm, programId } = loadSvm();
  const payer = await fundedSigner(svm);

  const make = "Kia";
  const model = "EV9";
  const carAccount = await carPda(programId, make, model);
  await sendIx(svm, payer, getAddCarInstruction({ carAccount, payer, year: 2023, make, model }));

  const rentalAccount = await rentalPda(programId, carAccount, payer.address);
  await sendIx(
    svm,
    payer,
    getBookRentalInstruction({
      rentalAccount,
      carAccount,
      payer,
      name: "Barney Rubble",
      pickUpDate: "03/15/2023 10:00 AM",
      returnDate: "03/16/2023 10:00 AM",
      price: 400,
    }),
  );

  // Created -> Returned skips PickedUp and must be rejected.
  await expectCustomError(
    svm,
    payer,
    getReturnCarInstruction({ rentalAccount, carAccount, payer }),
    ERROR_RENTAL_NOT_IN_PICKED_UP_STATUS,
  );

  const rental = decodeRentalOrder(svm.getAccount(rentalAccount)!);
  assert.equal(rental.data.status, RentalOrderStatus.Created);
});
