"use client";

import { isAddress, lamports, sequentialInstructionPlan } from "@solana/kit";
import { useAirdrop } from "@solana/react";
import Link from "next/link";
import { useState } from "react";
import { useConnectedAddress, useSolBalance, useWalletTokens } from "@/hooks/use-chain-data";
import { useEscrowClient } from "@/hooks/use-escrow-client";
import { useTransaction } from "@/hooks/use-transaction";
import { useWalletConnection } from "@/hooks/use-wallet";
import { AmountError, formatAmount, parseAmount } from "@/lib/amounts";
import { CLUSTER } from "@/lib/config";
import { describeTransactionError, isReportableError } from "@/lib/errors";
import { buildCreateTestTokenPlan, buildMintTestTokenPlan, TEST_TOKENS } from "@/lib/test-tokens";
import type { WalletToken } from "@/lib/wallet-tokens";
import { useChainActivity } from "./chain-activity";
import { TokenAmount } from "./token-amount";
import { TransactionError, TransactionSuccess } from "./transaction-notice";

const SOL_DECIMALS = 9;
const ONE_SOL = lamports(1_000_000_000n);

// Everything needed to try the escrow on a test cluster: SOL for fees and
// rent, two tokens to swap, and a way to hand some of them to a second wallet
// so it can take the first wallet's offers.
export const TestTokens = () => {
  const client = useEscrowClient();
  const connected = useWalletConnection();
  const viewer = useConnectedAddress();
  const solBalance = useSolBalance();
  const walletTokens = useWalletTokens();
  const airdrop = useAirdrop(client);
  const createTokens = useTransaction();
  const { announceChange } = useChainActivity();

  if (!connected || !viewer) {
    return (
      <div className="empty-state">
        <h2>Connect a wallet to get test tokens</h2>
        <p>Test SOL and test tokens go to the wallet you connect.</p>
      </div>
    );
  }

  const mintableTokens = (walletTokens.data ?? []).filter((token) => token.mint.mintAuthority === viewer);

  return (
    <div className="offer-detail">
      <section className="action-panel" aria-labelledby="sol-heading">
        <h2 className="action-panel__title" id="sol-heading">
          1. Test SOL
        </h2>
        <p>
          Every transaction costs a small fee in SOL, and every new account holds a rent deposit in SOL. You hold{" "}
          {solBalance.data === undefined ? "…" : `${formatAmount(solBalance.data, SOL_DECIMALS)} SOL`}.
        </p>
        <div className="notice__actions">
          <button
            type="button"
            className="button"
            disabled={airdrop.isRunning}
            onClick={async () => {
              try {
                await airdrop.dispatchAsync(viewer, ONE_SOL);
                announceChange();
              } catch {
                // The error is on airdrop.error, shown below.
              }
            }}
          >
            {airdrop.isRunning ? "Asking the faucet…" : `Get 1 SOL from the ${CLUSTER} faucet`}
          </button>
        </div>
        {isReportableError(airdrop.error) ? (
          <p className="field__error">
            The faucet said no: {describeTransactionError(airdrop.error)} Public faucets limit how often they pay out;
            try again later, or use <a href="https://faucet.solana.com">faucet.solana.com</a>.
          </p>
        ) : null}
      </section>

      <section className="action-panel" aria-labelledby="create-heading">
        <h2 className="action-panel__title" id="create-heading">
          2. Two test tokens
        </h2>
        <p>
          Creates two new tokens, with your wallet as the only one allowed to mint them, and mints you{" "}
          {TEST_TOKENS.map((spec) => `${formatAmount(spec.initialAmount, spec.decimals)} ${spec.symbol}`).join(" and ")}
          . They stand in for the book's USDC and TSLAx, and are worth nothing.
        </p>
        <button
          type="button"
          className="button button--primary"
          disabled={!connected.signer || createTokens.isRunning}
          onClick={async () => {
            const authority = connected.signer;
            if (!authority) {
              return;
            }
            try {
              await createTokens.dispatchAsync(async () => {
                const created = await Promise.all(
                  TEST_TOKENS.map((spec) => buildCreateTestTokenPlan(client, authority, spec)),
                );
                return sequentialInstructionPlan(created.map(({ plan }) => plan));
              });
            } catch {
              // The error is on createTokens.error, shown below.
            }
          }}
        >
          {createTokens.isRunning ? "Approve in your wallet, then wait for the network…" : "Create test tokens"}
        </button>
        {createTokens.data ? (
          <TransactionSuccess title="Test tokens created" signature={createTokens.data}>
            <p>
              They are in your wallet, and listed below. <Link href="/make">Make an offer</Link> with them.
            </p>
          </TransactionSuccess>
        ) : null}
        {isReportableError(createTokens.error) ? (
          <TransactionError error={createTokens.error} title="The test tokens were not created" />
        ) : null}
      </section>

      <section className="action-panel" aria-labelledby="mint-heading">
        <h2 className="action-panel__title" id="mint-heading">
          3. Tokens you can mint
        </h2>
        {mintableTokens.length === 0 ? (
          <p className="loading-text">
            {walletTokens.data === undefined ? "Reading your wallet…" : "None yet. Create the test tokens above."}
          </p>
        ) : (
          <>
            <p>
              Mint more to yourself, or to a second wallet so it has something to take your offers with: paste its
              address as the recipient.
            </p>
            <ul className="test-token-list">
              {mintableTokens.map((token) => (
                <li key={token.mint.address}>
                  <MintableToken token={token} />
                </li>
              ))}
            </ul>
          </>
        )}
      </section>
    </div>
  );
};

const MintableToken = ({ token }: { token: WalletToken }) => {
  const connected = useWalletConnection();
  const viewer = useConnectedAddress();
  const mint = useTransaction();
  const [amountText, setAmountText] = useState("");
  const [recipientText, setRecipientText] = useState("");

  const recipient = recipientText.trim() === "" ? viewer : recipientText.trim();
  let amount: bigint | null = null;
  let amountError: string | null = null;
  if (amountText.trim() !== "") {
    try {
      amount = parseAmount(amountText, token.mint.decimals);
    } catch (thrownObject) {
      if (!(thrownObject instanceof AmountError)) {
        throw thrownObject;
      }
      amountError = thrownObject.message;
    }
  }
  const recipientIsValid = recipient !== null && isAddress(recipient);

  return (
    <div className="test-token">
      <div className="test-token__header">
        <h3>{token.mint.symbol}</h3>
        <span>
          You hold <TokenAmount amount={token.amount} mint={token.mint} />
        </span>
      </div>
      <form
        className="test-token__form"
        onSubmit={async (event) => {
          event.preventDefault();
          const authority = connected?.signer;
          if (!authority || amount === null || amount === 0n || recipient === null || !isAddress(recipient)) {
            return;
          }
          try {
            await mint.dispatchAsync(() => buildMintTestTokenPlan(authority, token.mint, recipient, amount));
          } catch {
            // The error is on mint.error, shown below.
          }
        }}
      >
        <label className="field">
          <span className="field__label">Amount</span>
          <input
            className="field__control"
            inputMode="decimal"
            autoComplete="off"
            placeholder="100"
            value={amountText}
            onChange={(event) => setAmountText(event.target.value)}
          />
        </label>
        <label className="field">
          <span className="field__label">Recipient</span>
          <input
            className="field__control address"
            autoComplete="off"
            spellCheck={false}
            placeholder="You"
            value={recipientText}
            onChange={(event) => setRecipientText(event.target.value)}
          />
        </label>
        {amountError ? <span className="field__error">{amountError}</span> : null}
        {!recipientIsValid ? <span className="field__error">The recipient is not a Solana address.</span> : null}
        <button
          type="submit"
          className="button"
          disabled={amount === null || amount === 0n || !recipientIsValid || mint.isRunning}
        >
          {mint.isRunning ? "Minting…" : `Mint ${token.mint.symbol}`}
        </button>
      </form>
      {mint.data ? <TransactionSuccess title="Minted" signature={mint.data} /> : null}
      {isReportableError(mint.error) ? <TransactionError error={mint.error} title="Nothing was minted" /> : null}
    </div>
  );
};
