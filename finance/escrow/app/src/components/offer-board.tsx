"use client";

import Link from "next/link";
import { useMemo, useState } from "react";
import { useConnectedAddress, useOffers, useProgramDeployed, useWalletTokens } from "@/hooks/use-chain-data";
import { CLUSTER, ESCROW_PROGRAM } from "@/lib/config";
import { describeTransactionError } from "@/lib/errors";
import { type OfferListing, type OfferSort, sortOffers } from "@/lib/offers";
import { useChainActivity } from "./chain-activity";
import { OfferCard } from "./offer-card";

const ANY_TOKEN = "any";

// Every open offer, with the filters a taker uses to find one worth taking:
// the token they want, the token they have, and whether they can afford it.
export const OfferBoard = () => {
  const offers = useOffers();
  const programDeployed = useProgramDeployed();
  const walletTokens = useWalletTokens();
  const viewer = useConnectedAddress();
  const { isLive } = useChainActivity();

  const [receiveMint, setReceiveMint] = useState(ANY_TOKEN);
  const [payMint, setPayMint] = useState(ANY_TOKEN);
  const [sort, setSort] = useState<OfferSort>("best-price");
  const [onlyAffordable, setOnlyAffordable] = useState(false);
  const [hideOwn, setHideOwn] = useState(false);

  const balances = useMemo(() => {
    const byMint = new Map<string, bigint>();
    for (const token of walletTokens.data ?? []) {
      byMint.set(token.mint.address, token.amount);
    }
    return byMint;
  }, [walletTokens.data]);

  const balanceOf = (mintAddress: string): bigint | undefined =>
    viewer && walletTokens.data ? (balances.get(mintAddress) ?? 0n) : undefined;

  const allOffers = offers.data ?? [];
  const tokensOnOffer = uniqueMints(allOffers.map((offer) => offer.mintA));
  const tokensWanted = uniqueMints(allOffers.map((offer) => offer.mintB));

  const visibleOffers = sortOffers(
    allOffers.filter((offer) => {
      if (receiveMint !== ANY_TOKEN && offer.mintA.address !== receiveMint) {
        return false;
      }
      if (payMint !== ANY_TOKEN && offer.mintB.address !== payMint) {
        return false;
      }
      if (hideOwn && offer.maker === viewer) {
        return false;
      }
      if (onlyAffordable && (balanceOf(offer.mintB.address) ?? 0n) < offer.wantedAmount) {
        return false;
      }
      return true;
    }),
    sort,
  );

  if (programDeployed.data === false) {
    return (
      <div className="notice notice--warning">
        <h2 className="notice__title">The escrow program is not deployed on {CLUSTER}</h2>
        <p>
          Nothing is at <span className="address">{ESCROW_PROGRAM}</span> on this cluster, so there are no offers to
          show. Deploy the program (the app's README says how) and set <code>NEXT_PUBLIC_ESCROW_PROGRAM_ADDRESS</code>{" "}
          if it landed at a different address.
        </p>
      </div>
    );
  }

  if (offers.error) {
    return (
      <div className="notice notice--error">
        <h2 className="notice__title">Could not load offers</h2>
        <p>{describeTransactionError(offers.error)}</p>
        <div className="notice__actions">
          <button type="button" className="button" onClick={() => offers.refresh()}>
            Try again
          </button>
        </div>
      </div>
    );
  }

  return (
    <section className="offer-board" aria-label="Open offers">
      <div className="offer-board__toolbar">
        <label className="field">
          <span className="field__label">You want to get</span>
          <select
            className="field__control"
            value={receiveMint}
            onChange={(event) => setReceiveMint(event.target.value)}
          >
            <option value={ANY_TOKEN}>Any token</option>
            {tokensOnOffer.map((mint) => (
              <option key={mint.address} value={mint.address}>
                {mint.symbol}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span className="field__label">You want to pay with</span>
          <select className="field__control" value={payMint} onChange={(event) => setPayMint(event.target.value)}>
            <option value={ANY_TOKEN}>Any token</option>
            {tokensWanted.map((mint) => (
              <option key={mint.address} value={mint.address}>
                {mint.symbol}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span className="field__label">Sort</span>
          <select
            className="field__control"
            value={sort}
            onChange={(event) => setSort(event.target.value as OfferSort)}
          >
            <option value="best-price">Cheapest first</option>
            <option value="largest">Largest first</option>
            <option value="smallest">Smallest first</option>
          </select>
        </label>
        {viewer ? (
          <>
            <label className="field field--checkbox">
              <input
                type="checkbox"
                checked={onlyAffordable}
                onChange={(event) => setOnlyAffordable(event.target.checked)}
              />
              Only offers I can pay for
            </label>
            <label className="field field--checkbox">
              <input type="checkbox" checked={hideOwn} onChange={(event) => setHideOwn(event.target.checked)} />
              Hide my offers
            </label>
          </>
        ) : null}
      </div>

      <div className="offer-board__summary">
        <span>
          {offers.data === undefined
            ? "Loading offers…"
            : `${visibleOffers.length} of ${allOffers.length} open offer${allOffers.length === 1 ? "" : "s"}`}
          {receiveMint !== ANY_TOKEN || payMint !== ANY_TOKEN
            ? ". Cheapest means the least you pay per token you get."
            : ""}
        </span>
        {walletTokens.error ? (
          <span>Could not read your wallet's balances, so no offer says whether you can pay.</span>
        ) : null}
        <span className={`live-indicator${isLive ? " live-indicator--on" : ""}`}>
          {isLive ? "Live: new offers appear as they are made" : "Not live: refreshes after your own transactions"}
        </span>
      </div>

      {offers.data !== undefined && allOffers.length === 0 ? (
        <div className="empty-state">
          <h2>No open offers on {CLUSTER} yet</h2>
          <p>Every offer that was made has been taken or cancelled. Be the first to make one.</p>
          <Link className="button button--primary" href="/make">
            Make an offer
          </Link>
        </div>
      ) : null}

      {allOffers.length > 0 && visibleOffers.length === 0 ? (
        <div className="empty-state">
          <p>No offer matches these filters.</p>
        </div>
      ) : null}

      <ul className="offer-list">
        {visibleOffers.map((offer) => (
          <li key={offer.address}>
            <OfferCard offer={offer} viewer={viewer} viewerBalanceOfB={balanceOf(offer.mintB.address)} />
          </li>
        ))}
      </ul>
    </section>
  );
};

const uniqueMints = (mints: Array<OfferListing["mintA"]>) => {
  const byAddress = new Map(mints.map((mint) => [mint.address, mint]));
  return [...byAddress.values()].sort((first, second) => first.symbol.localeCompare(second.symbol));
};
