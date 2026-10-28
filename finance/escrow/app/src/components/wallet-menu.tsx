"use client";

import { type Address, lamports } from "@solana/kit";
import { useConnect, useDisconnect, useWallets } from "@solana/kit-plugin-wallet/react";
import { useAirdrop } from "@solana/react";
import { useEffect, useId, useRef, useState } from "react";
import { useConnectedAddress, useSolBalance, useSolName } from "@/hooks/use-chain-data";
import { useEscrowClient } from "@/hooks/use-escrow-client";
import { useHydratedWalletStatus, useWalletConnection } from "@/hooks/use-wallet";
import { formatAmount } from "@/lib/amounts";
import { CLUSTER, IS_TEST_CLUSTER } from "@/lib/config";
import { describeTransactionError, isReportableError } from "@/lib/errors";
import { shortAddress } from "@/lib/tokens";
import { AddressLink } from "./address-link";
import { useChainActivity } from "./chain-activity";
import { UnwrapSolNotice } from "./unwrap-sol-notice";

const SOL_DECIMALS = 9;
const ONE_SOL = lamports(1_000_000_000n);

// The wallet button in the header: connect a Wallet Standard wallet, see the
// connected account and its SOL, and on a test cluster ask the faucet for
// more.
export const WalletMenu = () => {
  const client = useEscrowClient();
  const status = useHydratedWalletStatus();
  const wallets = useWallets(client);
  const connected = useWalletConnection();
  const connectedAddress = useConnectedAddress();
  const solName = useSolName();
  const connect = useConnect(client);
  const disconnect = useDisconnect(client);
  const [isOpen, setIsOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const panelId = useId();

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const closeOnOutsideClick = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", closeOnOutsideClick);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOnOutsideClick);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [isOpen]);

  // Hold the button back while the plugin silently reconnects the wallet used
  // last time, so a returning visitor never sees "Connect" flash past.
  if (status === "pending" || status === "reconnecting") {
    return (
      <button type="button" className="button button--quiet" disabled>
        Wallet…
      </button>
    );
  }

  return (
    <div className="wallet-menu" ref={menuRef}>
      <button
        type="button"
        className={connected ? "button button--quiet" : "button button--primary"}
        aria-expanded={isOpen}
        aria-controls={panelId}
        onClick={() => setIsOpen(!isOpen)}
      >
        {connectedAddress ? (solName ?? shortAddress(connectedAddress)) : "Connect wallet"}
      </button>
      {isOpen ? (
        <div className="wallet-menu__panel" id={panelId}>
          {connected && connectedAddress ? (
            <ConnectedAccount
              walletName={connected.wallet.name}
              address={connectedAddress}
              solName={solName}
              onDisconnect={() => {
                disconnect.dispatch();
                setIsOpen(false);
              }}
            />
          ) : (
            <>
              <h2 className="wallet-menu__heading">Connect a wallet</h2>
              {wallets.length === 0 ? (
                <p className="wallet-menu__note">
                  No wallet in this browser supports Solana {CLUSTER}. Install a Solana wallet extension such as
                  Phantom, Solflare or Backpack, then reload this page.
                </p>
              ) : (
                <ul className="wallet-list">
                  {wallets.map((wallet) => (
                    <li key={wallet.name}>
                      <button
                        type="button"
                        className="wallet-option"
                        disabled={connect.isRunning}
                        onClick={async () => {
                          try {
                            await connect.dispatchAsync(wallet);
                            setIsOpen(false);
                          } catch {
                            // The error is on connect.error, shown below.
                          }
                        }}
                      >
                        {/* Wallet icons are data: URIs, which next/image does not optimize. */}
                        {/* biome-ignore lint/performance/noImgElement: a data: URI needs no optimizing */}
                        <img className="wallet-option__icon" src={wallet.icon} alt="" />
                        <span>{wallet.name}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {isReportableError(connect.error) ? (
                <p className="field__error">{describeTransactionError(connect.error)}</p>
              ) : null}
              <p className="wallet-menu__note">
                Connecting shares your address with this page. Nothing moves until you approve a transaction in your
                wallet.
              </p>
            </>
          )}
        </div>
      ) : null}
    </div>
  );
};

type ConnectedAccountProps = {
  walletName: string;
  address: Address;
  solName: string | null;
  onDisconnect: () => void;
};

const ConnectedAccount = ({ walletName, address, solName, onDisconnect }: ConnectedAccountProps) => {
  const client = useEscrowClient();
  const balance = useSolBalance();
  const airdrop = useAirdrop(client);
  const { announceChange } = useChainActivity();

  return (
    <div className="wallet-account">
      <span className="wallet-account__label">Connected with {walletName}</span>
      {solName ? <span className="wallet-account__name">{solName}</span> : null}
      <AddressLink address={address} full />
      <span className="wallet-account__balance">
        {balance.data === undefined ? "…" : `${formatAmount(balance.data, SOL_DECIMALS)} SOL`}
      </span>
      {IS_TEST_CLUSTER ? (
        <button
          type="button"
          className="text-button"
          disabled={airdrop.isRunning}
          onClick={async () => {
            try {
              await airdrop.dispatchAsync(address, ONE_SOL);
              announceChange();
            } catch {
              // The error is on airdrop.error, shown below.
            }
          }}
        >
          {airdrop.isRunning ? "Asking the faucet…" : `Get 1 test SOL from the ${CLUSTER} faucet`}
        </button>
      ) : null}
      {isReportableError(airdrop.error) ? (
        <p className="field__error">
          The faucet said no: {describeTransactionError(airdrop.error)} Public faucets limit how often they pay out; try
          again later or use https://faucet.solana.com.
        </p>
      ) : null}
      <UnwrapSolNotice />
      <button type="button" className="button button--quiet" onClick={onDisconnect}>
        Disconnect
      </button>
    </div>
  );
};
