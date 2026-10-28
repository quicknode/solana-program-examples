"use client";

import { useConnectedAddress, useOffers, useWalletTokens } from "@/hooks/use-chain-data";
import { useTransaction } from "@/hooks/use-transaction";
import { useWalletConnection } from "@/hooks/use-wallet";
import { formatAmount } from "@/lib/amounts";
import { wrappedSolBalance } from "@/lib/balances";
import { isReportableError } from "@/lib/errors";
import { buildUnwrapSolInstructions } from "@/lib/escrow-instructions";
import { isNativeSol } from "@/lib/tokens";
import { TransactionError } from "./transaction-notice";

const SOL_DECIMALS = 9;

// A maker who asked for SOL is paid while they are not there: the taker's
// transaction pays the SOL into the maker's native-mint token account, as
// wrapped SOL. This turns it back into plain SOL with one transaction.
export const UnwrapSolNotice = () => {
  const connected = useWalletConnection();
  const walletTokens = useWalletTokens();
  const viewer = useConnectedAddress();
  const offers = useOffers();
  const unwrap = useTransaction();
  const wrapped = wrappedSolBalance(walletTokens.data ?? []);
  // take_offer pays into this account, so while any of the viewer's offers
  // asks for SOL, unwrapping leaves an empty account open for it.
  const offersAskingForSol = (offers.data ?? []).filter(
    (offer) => offer.maker === viewer && isNativeSol(offer.mintB),
  ).length;

  if (!connected?.signer || wrapped === 0n) {
    return null;
  }
  const owner = connected.signer;
  return (
    <div className="notice notice--success" role="status">
      <p>
        You hold {formatAmount(wrapped, SOL_DECIMALS)} wrapped SOL, paid to you by takers of your offers. It sits in a
        token account; unwrapping closes that account and returns the SOL, and the account's rent, to your wallet.
        {offersAskingForSol > 0
          ? " You still have offers asking for SOL, so the same transaction opens a new, empty account for their takers to pay into."
          : ""}
      </p>
      <div className="notice__actions">
        <button
          type="button"
          className="button"
          disabled={unwrap.isRunning}
          onClick={async () => {
            try {
              await unwrap.dispatchAsync(() => buildUnwrapSolInstructions(owner, offersAskingForSol > 0));
            } catch {
              // The error is on unwrap.error, shown below.
            }
          }}
        >
          {unwrap.isRunning ? "Unwrapping…" : "Unwrap SOL"}
        </button>
      </div>
      {isReportableError(unwrap.error) ? <TransactionError error={unwrap.error} title="Nothing was unwrapped" /> : null}
    </div>
  );
};
