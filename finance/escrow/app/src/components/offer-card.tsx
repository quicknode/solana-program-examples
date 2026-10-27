import type { Address } from "@solana/kit";
import Link from "next/link";
import { formatAmount, formatPrice } from "@/lib/amounts";
import type { OfferListing } from "@/lib/offers";
import { shortAddress } from "@/lib/tokens";
import { TokenAmount } from "./token-amount";

type OfferCardProps = {
  offer: OfferListing;
  viewer: Address | null;
  // How much of the offer's token B the viewer holds, or undefined when no
  // wallet is connected.
  viewerBalanceOfB: bigint | undefined;
};

// One open offer, from the point of view of someone who might take it: what
// they would receive, what they would pay, the price that works out to, and
// whether their wallet can cover it.
export const OfferCard = ({ offer, viewer, viewerBalanceOfB }: OfferCardProps) => {
  const isYours = viewer === offer.maker;
  return (
    <Link className={`offer-card${isYours ? " offer-card--yours" : ""}`} href={`/offers/${offer.address}`}>
      <div className="offer-card__header">
        <span>
          Offer by <span className="address">{shortAddress(offer.maker)}</span>
        </span>
        <AffordabilityTag offer={offer} isYours={isYours} viewerBalanceOfB={viewerBalanceOfB} />
      </div>
      <dl className="offer-card__terms">
        <dt className="offer-card__term-label">You get</dt>
        <dd className="offer-card__term-value">
          <TokenAmount amount={offer.offeredAmount} mint={offer.mintA} size="large" />
        </dd>
        <dt className="offer-card__term-label">You pay</dt>
        <dd className="offer-card__term-value">
          <TokenAmount amount={offer.wantedAmount} mint={offer.mintB} size="large" />
        </dd>
      </dl>
      <p className="offer-card__price">
        1 {offer.mintA.symbol} = {formatPrice(offer.priceOfAInB)} {offer.mintB.symbol}
        <br />1 {offer.mintB.symbol} = {formatPrice(offer.priceOfBInA)} {offer.mintA.symbol}
      </p>
      <div className="offer-card__footer">
        <span className="offer-card__cta">{isYours ? "View or cancel →" : "Review and take →"}</span>
      </div>
    </Link>
  );
};

type AffordabilityTagProps = {
  offer: OfferListing;
  isYours: boolean;
  viewerBalanceOfB: bigint | undefined;
};

const AffordabilityTag = ({ offer, isYours, viewerBalanceOfB }: AffordabilityTagProps) => {
  if (isYours) {
    return <span className="status-tag status-tag--yours">Your offer</span>;
  }
  if (viewerBalanceOfB === undefined) {
    return null;
  }
  if (viewerBalanceOfB >= offer.wantedAmount) {
    return <span className="status-tag status-tag--ready">You can take this</span>;
  }
  const shortfall = offer.wantedAmount - viewerBalanceOfB;
  return (
    <span className="status-tag status-tag--short">
      Need {formatAmount(shortfall, offer.mintB.decimals)} more {offer.mintB.symbol}
    </span>
  );
};
