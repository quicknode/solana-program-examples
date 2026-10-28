import type { Metadata } from "next";
import { GuidePanel } from "@/components/guide-panel";
import { MakeOfferForm } from "@/components/make-offer-form";
import { UnwrapSolNotice } from "@/components/unwrap-sol-notice";

export const metadata: Metadata = { title: "Make an offer" };

const MakeOfferPage = () => (
  <>
    <header className="page-intro">
      <h1>Make an offer</h1>
      <p className="page-intro__lede">
        Lock up tokens you hold and name the tokens you want for them. Anyone can take the offer, and when they do, both
        sides move at once. Until then you can cancel and take everything back.
      </p>
    </header>
    <div className="page-columns">
      <div className="offer-detail">
        <UnwrapSolNotice />
        <MakeOfferForm />
      </div>
      <GuidePanel
        title="What making an offer does"
        steps={[
          <>
            Your wallet signs one transaction that calls the program's <code>make_offer</code> handler.
          </>,
          <>
            The program creates an offer account holding your terms, and a vault: a token account the offer account
            owns.
          </>,
          <>Your tokens move into the vault. From here only the program can move them, and only two ways.</>,
          <>
            A taker who sends you what you asked for gets the vault's tokens. Or you cancel, and they come back to you.
          </>,
        ]}
        footer={
          <>
            Each offer also gets a random id, so you can keep several open at once: the id is part of the offer
            account's address.
          </>
        }
      />
    </div>
  </>
);

export default MakeOfferPage;
