import { isAbortError, isSolanaError, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM } from "@solana/kit";
import { ESCROW_ERROR__OFFER_TERMS_CHANGED, ESCROW_ERROR__ZERO_AMOUNT } from "../generated";

// In JS it's possible to throw *anything*. A sensible programmer
// will only throw Errors but we must still check to satisfy
// TypeScript (and flag any craziness)
export const ensureError = (thrownObject: unknown): Error => {
  if (thrownObject instanceof Error) {
    return thrownObject;
  }
  return new Error(`Non-Error thrown: ${String(thrownObject)}`);
};

// An action superseded by a newer one rejects with an AbortError. That is not
// a failure to show anyone.
export const isReportableError = (error: unknown): boolean => error !== undefined && !isAbortError(error);

// Anchor's own constraint errors, which the escrow program raises through its
// account constraints rather than its own error enum.
// https://github.com/solana-foundation/anchor/blob/master/lang/src/error.rs
const ANCHOR_ERROR_MESSAGES: Record<number, string> = {
  // ConstraintSeeds under Anchor 1: the offer's address, re-derived from the
  // signer, is not this offer's. Anchor 2 reports a seeds mismatch as the
  // runtime's InvalidSeeds instead.
  2006: "Only the maker can cancel this offer: the offer's address does not derive from your address.",
  // ConstraintAddress: the signer is not the maker the offer stored.
  2012: "Only the maker can cancel this offer.",
  2040: "An offer cannot swap a token for itself: pick two different tokens.",
};

const ESCROW_ERROR_MESSAGES: Record<number, string> = {
  [ESCROW_ERROR__ZERO_AMOUNT]: "An offer must offer and want more than zero tokens.",
  [ESCROW_ERROR__OFFER_TERMS_CHANGED]:
    "The offer changed before your transaction landed: it now pays less, or wants more, than the terms you agreed to. Nothing moved. Look at the offer again before taking it.",
};

// The token program's own error codes, from its TokenError enum.
const TOKEN_ERROR_MESSAGES: Record<number, string> = {
  1: "Not enough tokens: a token account holds less than this transaction moves.",
};

// A program's own error arrives several layers down: a failed send wraps the
// failed simulation, which wraps the transaction error, which wraps the
// instruction's custom error code. Walk down the causes until one is that code.
export const findCustomErrorCode = (error: Error): number | undefined => {
  let current: unknown = error;
  while (current instanceof Error) {
    if (isSolanaError(current, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM)) {
      return current.context.code;
    }
    current = current.cause;
  }
  return undefined;
};

// Turn whatever a transaction threw into a sentence a person can act on.
export const describeTransactionError = (thrownObject: unknown): string => {
  const error = ensureError(thrownObject);
  const code = findCustomErrorCode(error);
  if (code !== undefined) {
    const known = ESCROW_ERROR_MESSAGES[code] ?? ANCHOR_ERROR_MESSAGES[code] ?? TOKEN_ERROR_MESSAGES[code];
    if (known) {
      return known;
    }
    return `The program refused the transaction with error code ${code}.`;
  }
  if (/reject|denied|declined|cancel/i.test(error.message)) {
    return "You declined the transaction in your wallet. Nothing was sent.";
  }
  if (/insufficient (funds|lamports)|debit an account but found no record of a prior credit/i.test(error.message)) {
    return "Your wallet does not have enough SOL to pay the transaction fee and the rent deposit.";
  }
  return error.message;
};
