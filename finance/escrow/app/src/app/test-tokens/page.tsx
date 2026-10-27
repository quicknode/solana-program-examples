import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { GuidePanel } from "@/components/guide-panel";
import { TestTokens } from "@/components/test-tokens";
import { IS_TEST_CLUSTER } from "@/lib/config";

export const metadata: Metadata = { title: "Test tokens" };

const TestTokensPage = () => {
  // Mainnet has no faucet, and minting worthless tokens there is not a test.
  if (!IS_TEST_CLUSTER) {
    notFound();
  }
  return (
    <>
      <header className="page-intro">
        <h1>Test tokens</h1>
        <p className="page-intro__lede">
          Everything you need to try both sides of an escrow on a test cluster, where tokens are free and worth nothing.
        </p>
      </header>
      <div className="page-columns">
        <TestTokens />
        <GuidePanel
          title="Try both sides with two wallets"
          steps={[
            <>
              Most wallets hold several accounts. Call the first Alice and the second Bob, and get each of them some
              test SOL.
            </>,
            <>As Alice, create the test tokens. Alice now holds 1,000 tUSDC and 10 tTSLAx.</>,
            <>As Alice, mint 1 tTSLAx to Bob's address, so Bob has something to pay with.</>,
            <>As Alice, make an offer: 250 tUSDC for 1 tTSLAx.</>,
            <>
              Switch your wallet to Bob, find Alice's offer, and take it. Bob ends with 250 tUSDC; Alice with 1 tTSLAx
              more.
            </>,
          ]}
          footer={
            <>This is the walkthrough the book's Escrow chapter tells, with test tokens standing in for real ones.</>
          }
        />
      </div>
    </>
  );
};

export default TestTokensPage;
