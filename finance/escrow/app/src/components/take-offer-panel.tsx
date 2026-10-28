"use client";

import type { Signature } from "@solana/kit";
import { TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { useConnectedAddress, useSolBalance, useWalletTokens } from "@/hooks/use-chain-data";
import { useEscrowRent } from "@/hooks/use-rent";
import { useTransaction } from "@/hooks/use-transaction";
import { useWalletConnection } from "@/hooks/use-wallet";
import { formatAmount } from "@/lib/amounts";
import { SOL_RESERVE_LAMPORTS, spendableBalance, wrappedSolBalance } from "@/lib/balances";
import { ESCROW_PROGRAM } from "@/lib/config";
import { isReportableError } from "@/lib/errors";
import { buildTakeOfferInstructions } from "@/lib/escrow-instructions";
import type { OfferListing } from "@/lib/offers";
import { isNativeSol } from "@/lib/tokens";
import { TokenAmount } from "./token-amount";
import { TransactionError } from "./transaction-notice";

const SOL_DECIMALS = 9;
// A generous ceiling on one transaction's fee, so the check below never tells
// someone they can afford a take that then fails for want of a fee.
const FEE_ALLOWANCE_LAMPORTS = 100_000n;

type TakeOfferPanelProps = {
  offer: OfferListing;
  onTaken: (signature: Signature) => void;
};

// Checks that the connected wallet can take the offer, shows the balances
// before and after, and sends take_offer.
export const TakeOfferPanel = ({ offer, onTaken }: TakeOfferPanelProps) => {
  const connected = useWalletConnection();
  const viewer = useConnectedAddress();
  const walletTokens = useWalletTokens();
  const solBalance = useSolBalance();
  const rent = useEscrowRent();
  const take = useTransaction();

  if (!connected || !viewer) {
    return (
      <section className="action-panel" aria-labelledby="take-heading">
        <h2 className="action-panel__title" id="take-heading">
          Take this offer
        </h2>
        <p>Connect a wallet (top right) to check whether you can take this offer, and to take it.</p>
      </section>
    );
  }

  if (walletTokens.error || solBalance.error) {
    return (
      <section className="action-panel">
        <TransactionError error={walletTokens.error ?? solBalance.error} title="Could not read your wallet" />
        <button
          type="button"
          className="button"
          onClick={() => {
            walletTokens.refresh();
            solBalance.refresh();
          }}
        >
          Try again
        </button>
      </section>
    );
  }

  if (walletTokens.data === undefined || solBalance.data === undefined) {
    return (
      <section className="action-panel">
        <p className="loading-text">Checking your wallet…</p>
      </section>
    );
  }

  const holdingA = walletTokens.data.find((token) => token.mint.address === offer.mintA.address);
  // Paying SOL draws on the wallet's SOL as well as any SOL already wrapped;
  // receiving SOL lands in the wallet as SOL, since the transaction unwraps it.
  const payingSol = isNativeSol(offer.mintB);
  const receivingSol = isNativeSol(offer.mintA);
  const balanceA = receivingSol ? solBalance.data + (holdingA?.amount ?? 0n) : (holdingA?.amount ?? 0n);
  const balanceB = spendableBalance(offer.mintB, walletTokens.data, solBalance.data);

  // The taker pays rent for their own token-A account if take_offer has to
  // create it, and keeps that account afterwards.
  const tokenAccountRent =
    offer.mintA.tokenProgram === TOKEN_2022_PROGRAM_ADDRESS
      ? rent.data?.token2022Account
      : rent.data?.classicTokenAccount;
  // Paying SOL keeps a reserve back for fees and rent inside balanceB.
  const solNeeded = payingSol
    ? SOL_RESERVE_LAMPORTS
    : FEE_ALLOWANCE_LAMPORTS + (holdingA || receivingSol ? 0n : (tokenAccountRent ?? 0n));

  const hasEnoughB = balanceB >= offer.wantedAmount;
  const hasEnoughSol = solBalance.data >= solNeeded;
  const canSign = connected.signer !== null;

  return (
    <section className="action-panel" aria-labelledby="take-heading">
      <h2 className="action-panel__title" id="take-heading">
        Take this offer
      </h2>
      <ul className="checklist">
        <li className={`checklist__item checklist__item--${hasEnoughB ? "pass" : "fail"}`}>
          <span>
            You hold <TokenAmount amount={balanceB} mint={offer.mintB} />, and this offer wants{" "}
            <TokenAmount amount={offer.wantedAmount} mint={offer.mintB} />.
            {hasEnoughB
              ? null
              : ` You need ${formatAmount(offer.wantedAmount - balanceB, offer.mintB.decimals)} ${offer.mintB.symbol} more.`}
          </span>
        </li>
        <li className={`checklist__item checklist__item--${hasEnoughSol ? "pass" : "fail"}`}>
          <span>
            You hold {formatAmount(solBalance.data, SOL_DECIMALS)} SOL for the transaction fee
            {holdingA || receivingSol ? "" : " and the new account's rent"}.
          </span>
        </li>
        <li className="checklist__item checklist__item--info">
          <span>
            {receivingSol
              ? "The SOL lands in your wallet as plain SOL: the transaction unwraps it as the last step."
              : holdingA
                ? `The ${offer.mintA.symbol} lands in the ${offer.mintA.symbol} account you already have.`
                : `You have no ${offer.mintA.symbol} account yet, so the transaction creates one for you. Its rent deposit${
                    tokenAccountRent === undefined ? "" : ` of ${formatAmount(tokenAccountRent, SOL_DECIMALS)} SOL`
                  } stays in the account, which is yours to keep.`}
          </span>
        </li>
        {payingSol ? (
          <li className="checklist__item checklist__item--info">
            <span>
              The transaction wraps the SOL you pay into a token account first, since the escrow program moves tokens,
              and closes that account again at the end.
              {formatAmount(SOL_RESERVE_LAMPORTS, SOL_DECIMALS)} SOL of your balance stays back for fees.
            </span>
          </li>
        ) : null}
        {canSign ? null : (
          <li className="checklist__item checklist__item--fail">
            <span>This wallet account can only be watched, not signed with. Connect one that can sign.</span>
          </li>
        )}
      </ul>

      <table className="balance-table">
        <thead>
          <tr>
            <th className="balance-table__heading" scope="col">
              Your balance
            </th>
            <th className="balance-table__heading" scope="col">
              Now
            </th>
            <th className="balance-table__heading" scope="col">
              After taking
            </th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <th className="balance-table__cell" scope="row">
              {offer.mintB.symbol}
            </th>
            <td className="balance-table__cell">{formatAmount(balanceB, offer.mintB.decimals)}</td>
            <td className="balance-table__cell">
              {hasEnoughB ? formatAmount(balanceB - offer.wantedAmount, offer.mintB.decimals) : "-"}
            </td>
          </tr>
          <tr>
            <th className="balance-table__cell" scope="row">
              {offer.mintA.symbol}
            </th>
            <td className="balance-table__cell">{formatAmount(balanceA, offer.mintA.decimals)}</td>
            <td className="balance-table__cell balance-table__cell--change">
              {formatAmount(balanceA + offer.offeredAmount, offer.mintA.decimals)}
            </td>
          </tr>
        </tbody>
      </table>

      <button
        type="button"
        className="button button--primary button--wide"
        disabled={!hasEnoughB || !hasEnoughSol || !canSign || take.isRunning}
        onClick={async () => {
          const taker = connected.signer;
          if (!taker) {
            return;
          }
          try {
            const signature = await take.dispatchAsync(() =>
              buildTakeOfferInstructions(taker, offer, ESCROW_PROGRAM, wrappedSolBalance(walletTokens.data ?? [])),
            );
            onTaken(signature);
          } catch {
            // The error is on take.error, shown below.
          }
        }}
      >
        {take.isRunning
          ? "Approve in your wallet, then wait for the network…"
          : `Pay ${formatAmount(offer.wantedAmount, offer.mintB.decimals)} ${offer.mintB.symbol}, get ${formatAmount(
              offer.offeredAmount,
              offer.mintA.decimals,
            )} ${offer.mintA.symbol}`}
      </button>
      <p className="field__hint">
        Your wallet will ask you to approve one transaction. It sends <code>take_offer</code> with your terms: at least{" "}
        <TokenAmount amount={offer.offeredAmount} mint={offer.mintA} /> out, at most{" "}
        <TokenAmount amount={offer.wantedAmount} mint={offer.mintB} /> in.
      </p>
      {isReportableError(take.error) ? <TransactionError error={take.error} title="The offer was not taken" /> : null}
    </section>
  );
};
