import type { Metadata } from "next";
import Link from "next/link";

export const metadata: Metadata = { title: "How it works" };

const SECTIONS: Array<{ id: string; title: string }> = [
  { id: "escrow", title: "What an escrow does" },
  { id: "handlers", title: "Three instructions" },
  { id: "accounts", title: "The offer and its vault" },
  { id: "bounds", title: "Why a take carries its terms" },
  { id: "costs", title: "Who pays for what" },
  { id: "refusals", title: "What the program refuses" },
  { id: "further", title: "Read further" },
];

const SOURCE = "https://github.com/quicknode/solana-program-examples/tree/main/finance/escrow";

const HowItWorksPage = () => (
  <>
    <header className="page-intro">
      <h1>How the escrow works</h1>
      <p className="page-intro__lede">
        Alice wants one share of Tesla stock, as the token TSLAx, and will pay 250 USDC for it. Bob holds a TSLAx and
        likes the price. Neither trusts the other to pay first. This program lets them trade anyway.
      </p>
    </header>
    <div className="guide">
      <div className="guide__body">
        <section className="guide-section" id="escrow">
          <h2>What an escrow does</h2>
          <p>
            If Alice sends Bob her USDC first, Bob can keep it and never send the TSLAx. The traditional fix is a third
            party who holds Alice's money until Bob delivers, and charges a fee for the favor.
          </p>
          <p>
            Here, a Solana program is the third party. Alice's USDC waits in a vault that only the program's code can
            move. The program takes no fee and has no operator: nobody holds a key that can sign for the vault. When Bob
            pays, the same transaction that moves his TSLAx to Alice moves the vault's USDC to Bob. Both happen, or
            neither does.
          </p>
        </section>

        <section className="guide-section" id="handlers">
          <h2>Three instructions</h2>
          <p>
            The maker is whoever makes an offer; the taker is whoever takes it. The program has three instruction
            handlers, and an offer is always in one of two states: open, or closed for good.
          </p>
          <table className="handler-table">
            <thead>
              <tr>
                <th className="handler-table__heading" scope="col">
                  Handler
                </th>
                <th className="handler-table__heading" scope="col">
                  Signed by
                </th>
                <th className="handler-table__heading" scope="col">
                  What it does
                </th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td className="handler-table__cell">
                  <code>make_offer</code>
                </td>
                <td className="handler-table__cell">The maker</td>
                <td className="handler-table__cell">
                  Creates the offer account and its vault, moves the maker's token A into the vault, and records how
                  much token B the maker wants. On this site: <Link href="/make">Make an offer</Link>.
                </td>
              </tr>
              <tr>
                <td className="handler-table__cell">
                  <code>take_offer</code>
                </td>
                <td className="handler-table__cell">The taker</td>
                <td className="handler-table__cell">
                  Moves the wanted token B from the taker to the maker, moves the vault's token A to the taker, and
                  closes the vault and the offer. On this site: pick an offer from <Link href="/">the list</Link>.
                </td>
              </tr>
              <tr>
                <td className="handler-table__cell">
                  <code>cancel_offer</code>
                </td>
                <td className="handler-table__cell">The maker only</td>
                <td className="handler-table__cell">
                  Moves the vault's token A back to the maker and closes the vault and the offer. Without it, tokens
                  nobody wanted would be stuck in the vault forever.
                </td>
              </tr>
            </tbody>
          </table>
          <p>
            Offers are all or nothing: nobody can take half of one. So the program never divides, and there is no
            rounding to argue about. The taker pays exactly the amount the offer asks for and gets exactly what the
            vault holds.
          </p>
        </section>

        <section className="guide-section" id="accounts">
          <h2>The offer and its vault</h2>
          <p>
            The <strong>offer account</strong> holds the terms: an id, the maker's address, the two token mints, and how
            much of token B the maker wants. Its address is a program derived address (PDA) of the seeds{" "}
            <code>"offer"</code>, the maker's address, and the id. Anyone can work out where a given maker's offer
            lives, and because the maker's address is a seed, two makers can never collide. The id lets one maker keep
            several offers open at once; this site picks a random one for each offer you make.
          </p>
          <p>
            The <strong>vault</strong> is the offer account's associated token account for token A. Its owner is the
            offer account, which has no private key. Only the escrow program can sign for a PDA it derived, so only the
            escrow program can move the vault's tokens, and its code only ever moves them to the taker or back to the
            maker.
          </p>
          <p>
            Nothing else is the program's: the taker's and maker's own token accounts are ordinary associated token
            accounts, and the program checks that every account passed in is the one the offer's stored terms say it
            must be.
          </p>
        </section>

        <section className="guide-section" id="bounds">
          <h2>Why a take carries its terms</h2>
          <p>
            An offer's address comes from its maker and id, and an id is free again once its offer closes. So a maker
            could watch for a take, cancel the offer, and make a worse one under the same id, at the same address,
            before the taker's transaction lands. The taker's transaction would then hit the new offer.
          </p>
          <p>
            To stop that bait and switch, <code>take_offer</code> takes two numbers from the taker: the least token A
            they will accept (<code>minimum_token_a_out</code>) and the most token B they will pay (
            <code>maximum_token_b_in</code>). Before any token moves, the program refuses the take with{" "}
            <code>OfferTermsChanged</code> if the vault holds less than the first or the offer wants more than the
            second. This site fills both in from the offer you were shown, so you only ever trade on the terms you saw.
          </p>
        </section>

        <section className="guide-section" id="costs">
          <h2>Who pays for what</h2>
          <ul className="guide-section__list">
            <li>
              The program charges nothing. Each side receives all of what the other offered. Solana's transaction fee
              still applies, paid by whoever sends the transaction.
            </li>
            <li>
              Every account holds a rent deposit in SOL. The maker pays the deposits for the offer account and the
              vault, and gets both back when the offer is taken or cancelled.
            </li>
            <li>
              If the maker has no account for token B yet, <code>make_offer</code> creates one, paid for by the maker,
              so a taker never pays for an account that belongs to the maker.
            </li>
            <li>
              SOL trades as a token, the native mint: lamports held in a token account. This site wraps your SOL into
              that account when you offer or pay SOL, and unwraps it when you receive SOL or cancel, in the same
              transaction, so you only ever see SOL.
            </li>
            <li>
              If the taker has no account for token A yet, <code>take_offer</code> creates one, paid for by the taker.
              It is the taker's account and stays theirs.
            </li>
          </ul>
        </section>

        <section className="guide-section" id="refusals">
          <h2>What the program refuses</h2>
          <ul className="guide-section__list">
            <li>
              An offer of zero tokens on either side, with <code>ZeroAmount</code>.
            </li>
            <li>
              An offer of a token for itself. The maker's token A and token B accounts would be the same account, and
              Anchor refuses the same writable account twice.
            </li>
            <li>A cancel signed by anyone but the maker.</li>
            <li>
              A take that lands on different terms than the taker signed for, with <code>OfferTermsChanged</code>.
            </li>
            <li>
              A take that names the wrong account anywhere: a different vault, the taker's own account in place of the
              maker's, or a different mint for token B. Each account must be the one the offer's stored terms derive.
            </li>
          </ul>
          <p>
            The program's tests run each of these, including the bait and switch, in{" "}
            <code>test_take_offer_rejects_switched_offer</code> and{" "}
            <code>test_take_offer_rejects_switched_offer_wanting_more_token_b</code>.
          </p>
        </section>

        <section className="guide-section" id="further">
          <h2>Read further</h2>
          <p>
            The Escrow chapter of <a href="https://solanabook.org">Building Financial Software on Solana</a> walks
            through this program handler by handler, with figures of every account each one touches. The book is free to
            download.
          </p>
          <p>
            The program's source, in Anchor 1, Anchor 2, Quasar and native Rust, is at{" "}
            <a href={SOURCE}>finance/escrow</a> in the companion repository, along with this site's own source.
          </p>
        </section>
      </div>

      <nav className="guide-contents" aria-label="On this page">
        <span className="guide-contents__title">On this page</span>
        <ol className="guide-contents__list">
          {SECTIONS.map((section) => (
            <li key={section.id}>
              <a className="guide-contents__link" href={`#${section.id}`}>
                {section.title}
              </a>
            </li>
          ))}
        </ol>
      </nav>
    </div>
  </>
);

export default HowItWorksPage;
