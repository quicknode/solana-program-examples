import { useConnection } from "@solana/wallet-adapter-react";
import { PublicKey } from "@solana/web3.js";
import { useEffect, useMemo, useState } from "react";
import type { VaultState } from "../hooks/useVault";
import { parseAmount, parsePercentToBps, toAmountInput } from "../lib/amounts";
import { describeError } from "../lib/tx";
import {
  BPS_DENOMINATOR,
  MAX_ASSETS,
  MAX_FEE_BPS,
  MAX_SLIPPAGE_BPS,
  PYTH_PRICE_PRECISION,
  ROUTER_PROGRAM_ID,
  STRATEGY_INDEX,
  USDC_MINT,
} from "../solana/config";
import { formatBps, formatShares, formatUnits, formatUsdc, shortAddress } from "../solana/format";
import { approvedAssetPda } from "../solana/pdas";
import type { StrategyView } from "../solana/strategy";
import { Button, Panel, Select, StatusLine, TextField, type TxStatus } from "./atoms";

const SECONDS_PER_YEAR = 31_536_000n;

/** Shared submit/status/busy wiring for a manager action. */
function useAction() {
  const [status, setStatus] = useState<TxStatus>({ kind: "idle" });
  const [busy, setBusy] = useState(false);
  async function run(fn: () => Promise<string>, verb: string) {
    setBusy(true);
    setStatus({ kind: "pending", message: "Confirm in your wallet…" });
    try {
      const signature = await fn();
      setStatus({ kind: "success", message: `${verb} confirmed.`, signature });
    } catch (err) {
      setStatus({ kind: "error", message: describeError(err) });
    } finally {
      setBusy(false);
    }
  }
  return { status, busy, run };
}

function tryPubkey(value: string): PublicKey | null {
  const t = value.trim();
  if (t.length < 32) return null;
  try {
    return new PublicKey(t);
  } catch {
    return null;
  }
}

const assetOptions = (view: StrategyView) =>
  view.assets.map((a) => ({
    value: String(a.index),
    label: `#${a.index} · ${shortAddress(a.mint)} · ${formatBps(a.weightBps, 0)}`,
  }));

// ---- Rebalance -------------------------------------------------------------

function RebalancePanel({ view, onRebalance }: { view: StrategyView; onRebalance: VaultState["rebalance"] }) {
  const { status, busy, run } = useAction();
  const [sell, setSell] = useState("0");
  const [buy, setBuy] = useState(view.assets.length > 1 ? "1" : "0");
  const [sellInput, setSellInput] = useState("");
  const [usdcInput, setUsdcInput] = useState("");

  if (view.assets.length < 2) {
    return (
      <Panel title="Rebalance">
        <p className="text-[13px] leading-relaxed text-muted">
          Rebalancing needs at least two assets. Add another asset first.
        </p>
      </Panel>
    );
  }

  const sellIdx = Number(sell);
  const buyIdx = Number(buy);
  const sellAsset = view.assets[sellIdx];
  const buyAsset = view.assets[buyIdx];
  const sellMinor = parseAmount(sellInput, 6);
  const usdcMinor = parseAmount(usdcInput, 6);
  const impliedUsdc =
    sellMinor !== null && sellAsset.price ? (sellMinor * sellAsset.price) / PYTH_PRICE_PRECISION : null;
  const legStale = sellAsset.price === null || sellAsset.stale || buyAsset.price === null || buyAsset.stale;

  const block =
    sellIdx === buyIdx
      ? "Choose two different assets."
      : legStale
        ? "An oracle price is stale or missing — the rebalance would revert on-chain."
        : sellMinor !== null && sellMinor > sellAsset.vaultAmount
          ? "Sell amount exceeds the vault balance."
          : null;
  const ready =
    sellIdx !== buyIdx &&
    !legStale &&
    sellMinor !== null &&
    sellMinor > 0n &&
    sellMinor <= sellAsset.vaultAmount &&
    usdcMinor !== null &&
    usdcMinor > 0n;

  return (
    <Panel title="Rebalance" hint="sell one asset, buy another">
      <div className="grid grid-cols-2 gap-3">
        <Select label="Sell" value={sell} onChange={setSell} options={assetOptions(view)} />
        <Select label="Buy" value={buy} onChange={setBuy} options={assetOptions(view)} />
      </div>
      <TextField
        label="Sell amount"
        value={sellInput}
        onChange={setSellInput}
        placeholder="0.00"
        right={
          <button
            type="button"
            onClick={() => setSellInput(toAmountInput(sellAsset.vaultAmount))}
            className="tabular-nums transition-colors hover:text-accent"
          >
            Vault {formatUnits(sellAsset.vaultAmount, 6, 4)} · Max
          </button>
        }
      />
      <TextField
        label="USDC to reinvest"
        value={usdcInput}
        onChange={setUsdcInput}
        suffix="USDC"
        placeholder="0.00"
        right={
          impliedUsdc !== null ? (
            <button
              type="button"
              onClick={() => setUsdcInput(toAmountInput(impliedUsdc))}
              className="tabular-nums transition-colors hover:text-accent"
            >
              Oracle ≈ {formatUsdc(impliedUsdc)} · Use
            </button>
          ) : (
            "enter sell amount"
          )
        }
      />
      {block && <p className="text-[12px] leading-relaxed text-muted">{block}</p>}
      <Button
        disabled={!ready || busy}
        onClick={() => run(() => onRebalance(sellIdx, buyIdx, sellMinor!, usdcMinor!), "Rebalance")}
      >
        {busy ? "Working…" : "Rebalance"}
      </Button>
      <StatusLine status={status} />
    </Panel>
  );
}

// ---- Set weight ------------------------------------------------------------

function SetWeightPanel({ view, onSetWeight }: { view: StrategyView; onSetWeight: VaultState["setWeight"] }) {
  const { status, busy, run } = useAction();
  const [idx, setIdx] = useState("0");
  const [pct, setPct] = useState("");
  const assetIdx = Number(idx);
  const asset = view.assets[assetIdx];
  const newBps = parsePercentToBps(pct);
  const oldBps = asset?.weightBps ?? 0;
  const newTotal = view.account!.totalWeightBps - oldBps + (newBps ?? 0);
  const invalid = pct.trim() !== "" && (newBps === null || newBps < 0 || newBps > BPS_DENOMINATOR);

  const block = invalid
    ? "Enter a weight from 0 to 100%."
    : newBps !== null && newTotal > BPS_DENOMINATOR
      ? `Total weight would reach ${formatBps(newTotal, 0)} — over 100%.`
      : null;
  const ready = newBps !== null && !invalid && newTotal <= BPS_DENOMINATOR;

  return (
    <Panel title="Set weight" hint="0% retires the asset">
      <Select label="Asset" value={idx} onChange={setIdx} options={assetOptions(view)} />
      <TextField
        label={`New target weight — currently ${formatBps(oldBps, 0)}`}
        value={pct}
        onChange={setPct}
        suffix="%"
        invalid={invalid}
        placeholder="0"
        right={
          <button type="button" onClick={() => setPct("0")} className="transition-colors hover:text-loss">
            Retire → 0%
          </button>
        }
      />
      {newBps !== null && !invalid && (
        <p className="font-mono text-[12px] text-faint">
          Resulting total <span className="text-muted">{formatBps(newTotal, 0)}</span>{" "}
          <span className={newTotal === BPS_DENOMINATOR ? "text-gain" : "text-accent"}>
            {newTotal === BPS_DENOMINATOR ? "live" : "configuring — deposits closed"}
          </span>
        </p>
      )}
      {block && <p className="text-[12px] leading-relaxed text-muted">{block}</p>}
      <Button disabled={!ready || busy} onClick={() => run(() => onSetWeight(assetIdx, newBps!), "Weight change")}>
        {busy ? "Working…" : "Set weight"}
      </Button>
      <StatusLine status={status} />
    </Panel>
  );
}

// ---- Add asset -------------------------------------------------------------

function AddAssetPanel({ view, onAddAsset }: { view: StrategyView; onAddAsset: VaultState["addAsset"] }) {
  const { connection } = useConnection();
  const { status, busy, run } = useAction();
  const [mintStr, setMintStr] = useState("");
  const [pct, setPct] = useState("");
  const [approved, setApproved] = useState<"checking" | boolean | null>(null);

  const mint = useMemo(() => tryPubkey(mintStr), [mintStr]);
  const registry = view.account!.registry;

  useEffect(() => {
    if (!mint) {
      setApproved(null);
      return;
    }
    let cancelled = false;
    setApproved("checking");
    connection
      .getAccountInfo(approvedAssetPda(registry, mint))
      .then((info) => !cancelled && setApproved(info !== null))
      .catch(() => !cancelled && setApproved(null)); // check failed; let the chain decide
    return () => {
      cancelled = true;
    };
  }, [mint, registry, connection]);

  const newBps = parsePercentToBps(pct);
  const newTotal = view.account!.totalWeightBps + (newBps ?? 0);
  const atCap = view.account!.assetCount >= MAX_ASSETS;

  const block = atCap
    ? `Basket is at the ${MAX_ASSETS}-asset cap.`
    : mintStr.trim() !== "" && !mint
      ? "Not a valid mint address."
      : approved === false
        ? "This mint isn’t approved by the curator — add_asset will fail."
        : newBps !== null && newTotal > BPS_DENOMINATOR
          ? `Total weight would reach ${formatBps(newTotal, 0)} — over 100%.`
          : null;
  const ready =
    !atCap && mint !== null && approved !== false && newBps !== null && newBps >= 0 && newTotal <= BPS_DENOMINATOR;

  return (
    <Panel title="Add asset" hint={`${view.account!.assetCount}/${MAX_ASSETS} assets`}>
      <TextField
        label="Approved mint address"
        value={mintStr}
        onChange={setMintStr}
        placeholder="Base58 mint…"
        invalid={mintStr.trim() !== "" && !mint}
        right={
          approved === "checking"
            ? "checking…"
            : approved === true
              ? "approved ✓"
              : approved === false
                ? "not approved"
                : ""
        }
      />
      <TextField label="Target weight" value={pct} onChange={setPct} suffix="%" placeholder="0" />
      {block && <p className="text-[12px] leading-relaxed text-muted">{block}</p>}
      <Button disabled={!ready || busy} onClick={() => run(() => onAddAsset(mint!, newBps!), "Add asset")}>
        {busy ? "Working…" : "Add asset"}
      </Button>
      <StatusLine status={status} />
    </Panel>
  );
}

// ---- Collect fees ----------------------------------------------------------

function CollectFeesPanel({ view, onCollect }: { view: StrategyView; onCollect: VaultState["collectFees"] }) {
  const { status, busy, run } = useAction();
  const s = view.account!;
  const now = Math.floor(Date.now() / 1000);
  const elapsed = BigInt(Math.max(0, now - s.lastFeeAccrualTimestamp.toNumber()));
  const feeShares = (view.totalShares * BigInt(s.feeBps) * elapsed) / (BigInt(BPS_DENOMINATOR) * SECONDS_PER_YEAR);
  const feeValue = view.totalShares > 0n ? (feeShares * view.navMinor) / view.totalShares : 0n;

  return (
    <Panel title="Collect fees" hint="permissionless">
      <div className="space-y-1 font-mono text-[12px]">
        <div className="flex justify-between">
          <span className="text-faint">Accrued since last collection</span>
          <span className="tabular-nums text-ink">≈ {formatShares(feeShares)} shares</span>
        </div>
        <div className="flex justify-between">
          <span className="text-faint">Est. value to manager</span>
          <span className="tabular-nums text-muted">${formatUsdc(feeValue)}</span>
        </div>
      </div>
      <p className="text-[12px] leading-relaxed text-faint">
        Mints time-and-rate-proportional fee shares to the manager, diluting holders by the fee.
      </p>
      <Button disabled={busy} onClick={() => run(() => onCollect(), "Fee collection")}>
        {busy ? "Working…" : "Collect fees"}
      </Button>
      <StatusLine status={status} />
    </Panel>
  );
}

// ---- Manager view ----------------------------------------------------------

export function ManagerView({ vault }: { vault: VaultState }) {
  const view = vault.view!;
  return (
    <div>
      <div className="flex flex-wrap items-baseline justify-between gap-2 border-b border-line px-6 py-4">
        <span className="font-mono text-[12px] text-muted">
          Manager console — operating strategy #{view.index.toString()}
        </span>
        <span className="font-mono text-[11px] text-faint">
          fee {formatBps(view.account!.feeBps)} · fixed at creation, no setter
        </span>
      </div>
      <div className="grid gap-6 p-6 lg:grid-cols-2">
        <RebalancePanel view={view} onRebalance={vault.rebalance} />
        <SetWeightPanel view={view} onSetWeight={vault.setWeight} />
        <AddAssetPanel view={view} onAddAsset={vault.addAsset} />
        <CollectFeesPanel view={view} onCollect={vault.collectFees} />
      </div>
    </div>
  );
}

// ---- Create strategy (shown on the not-found path) -------------------------

export function CreateStrategyForm({ vault }: { vault: VaultState }) {
  const { status, busy, run } = useAction();
  const [registry, setRegistry] = useState("");
  const [usdc, setUsdc] = useState(USDC_MINT ? USDC_MINT.toBase58() : "");
  const [router, setRouter] = useState(ROUTER_PROGRAM_ID.toBase58());
  const [feePct, setFeePct] = useState("1");
  const [slipPct, setSlipPct] = useState("1");

  const registryPk = tryPubkey(registry);
  const usdcPk = tryPubkey(usdc);
  const routerPk = tryPubkey(router);
  const feeBps = parsePercentToBps(feePct);
  const slipBps = parsePercentToBps(slipPct);

  const block = !vault.connected
    ? "Connect a wallet to create the strategy."
    : !registryPk
      ? "Enter the curator’s registry address."
      : !usdcPk
        ? "Enter the USDC mint."
        : !routerPk
          ? "Enter the swap router program id."
          : feeBps === null || feeBps > MAX_FEE_BPS
            ? `Fee must be 0–${MAX_FEE_BPS / 100}%.`
            : slipBps === null || slipBps > MAX_SLIPPAGE_BPS
              ? `Max slippage must be 0–${MAX_SLIPPAGE_BPS / 100}%.`
              : null;
  const ready = !block;

  return (
    <div className="mx-auto mt-6 max-w-xl">
      <Panel title={`Create strategy #${STRATEGY_INDEX.toString()}`} hint="manager setup">
        <p className="text-[12px] leading-relaxed text-muted">
          Bootstraps this strategy at index {STRATEGY_INDEX.toString()}. Requires a registry a curator has already
          created and approved assets in. You become the manager.
        </p>
        <TextField
          label="Registry"
          value={registry}
          onChange={setRegistry}
          placeholder="Base58…"
          invalid={registry.trim() !== "" && !registryPk}
        />
        <TextField
          label="USDC mint"
          value={usdc}
          onChange={setUsdc}
          placeholder="Base58…"
          invalid={usdc.trim() !== "" && !usdcPk}
        />
        <TextField
          label="Swap router program"
          value={router}
          onChange={setRouter}
          placeholder="Base58…"
          invalid={router.trim() !== "" && !routerPk}
        />
        <div className="grid grid-cols-2 gap-3">
          <TextField label="Management fee" value={feePct} onChange={setFeePct} suffix="%" />
          <TextField label="Max slippage" value={slipPct} onChange={setSlipPct} suffix="%" />
        </div>
        {block && vault.connected && <p className="text-[12px] leading-relaxed text-muted">{block}</p>}
        <Button
          disabled={!ready || busy}
          onClick={() =>
            run(
              () =>
                vault.createStrategy({
                  registry: registryPk!,
                  usdcMint: usdcPk!,
                  index: STRATEGY_INDEX,
                  feeBps: feeBps!,
                  maxSlippageBps: slipBps!,
                  swapRouter: routerPk!,
                }),
              "Strategy creation",
            )
          }
        >
          {busy ? "Working…" : "Create strategy"}
        </Button>
        <StatusLine status={status} />
      </Panel>
    </div>
  );
}
