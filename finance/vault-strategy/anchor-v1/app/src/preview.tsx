import { BN } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import ReactDOM from "react-dom/client";
import "./index.css";
import { ManagerView } from "./components/ManagerView";
import type { VaultState } from "./hooks/useVault";
import type { StrategyAccount } from "./idl/vaultStrategy";
import type { AssetView, StrategyView } from "./solana/strategy";
import { InvestorView } from "./views/InvestorView";

// ────────────────────────────────────────────────────────────────────────────
// DEV-ONLY design preview. Renders InvestorView with fabricated data so the flagship
// view can be reviewed without a deployed devnet program. NOT wired into the real app
// (main.tsx / index.html): the shipped app only ever renders live account reads.
// ────────────────────────────────────────────────────────────────────────────

const k = () => PublicKey.unique();

const asset = (
  index: number,
  price: bigint,
  vaultAmount: bigint,
  valueUsdc: bigint,
  weightBps: number,
  actualWeight: number,
): AssetView => ({
  index,
  config: k(),
  mint: k(),
  vault: k(),
  priceFeed: k(),
  weightBps,
  vaultAmount,
  price,
  publishTime: 1_900_000_000,
  stale: false,
  valueUsdc,
  actualWeight,
});

const account: StrategyAccount = {
  index: new BN(0),
  manager: k(),
  registry: k(),
  shareMint: k(),
  usdcMint: k(),
  swapRouter: k(),
  feeBps: 100,
  maxSlippageBps: 100,
  totalShares: new BN("12600000000"),
  lastFeeAccrualTimestamp: new BN("1900000000"),
  assetCount: 2,
  totalWeightBps: 10_000,
  bump: 255,
};

const view: StrategyView = {
  exists: true,
  index: 0n,
  strategy: k(),
  shareMint: account.shareMint,
  usdcVault: k(),
  account,
  usdcAmount: 0n,
  assets: [
    asset(0, 25_000_000_000n, 20_545_200n, 5_136_300_000n, 4000, 0.4),
    asset(1, 18_000_000_000n, 42_802_500n, 7_704_450_000n, 6000, 0.6),
  ],
  navMinor: 12_840_750_000n,
  navComplete: true,
  totalShares: 12_600_000_000n,
  navPerShareMinor: 1_019_107n,
  fullyAllocated: true,
};

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const mockSig = "Prev1ewS1gnatureX9mB4kZ2qWvR7tNjHcYfDpL3sAe8uK1oI5rT6yUw";

const act = async () => {
  await sleep(500);
  return mockSig;
};

const vault: VaultState = {
  loading: false,
  error: null,
  view,
  position: {
    shares: 3_150_000_000n,
    ownership: 0.25,
    valueMinor: 3_210_187_500n,
    shareAccount: k(),
    shareAccountExists: true,
  },
  walletUsdc: 5_000_000_000n,
  connected: true,
  isManager: true,
  refresh: () => {},
  deposit: act,
  redeem: act,
  rebalance: act,
  setWeight: act,
  addAsset: act,
  collectFees: act,
  createStrategy: act,
};

ReactDOM.createRoot(document.getElementById("root")!).render(
  <div className="flex min-h-full flex-col">
    <header className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-6 py-4">
      <div className="flex items-baseline gap-3">
        <span className="inline-block h-3.5 w-3.5 translate-y-[1px] bg-accent" aria-hidden />
        <span className="font-sans text-[15px] font-bold tracking-tight text-ink">VAULT STRATEGY</span>
        <span className="font-mono text-[13px] text-faint">/ #0</span>
      </div>
      <div className="flex items-center gap-3">
        <span className="inline-flex items-center gap-1.5 border border-line px-2 py-1 text-[11px] uppercase tracking-widest text-muted">
          <span className="h-1.5 w-1.5 rounded-full bg-gain" /> devnet
        </span>
        <span className="rounded-[3px] bg-accent px-3 py-[7px] font-sans text-[13px] font-semibold text-graphite">
          7xQ…4mB
        </span>
      </div>
    </header>
    <main className="flex-1">
      <div className="border-b border-line px-6 py-2 font-mono text-[11px] uppercase tracking-widest text-faint">
        Investor tab
      </div>
      <InvestorView vault={vault} />
      <div className="border-y-4 border-line/60 bg-panel px-6 py-2 font-mono text-[11px] uppercase tracking-widest text-faint">
        Manager tab
      </div>
      <ManagerView vault={vault} />
    </main>
    <footer className="border-t border-line px-6 py-4 text-[11px] leading-relaxed text-faint">
      Design preview · fabricated data. The shipped app renders only live on-chain reads.
    </footer>
  </div>,
);
