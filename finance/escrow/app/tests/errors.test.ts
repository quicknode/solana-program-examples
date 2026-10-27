import assert from "node:assert";
import { describe, test } from "node:test";
import {
  SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM,
  SOLANA_ERROR__JSON_RPC__SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE,
  SolanaError,
} from "@solana/kit";
import { describeTransactionError, ensureError, findCustomErrorCode } from "../src/lib/errors";

// The shape a failed simulation arrives in: a preflight failure whose cause
// is the instruction's custom error.
const preflightFailure = (code: number) =>
  new SolanaError(SOLANA_ERROR__JSON_RPC__SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE, {
    accounts: null,
    cause: new SolanaError(SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM, { code, index: 0 }),
    logs: [],
    returnData: null,
    unitsConsumed: 0n,
  } as never);

describe("describeTransactionError", () => {
  test("finds the program's error code beneath the preflight failure", () => {
    assert.strictEqual(findCustomErrorCode(preflightFailure(6001)), 6001);
  });

  test("explains the escrow program's own errors", () => {
    assert.match(describeTransactionError(preflightFailure(6001)), /offer changed before your transaction landed/);
    assert.match(describeTransactionError(preflightFailure(6000)), /more than zero tokens/);
  });

  test("explains Anchor's constraint errors and the token program's", () => {
    assert.match(describeTransactionError(preflightFailure(2012)), /Only the maker can cancel/);
    assert.match(describeTransactionError(preflightFailure(1)), /Not enough tokens/);
  });

  test("names an unknown code rather than hiding it", () => {
    assert.match(describeTransactionError(preflightFailure(4242)), /error code 4242/);
  });

  test("recognises a declined wallet prompt", () => {
    assert.match(describeTransactionError(new Error("User rejected the request.")), /declined the transaction/);
  });

  test("turns a thrown non-Error into an Error", () => {
    assert.strictEqual(ensureError("boom").message, "Non-Error thrown: boom");
  });
});
