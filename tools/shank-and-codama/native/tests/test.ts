// In-process integration test for the car rental service program.
//
// Runs entirely in CI with no network: the program `.so` is loaded into a
// LiteSVM instance and exercised through the Codama-generated client
// (tests/generated). It creates a car (add_car), books a rental
// (book_rental) and picks it up (pick_up_car), asserting on-chain account
// state after each step.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  address,
  getAddressEncoder,
  getProgramDerivedAddress,
  generateKeyPairSigner,
  getUtf8Encoder,
  lamports,
  pipe,
  appendTransactionMessageInstruction,
  createTransactionMessage,
  setTransactionMessageFeePayerSigner,
  signTransactionMessageWithSigners,
  type Address,
} from "@solana/kit";
import { FailedTransactionMetadata, LiteSVM } from "litesvm";

import {
  CAR_RENTAL_SERVICE_PROGRAM_ADDRESS,
  decodeCar,
  decodeRentalOrder,
  getAddCarInstruction,
  getBookRentalInstruction,
  getPickUpCarInstruction,
  RentalOrderStatus,
} from "./generated/src/generated/index.ts";

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
  // deno-lint-ignore no-explicit-any
  ix: any,
) {
  const tx = await pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(payer, m),
    (m) => svm.setTransactionMessageLifetimeUsingLatestBlockhash(m),
    (m) => appendTransactionMessageInstruction(ix, m),
    (m) => signTransactionMessageWithSigners(m),
  );
  const result = svm.sendTransaction(tx);
  if (result instanceof FailedTransactionMetadata) {
    throw new Error(`Transaction failed: ${result.err()}\n${result.meta().logs().join("\n")}`);
  }
  return result;
}

test("car rental service: add_car, book_rental, pick_up_car", async () => {
  const { svm, programId } = loadSvm();

  const payer = await generateKeyPairSigner();
  svm.airdrop(payer.address, lamports(10_000_000_000n));

  // 1. add_car
  const make = "BMW";
  const model = "iX1";
  const carAccount = await carPda(programId, make, model);

  await sendIx(
    svm,
    payer,
    getAddCarInstruction({ carAccount, payer, year: 2020, make, model }),
  );

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
  await sendIx(
    svm,
    payer,
    getPickUpCarInstruction({ rentalAccount, carAccount, payer: payer.address }),
  );

  rentalRaw = svm.getAccount(rentalAccount);
  assert.ok(rentalRaw?.exists, "rental account should still exist");
  rental = decodeRentalOrder(rentalRaw);
  assert.equal(rental.data.status, RentalOrderStatus.PickedUp);
});
