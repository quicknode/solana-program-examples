import Link from "next/link";
import { GuidePanel } from "@/components/guide-panel";
import { OfferBoard } from "@/components/offer-board";

const OffersPage = () => (
  <>
    <header className="page-intro">
      <h1>Open offers</h1>
      <p className="page-intro__lede">
        Each offer is one person's tokens, locked in a vault by the escrow program, waiting for someone to send the
        tokens they asked for in return. Find one you want and take it: both sides move in one transaction, or neither
        does.
      </p>
    </header>
    <div className="page-columns">
      <OfferBoard />
      <GuidePanel
        title="How to take an offer"
        steps={[
          <>Connect a wallet with the top-right button.</>,
          <>
            Narrow the list to the token you want to get and the token you can pay with. Cheapest first puts the best
            price at the top.
          </>,
          <>Each card says whether your wallet holds enough to pay. Open one to see exactly what moves, and where.</>,
          <>
            Take it. You sign one transaction: your tokens go to the maker, the vault's tokens come to you, and the
            program closes the offer.
          </>,
        ]}
        footer={
          <>
            Want to sell instead? <Link href="/make">Make your own offer</Link>. New to escrow?{" "}
            <Link href="/how-it-works">Read how it works</Link>.
          </>
        }
      />
    </div>
  </>
);

export default OffersPage;
