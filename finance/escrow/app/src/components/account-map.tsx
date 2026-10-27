import type { OfferListing } from "@/lib/offers";
import { AddressLink } from "./address-link";

// The accounts behind an offer, each with the one sentence that says why it
// exists. These are the accounts take_offer and cancel_offer name.
export const AccountMap = ({ offer }: { offer: OfferListing }) => (
  <section className="account-map" aria-labelledby="accounts-heading">
    <h2 id="accounts-heading">The accounts behind this offer</h2>
    <dl className="account-map__list">
      <dt className="account-map__name">Offer</dt>
      <dd className="account-map__value">
        <AddressLink address={offer.address} />
        <span className="account-map__explanation">
          Holds the terms: who made the offer, the two tokens, and how much {offer.mintB.symbol} the maker wants. Its
          address is a PDA of the seeds <code>"offer"</code>, the maker's address, and the offer id{" "}
          <code>{offer.id.toString()}</code>.
        </span>
      </dd>
      <dt className="account-map__name">Vault</dt>
      <dd className="account-map__value">
        <AddressLink address={offer.vault} />
        <span className="account-map__explanation">
          The offer account's own {offer.mintA.symbol} account, holding the tokens on offer. Only the program can move
          them, by signing as the offer account.
        </span>
      </dd>
      <dt className="account-map__name">Maker</dt>
      <dd className="account-map__value">
        <AddressLink address={offer.maker} />
        <span className="account-map__explanation">
          Made the offer, paid both accounts' rent, and gets that rent back when the offer closes.
        </span>
      </dd>
      <dt className="account-map__name">Token A</dt>
      <dd className="account-map__value">
        <AddressLink address={offer.mintA.address} />
        <span className="account-map__explanation">
          {offer.mintA.name} ({offer.mintA.symbol}), {offer.mintA.decimals} decimal places. What the maker offers.
        </span>
      </dd>
      <dt className="account-map__name">Token B</dt>
      <dd className="account-map__value">
        <AddressLink address={offer.mintB.address} />
        <span className="account-map__explanation">
          {offer.mintB.name} ({offer.mintB.symbol}), {offer.mintB.decimals} decimal places. What the maker wants.
        </span>
      </dd>
    </dl>
  </section>
);
