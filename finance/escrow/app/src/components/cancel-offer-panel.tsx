"use client";

import type { Signature } from "@solana/kit";
import { TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { useEscrowRent } from "@/hooks/use-rent";
import { useTransaction } from "@/hooks/use-transaction";
import { useWalletConnection } from "@/hooks/use-wallet";
import { formatAmount } from "@/lib/amounts";
import { ESCROW_PROGRAM } from "@/lib/config";
import { isReportableError } from "@/lib/errors";
import { buildCancelOfferInstruction } from "@/lib/escrow-instructions";
import type { OfferListing } from "@/lib/offers";
import { TokenAmount } from "./token-amount";
import { TransactionError } from "./transaction-notice";

const SOL_DECIMALS = 9;

type CancelOfferPanelProps = {
  offer: OfferListing;
  onCancelled: (signature: Signature) => void;
};

// The maker's view of their own offer: cancel it and take everything back.
export const CancelOfferPanel = ({ offer, onCancelled }: CancelOfferPanelProps) => {
  const connected = useWalletConnection();
  const rent = useEscrowRent();
  const cancel = useTransaction();

  const vaultRent =
    offer.mintA.tokenProgram === TOKEN_2022_PROGRAM_ADDRESS
      ? rent.data?.token2022Account
      : rent.data?.classicTokenAccount;
  const rentBack = rent.data && vaultRent !== undefined ? rent.data.offerAccount + vaultRent : undefined;

  return (
    <section className="action-panel" aria-labelledby="cancel-heading">
      <h2 className="action-panel__title" id="cancel-heading">
        Cancel your offer
      </h2>
      <ul className="checklist">
        <li className="checklist__item checklist__item--info">
          <span>
            <TokenAmount amount={offer.offeredAmount} mint={offer.mintA} moving /> comes back from the vault to your{" "}
            {offer.mintA.symbol} account.
          </span>
        </li>
        <li className="checklist__item checklist__item--info">
          <span>
            The offer account and the vault close, and their rent
            {rentBack === undefined ? "" : `, ${formatAmount(rentBack, SOL_DECIMALS)} SOL,`} comes back to your wallet.
          </span>
        </li>
      </ul>
      <button
        type="button"
        className="button button--danger button--wide"
        disabled={!connected?.signer || cancel.isRunning}
        onClick={async () => {
          const maker = connected?.signer;
          if (!maker) {
            return;
          }
          try {
            const signature = await cancel.dispatchAsync(async () => [
              await buildCancelOfferInstruction(maker, offer, ESCROW_PROGRAM),
            ]);
            onCancelled(signature);
          } catch {
            // The error is on cancel.error, shown below.
          }
        }}
      >
        {cancel.isRunning ? "Approve in your wallet, then wait for the network…" : "Cancel offer and take it all back"}
      </button>
      {isReportableError(cancel.error) ? (
        <TransactionError error={cancel.error} title="The offer was not cancelled" />
      ) : null}
    </section>
  );
};
