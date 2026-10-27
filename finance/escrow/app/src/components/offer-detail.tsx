"use client";

import type { Address, Signature } from "@solana/kit";
import Link from "next/link";
import { useState } from "react";
import { useConnectedAddress, useOffer } from "@/hooks/use-chain-data";
import { formatPrice } from "@/lib/amounts";
import { describeTransactionError } from "@/lib/errors";
import type { OfferListing } from "@/lib/offers";
import { AccountMap } from "./account-map";
import { CancelOfferPanel } from "./cancel-offer-panel";
import { GuidePanel } from "./guide-panel";
import { TakeOfferPanel } from "./take-offer-panel";
import { CoinMark, TokenAmount } from "./token-amount";
import { TransactionSuccess } from "./transaction-notice";

// What finished on this page, kept after the offer itself is gone: a taken or
// cancelled offer's account is closed, so it can no longer be read.
type Outcome = { kind: "taken" | "cancelled"; offer: OfferListing; signature: Signature };

export const OfferDetail = ({ offerAddress }: { offerAddress: Address }) => {
  const offer = useOffer(offerAddress);
  const viewer = useConnectedAddress();
  const [outcome, setOutcome] = useState<Outcome | null>(null);

  if (outcome) {
    return <OutcomeReceipt outcome={outcome} />;
  }

  if (offer.error) {
    return (
      <div className="notice notice--error">
        <h1 className="notice__title">Could not load this offer</h1>
        <p>{describeTransactionError(offer.error)}</p>
        <div className="notice__actions">
          <button type="button" className="button" onClick={() => offer.refresh()}>
            Try again
          </button>
        </div>
      </div>
    );
  }

  if (offer.data === undefined) {
    return <p className="loading-text">Reading the offer from the chain…</p>;
  }

  if (offer.data === null) {
    return (
      <div className="empty-state">
        <h1>This offer is gone</h1>
        <p>
          No open offer lives at <span className="address">{offerAddress}</span>. Someone took it, or the maker
          cancelled it; either way, its accounts are closed.
        </p>
        <Link className="button" href="/">
          See the open offers
        </Link>
      </div>
    );
  }

  const listing = offer.data;
  const isMaker = viewer === listing.maker;

  return (
    <div className="offer-detail">
      <Link className="back-link" href="/">
        ← All offers
      </Link>
      <header className="page-intro">
        <h1>
          {listing.mintA.symbol} for {listing.mintB.symbol}
        </h1>
        <p className="page-intro__lede">
          {isMaker
            ? "This is your offer. Your tokens wait in the vault until someone takes it or you cancel it."
            : "Review what you would pay and receive. Taking the offer swaps both sides in a single transaction."}
        </p>
      </header>

      <SwapSummary offer={listing} isMaker={isMaker} />

      <div className="page-columns">
        <div className="offer-detail">
          {isMaker ? (
            <CancelOfferPanel
              offer={listing}
              onCancelled={(signature) => setOutcome({ kind: "cancelled", offer: listing, signature })}
            />
          ) : (
            <TakeOfferPanel
              offer={listing}
              onTaken={(signature) => setOutcome({ kind: "taken", offer: listing, signature })}
            />
          )}
          <MovementList offer={listing} isMaker={isMaker} />
          <AccountMap offer={listing} />
        </div>
        <GuidePanel
          title={isMaker ? "Cancelling your offer" : "Before you take it"}
          steps={
            isMaker
              ? [
                  <>Only you can cancel: the program checks that the signer is the offer's maker.</>,
                  <>
                    Cancelling returns everything in the vault to your {listing.mintA.symbol} account, and the rent you
                    paid for the offer and vault accounts to your wallet.
                  </>,
                  <>
                    An offer has no expiry. Until you cancel it, anyone holding the tokens you asked for can take it.
                  </>,
                ]
              : [
                  <>
                    Check the price. The program swaps exactly the amounts shown: there is no fee, no spread and no
                    slippage.
                  </>,
                  <>
                    Make sure you hold the {listing.mintB.symbol} it asks for. The panel on the left checks your wallet
                    for you.
                  </>,
                  <>
                    Your transaction carries the terms you see here. If the maker swaps this offer for a worse one
                    before your transaction lands, the program refuses it and nothing moves.
                  </>,
                ]
          }
          footer={
            <>
              Why the terms travel with the transaction: <Link href="/how-it-works#bounds">the bait and switch</Link>.
            </>
          }
        />
      </div>
    </div>
  );
};

const SwapSummary = ({ offer, isMaker }: { offer: OfferListing; isMaker: boolean }) => (
  <section className="swap-summary" aria-label="The swap">
    <div className="swap-summary__side">
      <span className="swap-summary__label">{isMaker ? "You put in" : "You pay"}</span>
      <TokenAmount
        amount={isMaker ? offer.offeredAmount : offer.wantedAmount}
        mint={isMaker ? offer.mintA : offer.mintB}
        size="large"
      />
      <span className="swap-summary__note">
        {isMaker ? "Held in the vault until the offer is taken" : "From your wallet, straight to the maker"}
      </span>
    </div>
    <span className="swap-summary__arrow" aria-hidden="true">
      →
    </span>
    <div className="swap-summary__side swap-summary__side--receive">
      <span className="swap-summary__label">{isMaker ? "You get when it is taken" : "You get"}</span>
      <TokenAmount
        amount={isMaker ? offer.wantedAmount : offer.offeredAmount}
        mint={isMaker ? offer.mintB : offer.mintA}
        size="large"
        moving
      />
      <span className="swap-summary__note">
        {isMaker ? "Paid straight to your wallet by the taker" : "Out of the vault, released by the program"}
      </span>
    </div>
    <p className="swap-summary__price">
      <span>
        1 {offer.mintA.symbol} = {formatPrice(offer.priceOfAInB)} {offer.mintB.symbol}
      </span>
      <span>
        1 {offer.mintB.symbol} = {formatPrice(offer.priceOfBInA)} {offer.mintA.symbol}
      </span>
    </p>
  </section>
);

const MovementList = ({ offer, isMaker }: { offer: OfferListing; isMaker: boolean }) => (
  <section className="account-map" aria-labelledby="movement-heading">
    <h2 id="movement-heading">{isMaker ? "What happens when someone takes it" : "What happens when you take it"}</h2>
    <p>
      One transaction, calling the program's <code>take_offer</code> handler. Every step below happens, or none does.
    </p>
    <ol className="movement-list">
      <li className="movement-list__item">
        <div>
          <div className="movement-list__line">
            {isMaker ? "The taker sends you" : "You send the maker"} <CoinMark />
            <TokenAmount amount={offer.wantedAmount} mint={offer.mintB} moving />
          </div>
          <p className="movement-list__detail">
            From the taker's {offer.mintB.symbol} account to the maker's, signed by the taker.
          </p>
        </div>
      </li>
      <li className="movement-list__item">
        <div>
          <div className="movement-list__line">
            The vault sends {isMaker ? "the taker" : "you"} <CoinMark />
            <TokenAmount amount={offer.offeredAmount} mint={offer.mintA} moving />
          </div>
          <p className="movement-list__detail">
            Signed by the offer account itself, which the program can sign for because its address is a PDA of the
            program's.
          </p>
        </div>
      </li>
      <li className="movement-list__item">
        <div>
          <div className="movement-list__line">The program closes the vault and the offer account</div>
          <p className="movement-list__detail">
            Their rent deposits go back to the maker, who paid them when making the offer.
          </p>
        </div>
      </li>
    </ol>
  </section>
);

const OutcomeReceipt = ({ outcome }: { outcome: Outcome }) => {
  const { offer, signature } = outcome;
  return (
    <div className="offer-detail">
      <Link className="back-link" href="/">
        ← All offers
      </Link>
      {outcome.kind === "taken" ? (
        <TransactionSuccess
          title="Offer taken"
          signature={signature}
          actions={
            <Link className="button" href="/">
              Back to the offers
            </Link>
          }
        >
          <p>
            You paid <TokenAmount amount={offer.wantedAmount} mint={offer.mintB} /> and received{" "}
            <TokenAmount amount={offer.offeredAmount} mint={offer.mintA} moving />. The offer is closed, and the maker
            has their rent back.
          </p>
        </TransactionSuccess>
      ) : (
        <TransactionSuccess
          title="Offer cancelled"
          signature={signature}
          actions={
            <>
              <Link className="button" href="/make">
                Make another offer
              </Link>
              <Link className="button button--quiet" href="/">
                Back to the offers
              </Link>
            </>
          }
        >
          <p>
            <TokenAmount amount={offer.offeredAmount} mint={offer.mintA} moving /> went back to your wallet, along with
            the rent for the offer account and the vault.
          </p>
        </TransactionSuccess>
      )}
    </div>
  );
};
