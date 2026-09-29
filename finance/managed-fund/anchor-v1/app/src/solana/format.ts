import type { PublicKey } from "@solana/web3.js";
import { CLUSTER, RPC_URL } from "./config";

function groupThousands(intDigits: string): string {
  return intDigits.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** Format a fixed-point integer (`minor`) with `decimals` places, showing `frac` of them. */
export function formatUnits(minor: bigint, decimals: number, frac = 2): string {
  const negative = minor < 0n;
  const abs = negative ? -minor : minor;
  const base = 10n ** BigInt(decimals);
  const whole = abs / base;
  const fraction = (abs % base).toString().padStart(decimals, "0");
  const shownFrac = frac <= 0 ? "" : `.${fraction.slice(0, frac).padEnd(frac, "0")}`;
  return `${negative ? "-" : ""}${groupThousands(whole.toString())}${shownFrac}`;
}

/** USDC amount (6dp minor units) → "1,363.50". */
export const formatUsdc = (minor: bigint, frac = 2): string => formatUnits(minor, 6, frac);

/** Share amount (6dp minor units) → "900.000000", trailing zeros trimmed to `minFrac`. */
export function formatShares(minor: bigint, minFrac = 2): string {
  const full = formatUnits(minor, 6, 6);
  if (!full.includes(".")) return full;
  const [w, f] = full.split(".");
  const trimmed = f.replace(/0+$/, "");
  const kept = trimmed.length < minFrac ? trimmed.padEnd(minFrac, "0") : trimmed;
  return kept ? `${w}.${kept}` : w;
}

/** Basis points → percent string, e.g. 100 → "1.00%". */
export const formatBps = (bps: number, frac = 2): string => `${(bps / 100).toFixed(frac)}%`;

/** A 0..1 ratio → percent string, e.g. 0.375 → "37.5%". */
export const formatRatioPct = (ratio: number, frac = 1): string => `${(ratio * 100).toFixed(frac)}%`;

export function shortAddress(value: PublicKey | string, edge = 4): string {
  const s = typeof value === "string" ? value : value.toBase58();
  return s.length <= edge * 2 + 1 ? s : `${s.slice(0, edge)}…${s.slice(-edge)}`;
}

function explorerSuffix(): string {
  if (CLUSTER === "mainnet-beta") return "";
  if (CLUSTER === "custom") return `?cluster=custom&customUrl=${encodeURIComponent(RPC_URL)}`;
  return `?cluster=${CLUSTER}`; // devnet / testnet
}

export const explorerTx = (signature: string): string =>
  `https://explorer.solana.com/tx/${signature}${explorerSuffix()}`;

export const explorerAddress = (address: PublicKey | string): string =>
  `https://explorer.solana.com/address/${typeof address === "string" ? address : address.toBase58()}${explorerSuffix()}`;
