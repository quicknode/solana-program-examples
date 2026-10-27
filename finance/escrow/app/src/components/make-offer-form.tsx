"use client";

import { type Address, isAddress } from "@solana/kit";
import { useRequest } from "@solana/react";
import { TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useMemo, useState } from "react";
import { findOfferPda } from "@/generated";
import { useConnectedAddress, useOffers, useSolBalance, useWalletTokens } from "@/hooks/use-chain-data";
import { useEscrowClient } from "@/hooks/use-escrow-client";
import { useEscrowRent } from "@/hooks/use-rent";
import { useTransaction } from "@/hooks/use-transaction";
import { useWalletConnection } from "@/hooks/use-wallet";
import { AmountError, formatAmount, formatPrice, parseAmount, unitPrice } from "@/lib/amounts";
import { ESCROW_PROGRAM, IS_TEST_CLUSTER } from "@/lib/config";
import { isReportableError } from "@/lib/errors";
import { buildMakeOfferInstruction, generateOfferId } from "@/lib/escrow-instructions";
import { fetchMintInfo, type MintInfo } from "@/lib/tokens";
import { TokenAmount } from "./token-amount";
import { TransactionError } from "./transaction-notice";

const SOL_DECIMALS = 9;
const OTHER_TOKEN = "other";
const FEE_ALLOWANCE_LAMPORTS = 100_000n;

type ParsedAmount = { value: bigint; error: null } | { value: null; error: string | null };

const readAmount = (text: string, mint: MintInfo | undefined): ParsedAmount => {
  if (!mint || text.trim() === "") {
    return { value: null, error: null };
  }
  try {
    const value = parseAmount(text, mint.decimals);
    return value === 0n ? { value: null, error: "An offer needs more than zero tokens" } : { value, error: null };
  } catch (thrownObject) {
    if (thrownObject instanceof AmountError) {
      return { value: null, error: thrownObject.message };
    }
    throw thrownObject;
  }
};

// The maker's form: pick a token you hold and how much of it to lock up, pick
// the token you want and how much of it, check the result, and send
// make_offer.
export const MakeOfferForm = () => {
  const client = useEscrowClient();
  const router = useRouter();
  const connected = useWalletConnection();
  const viewer = useConnectedAddress();
  const walletTokens = useWalletTokens();
  const offers = useOffers();
  const solBalance = useSolBalance();
  const rent = useEscrowRent();
  const make = useTransaction();

  const [mintAddressA, setMintAddressA] = useState("");
  const [amountTextA, setAmountTextA] = useState("");
  const [mintChoiceB, setMintChoiceB] = useState("");
  const [otherMintText, setOtherMintText] = useState("");
  const [amountTextB, setAmountTextB] = useState("");

  const heldTokens = (walletTokens.data ?? []).filter((token) => token.amount > 0n);
  const holdingA = heldTokens.find((token) => token.mint.address === mintAddressA);
  const mintA = holdingA?.mint;

  // Tokens a maker might ask for: anything already traded here, and anything
  // in their own wallet.
  const wantableMints = useMemo(() => {
    const byAddress = new Map<string, MintInfo>();
    for (const offer of offers.data ?? []) {
      byAddress.set(offer.mintA.address, offer.mintA);
      byAddress.set(offer.mintB.address, offer.mintB);
    }
    for (const token of walletTokens.data ?? []) {
      byAddress.set(token.mint.address, token.mint);
    }
    return [...byAddress.values()].sort((first, second) => first.symbol.localeCompare(second.symbol));
  }, [offers.data, walletTokens.data]);

  const trimmedOtherMint = otherMintText.trim();
  const otherMintAddress: Address | null =
    mintChoiceB === OTHER_TOKEN && isAddress(trimmedOtherMint) ? trimmedOtherMint : null;
  const otherMintSource = useMemo(
    () =>
      otherMintAddress ? (abortSignal: AbortSignal) => fetchMintInfo(client.rpc, otherMintAddress, abortSignal) : null,
    [client, otherMintAddress],
  );
  const otherMint = useRequest(otherMintSource);
  const mintB =
    mintChoiceB === OTHER_TOKEN
      ? (otherMint.data ?? undefined)
      : wantableMints.find((mint) => mint.address === mintChoiceB);

  const amountA = readAmount(amountTextA, mintA);
  const amountB = readAmount(amountTextB, mintB);

  const problems: Array<string> = [];
  if (mintA && mintB && mintA.address === mintB.address) {
    problems.push("An offer swaps two different tokens. Pick a different token to ask for.");
  }
  if (mintA && mintB && mintA.tokenProgram !== mintB.tokenProgram) {
    problems.push(
      `${mintA.symbol} and ${mintB.symbol} belong to different token programs (one is a Token-2022 token, the other is not). The escrow program moves both tokens with one token program, so it can only swap two tokens of the same kind.`,
    );
  }
  if (holdingA && amountA.value !== null && amountA.value > holdingA.amount) {
    problems.push(`You hold ${formatAmount(holdingA.amount, holdingA.mint.decimals)} ${holdingA.mint.symbol}.`);
  }

  const vaultRent =
    mintA?.tokenProgram === TOKEN_2022_PROGRAM_ADDRESS ? rent.data?.token2022Account : rent.data?.classicTokenAccount;
  const rentDeposit = rent.data && vaultRent !== undefined ? rent.data.offerAccount + vaultRent : undefined;
  if (
    rentDeposit !== undefined &&
    solBalance.data !== undefined &&
    solBalance.data < rentDeposit + FEE_ALLOWANCE_LAMPORTS
  ) {
    problems.push(
      `Making an offer deposits ${formatAmount(rentDeposit, SOL_DECIMALS)} SOL of rent, and you hold ${formatAmount(solBalance.data, SOL_DECIMALS)} SOL.`,
    );
  }

  const isComplete = mintA !== undefined && mintB !== undefined && amountA.value !== null && amountB.value !== null;
  const canSubmit = isComplete && problems.length === 0 && connected?.signer && !make.isRunning;

  if (!connected || !viewer) {
    return (
      <div className="empty-state">
        <h2>Connect a wallet to make an offer</h2>
        <p>The form lists the tokens your wallet holds, so it needs to know which wallet that is.</p>
      </div>
    );
  }

  if (walletTokens.error) {
    return (
      <div className="offer-detail">
        <TransactionError error={walletTokens.error} title="Could not read your wallet's tokens" />
        <button type="button" className="button" onClick={() => walletTokens.refresh()}>
          Try again
        </button>
      </div>
    );
  }

  if (walletTokens.data === undefined) {
    return <p className="loading-text">Reading your wallet's tokens…</p>;
  }

  if (heldTokens.length === 0) {
    return (
      <div className="empty-state">
        <h2>Your wallet holds no tokens to offer</h2>
        <p>An offer locks up tokens you already hold, and this wallet holds none.</p>
        {IS_TEST_CLUSTER ? (
          <Link className="button button--primary" href="/test-tokens">
            Get test tokens
          </Link>
        ) : null}
      </div>
    );
  }

  return (
    <form
      className="make-offer-form"
      onSubmit={async (event) => {
        event.preventDefault();
        const maker = connected.signer;
        if (!maker || !mintA || !mintB || amountA.value === null || amountB.value === null) {
          return;
        }
        const id = generateOfferId();
        try {
          await make.dispatchAsync(async () => [
            await buildMakeOfferInstruction({
              maker,
              mintA,
              mintB,
              offeredAmount: amountA.value,
              wantedAmount: amountB.value,
              id,
              programAddress: ESCROW_PROGRAM,
            }),
          ]);
          const [offerAddress] = await findOfferPda({ maker: maker.address, id }, { programAddress: ESCROW_PROGRAM });
          router.push(`/offers/${offerAddress}`);
        } catch {
          // The error is on make.error, shown below.
        }
      }}
    >
      <fieldset className="form-section">
        <legend className="form-section__legend">1. What you give</legend>
        <p className="form-section__description">
          These tokens leave your wallet now and wait in a vault the program controls, until someone takes the offer or
          you cancel it.
        </p>
        <label className="field">
          <span className="field__label">Token</span>
          <select
            className="field__control"
            value={mintAddressA}
            onChange={(event) => setMintAddressA(event.target.value)}
            required
          >
            <option value="" disabled>
              Choose a token you hold
            </option>
            {heldTokens.map((token) => (
              <option key={token.mint.address} value={token.mint.address}>
                {token.mint.symbol}: you hold {formatAmount(token.amount, token.mint.decimals)}
              </option>
            ))}
          </select>
        </label>
        <div className="field">
          <label className="field__label" htmlFor="amount-a">
            Amount
          </label>
          <div className="amount-input">
            <input
              id="amount-a"
              className="field__control"
              inputMode="decimal"
              autoComplete="off"
              placeholder="250"
              value={amountTextA}
              onChange={(event) => setAmountTextA(event.target.value)}
              disabled={!mintA}
            />
            {holdingA ? (
              <button
                type="button"
                className="text-button"
                onClick={() =>
                  setAmountTextA(formatAmount(holdingA.amount, holdingA.mint.decimals).replaceAll(",", ""))
                }
              >
                Max
              </button>
            ) : null}
          </div>
          {amountA.error ? <span className="field__error">{amountA.error}</span> : null}
        </div>
      </fieldset>

      <fieldset className="form-section">
        <legend className="form-section__legend">2. What you want</legend>
        <p className="form-section__description">
          Whoever takes the offer sends you exactly this much, straight to your wallet, in the same transaction that
          releases your tokens to them.
        </p>
        <label className="field">
          <span className="field__label">Token</span>
          <select
            className="field__control"
            value={mintChoiceB}
            onChange={(event) => setMintChoiceB(event.target.value)}
            required
          >
            <option value="" disabled>
              Choose a token
            </option>
            {wantableMints
              .filter((mint) => mint.address !== mintAddressA)
              .map((mint) => (
                <option key={mint.address} value={mint.address}>
                  {mint.symbol}: {mint.name}
                </option>
              ))}
            <option value={OTHER_TOKEN}>Another token, by mint address…</option>
          </select>
        </label>
        <div className="field">
          <label className="field__label" htmlFor="amount-b">
            Amount
          </label>
          <input
            id="amount-b"
            className="field__control"
            inputMode="decimal"
            autoComplete="off"
            placeholder="1"
            value={amountTextB}
            onChange={(event) => setAmountTextB(event.target.value)}
            disabled={!mintB}
          />
          {amountB.error ? <span className="field__error">{amountB.error}</span> : null}
        </div>
        {mintChoiceB === OTHER_TOKEN ? (
          <label className="field form-section__wide">
            <span className="field__label">Mint address</span>
            <input
              className="field__control address"
              autoComplete="off"
              spellCheck={false}
              placeholder="The token's mint address"
              value={otherMintText}
              onChange={(event) => setOtherMintText(event.target.value)}
            />
            {otherMintText.trim() !== "" && !otherMintAddress ? (
              <span className="field__error">That is not a Solana address.</span>
            ) : null}
            {otherMintAddress && otherMint.data === null ? (
              <span className="field__error">No token mint lives at that address on this cluster.</span>
            ) : null}
            {otherMint.data ? (
              <span className="field__hint">
                {otherMint.data.name} ({otherMint.data.symbol}), {otherMint.data.decimals} decimal places.
              </span>
            ) : null}
          </label>
        ) : null}
      </fieldset>

      <section className="action-panel offer-preview" aria-labelledby="preview-heading">
        <h2 className="action-panel__title" id="preview-heading">
          3. Check and make the offer
        </h2>
        {isComplete ? (
          <>
            <p className="offer-preview__sentence">
              You offer <TokenAmount amount={amountA.value} mint={mintA} moving /> for{" "}
              <TokenAmount amount={amountB.value} mint={mintB} />.
            </p>
            <p>
              That prices 1 {mintA.symbol} at{" "}
              {formatPrice(unitPrice(amountA.value, mintA.decimals, amountB.value, mintB.decimals))} {mintB.symbol}, and
              1 {mintB.symbol} at {formatPrice(unitPrice(amountB.value, mintB.decimals, amountA.value, mintA.decimals))}{" "}
              {mintA.symbol}.
            </p>
          </>
        ) : (
          <p className="loading-text">Choose both tokens and amounts to see your offer.</p>
        )}
        <ul className="checklist">
          {problems.map((problem) => (
            <li key={problem} className="checklist__item checklist__item--fail">
              <span>{problem}</span>
            </li>
          ))}
          <li className="checklist__item checklist__item--info">
            <span>
              You pay the rent for the offer account and the vault
              {rentDeposit === undefined ? "" : `: ${formatAmount(rentDeposit, SOL_DECIMALS)} SOL`}. It comes back to
              you when the offer is taken or cancelled. If you have no account yet for the token you want, the
              transaction creates one and you pay its rent too; that account stays yours.
            </span>
          </li>
          <li className="checklist__item checklist__item--info">
            <span>You can cancel at any time before someone takes it, and get your tokens back.</span>
          </li>
        </ul>
        <button type="submit" className="button button--primary button--wide" disabled={!canSubmit}>
          {make.isRunning ? "Approve in your wallet, then wait for the network…" : "Make offer"}
        </button>
        {isReportableError(make.error) ? <TransactionError error={make.error} title="The offer was not made" /> : null}
      </section>
    </form>
  );
};
