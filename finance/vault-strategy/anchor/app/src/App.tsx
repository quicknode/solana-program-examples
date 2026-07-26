import { useEffect, useMemo, useState } from 'react'
import { useAnchorWallet, useConnection, useWallet } from '@solana/wallet-adapter-react'
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui'
import type { PublicKey } from '@solana/web3.js'
import { getProgram } from './solana/program'
import { loadPosition, loadStrategyView, type Position, type StrategyView } from './solana/strategy'
import {
  CLUSTER,
  RPC_URL,
  ROUTER_PROGRAM_ID,
  STRATEGY_INDEX,
  USDC_MINT,
  VAULT_PROGRAM_ID,
} from './solana/config'
import {
  explorerAddress,
  formatBps,
  formatRatioPct,
  formatShares,
  formatUnits,
  formatUsdc,
  shortAddress,
} from './solana/format'

// ----------------------------------------------------------------------------
// LAYER 1 — client wiring. This screen is a diagnostic readout that proves every
// on-chain read path works; every value shown is a real account read. The composed
// investor and manager views replace it in the next layers.
// ----------------------------------------------------------------------------

interface LoadState {
  loading: boolean
  error: string | null
  view: StrategyView | null
  position: Position | null
}

function useStrategy(): LoadState & { refresh: () => void } {
  const { connection } = useConnection()
  const anchorWallet = useAnchorWallet()
  const { publicKey } = useWallet()
  const [tick, setTick] = useState(0)
  const [state, setState] = useState<LoadState>({
    loading: true,
    error: null,
    view: null,
    position: null,
  })

  useEffect(() => {
    let cancelled = false
    setState((s) => ({ ...s, loading: true, error: null }))
    const program = getProgram(connection, anchorWallet ?? undefined)
    ;(async () => {
      try {
        const view = await loadStrategyView(connection, program)
        const position =
          view.exists && publicKey ? await loadPosition(connection, view, publicKey) : null
        if (!cancelled) setState({ loading: false, error: null, view, position })
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        if (!cancelled) setState({ loading: false, error: message, view: null, position: null })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [connection, anchorWallet, publicKey, tick])

  return { ...state, refresh: () => setTick((t) => t + 1) }
}

// ---- small presentational atoms --------------------------------------------

function Chip({ children }: { children: React.ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 border border-line px-2 py-1 text-[11px] uppercase tracking-widest text-muted">
      {children}
    </span>
  )
}

function Addr({ value }: { value: PublicKey | string }) {
  return (
    <a
      href={explorerAddress(value)}
      target="_blank"
      rel="noreferrer"
      className="font-mono text-[12px] text-muted underline-offset-2 hover:text-accent hover:underline"
    >
      {shortAddress(value)}
    </a>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[10px] uppercase tracking-widest text-faint">{label}</span>
      <span className="font-mono text-[13px] text-ink">{children}</span>
    </div>
  )
}

function priceUsd(price: bigint | null): string {
  if (price === null) return '—'
  // Pyth exponent -8.
  return `$${formatUnits(price, 8, 2)}`
}

// ---- panels ----------------------------------------------------------------

function ConfigBar() {
  const host = useMemo(() => {
    try {
      return new URL(RPC_URL).host
    } catch {
      return RPC_URL
    }
  }, [])
  return (
    <div className="grid grid-cols-2 gap-x-8 gap-y-4 border-b border-line px-6 py-4 sm:grid-cols-3 lg:grid-cols-5">
      <Field label="RPC">
        <span className="text-muted">{host}</span>
      </Field>
      <Field label="Vault program">
        <Addr value={VAULT_PROGRAM_ID} />
      </Field>
      <Field label="Router program">
        <Addr value={ROUTER_PROGRAM_ID} />
      </Field>
      <Field label="USDC mint">
        {USDC_MINT ? <Addr value={USDC_MINT} /> : <span className="text-loss">unset</span>}
      </Field>
      <Field label="Strategy index">#{STRATEGY_INDEX.toString()}</Field>
    </div>
  )
}

function HeroStat({
  label,
  value,
  sub,
  caveat,
}: {
  label: string
  value: string
  sub?: string
  caveat?: string
}) {
  return (
    <div className="flex flex-col gap-2 px-6 py-6">
      <span className="text-[11px] uppercase tracking-widest text-faint">{label}</span>
      <span className="font-mono text-stat leading-none text-ink">{value}</span>
      {sub && <span className="font-mono text-[12px] text-muted">{sub}</span>}
      {caveat && <span className="text-[11px] uppercase tracking-widest text-accent">{caveat}</span>}
    </div>
  )
}

function StrategyPanel({ view, position }: { view: StrategyView; position: Position | null }) {
  const s = view.account!
  return (
    <div>
      <div className="grid grid-cols-1 divide-y divide-line border-b border-line sm:grid-cols-3 sm:divide-x sm:divide-y-0">
        <HeroStat
          label="AUM"
          value={`$${formatUsdc(view.navMinor)}`}
          sub={`${view.assets.length} asset${view.assets.length === 1 ? '' : 's'} + USDC`}
          caveat={view.navComplete ? undefined : 'partial — unpriced holdings'}
        />
        <HeroStat
          label="NAV / share"
          value={formatUnits(view.navPerShareMinor, 6, 4)}
          sub="USDC per share"
        />
        <HeroStat
          label={position ? 'My position' : 'Total shares'}
          value={
            position ? `$${formatUsdc(position.valueMinor)}` : formatShares(view.totalShares)
          }
          sub={
            position
              ? `${formatShares(position.shares)} shares · ${formatRatioPct(position.ownership)}`
              : 'shares outstanding'
          }
        />
      </div>

      <div className="grid grid-cols-2 gap-x-8 gap-y-5 border-b border-line px-6 py-5 sm:grid-cols-3 lg:grid-cols-4">
        <Field label="Manager">
          <Addr value={s.manager} />
        </Field>
        <Field label="Registry">
          <Addr value={s.registry} />
        </Field>
        <Field label="Share mint">
          <Addr value={view.shareMint} />
        </Field>
        <Field label="USDC mint">
          <Addr value={s.usdcMint} />
        </Field>
        <Field label="Management fee">{formatBps(s.feeBps)} / yr</Field>
        <Field label="Max slippage">{formatBps(s.maxSlippageBps)}</Field>
        <Field label="Allocation">
          {formatBps(s.totalWeightBps, 0)}{' '}
          <span className={view.fullyAllocated ? 'text-gain' : 'text-accent'}>
            {view.fullyAllocated ? 'live' : 'configuring'}
          </span>
        </Field>
        <Field label="Total shares">{formatShares(view.totalShares)}</Field>
      </div>

      <div className="px-6 py-5">
        <div className="mb-3 text-[10px] uppercase tracking-widest text-faint">Allocation</div>
        {view.assets.length === 0 ? (
          <div className="font-mono text-[13px] text-muted">No assets added yet.</div>
        ) : (
          <table className="w-full border-collapse font-mono text-[13px]">
            <thead>
              <tr className="text-[10px] uppercase tracking-widest text-faint">
                <th className="py-2 text-left font-normal">#</th>
                <th className="py-2 text-left font-normal">Mint</th>
                <th className="py-2 text-right font-normal">Target</th>
                <th className="py-2 text-right font-normal">Actual</th>
                <th className="py-2 text-right font-normal">Vault balance</th>
                <th className="py-2 text-right font-normal">Price</th>
                <th className="py-2 text-right font-normal">Value</th>
              </tr>
            </thead>
            <tbody>
              {view.assets.map((a) => (
                <tr key={a.index} className="border-t border-line">
                  <td className="py-2.5 text-left text-faint">{a.index}</td>
                  <td className="py-2.5 text-left">
                    <Addr value={a.mint} />
                  </td>
                  <td className="py-2.5 text-right text-muted">{formatBps(a.weightBps, 0)}</td>
                  <td className="py-2.5 text-right text-ink">
                    {a.actualWeight === null ? '—' : formatRatioPct(a.actualWeight)}
                  </td>
                  <td className="py-2.5 text-right text-ink">{formatUnits(a.vaultAmount, 6, 6)}</td>
                  <td className="py-2.5 text-right">
                    <span className="text-ink">{priceUsd(a.price)}</span>
                    {a.stale && (
                      <span className="ml-2 text-[10px] uppercase tracking-widest text-loss">
                        stale
                      </span>
                    )}
                  </td>
                  <td className="py-2.5 text-right text-ink">
                    {a.valueUsdc === null ? '—' : `$${formatUsdc(a.valueUsdc)}`}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}

function EmptyState({ view }: { view: StrategyView }) {
  return (
    <div className="px-6 py-16">
      <div className="mx-auto max-w-lg border border-line bg-panel px-6 py-8">
        <div className="mb-2 text-[11px] uppercase tracking-widest text-accent">No strategy</div>
        <h2 className="mb-3 font-sans text-xl font-semibold text-ink">
          Nothing at strategy #{view.index.toString()} on this cluster
        </h2>
        <p className="mb-5 text-[13px] leading-relaxed text-muted">
          The program isn't deployed here, or no strategy has been created at this index yet. Deploy
          the vault-strategy program to devnet and seed it, then point <code>VITE_*</code> at the
          new ids. See <code>app/README.md</code>.
        </p>
        <div className="space-y-3">
          <Field label="Expected strategy PDA">
            <Addr value={view.strategy} />
          </Field>
          <Field label="Vault program">
            <Addr value={VAULT_PROGRAM_ID} />
          </Field>
        </div>
      </div>
    </div>
  )
}

export default function App() {
  const { loading, error, view, position, refresh } = useStrategy()

  return (
    <div className="min-h-full">
      <header className="flex items-center justify-between border-b border-line px-6 py-4">
        <div className="flex items-baseline gap-3">
          <span className="inline-block h-3.5 w-3.5 translate-y-[1px] bg-accent" aria-hidden />
          <span className="font-sans text-[15px] font-bold tracking-tight text-ink">
            VAULT STRATEGY
          </span>
          <span className="font-mono text-[13px] text-faint">/ #{STRATEGY_INDEX.toString()}</span>
        </div>
        <div className="flex items-center gap-3">
          <Chip>
            <span className="h-1.5 w-1.5 rounded-full bg-gain" aria-hidden />
            {CLUSTER}
          </Chip>
          <button
            onClick={refresh}
            className="border border-line px-3 py-[7px] text-[11px] uppercase tracking-widest text-muted transition-colors hover:border-line2 hover:text-ink"
          >
            Refresh
          </button>
          <WalletMultiButton />
        </div>
      </header>

      <ConfigBar />

      <main>
        {loading && (
          <div className="px-6 py-16 text-center font-mono text-[13px] text-muted">
            reading chain…
          </div>
        )}
        {!loading && error && (
          <div className="px-6 py-16">
            <div className="mx-auto max-w-lg border border-loss/40 bg-panel px-6 py-6">
              <div className="mb-2 text-[11px] uppercase tracking-widest text-loss">RPC error</div>
              <p className="break-words font-mono text-[13px] text-muted">{error}</p>
              <p className="mt-4 text-[12px] text-faint">
                Set <code>VITE_RPC_URL</code> to your Quicknode devnet endpoint in{' '}
                <code>.env.local</code>.
              </p>
            </div>
          </div>
        )}
        {!loading && !error && view && (view.exists ? (
          <StrategyPanel view={view} position={position} />
        ) : (
          <EmptyState view={view} />
        ))}
      </main>

      <footer className="border-t border-line px-6 py-4 text-[11px] text-faint">
        Layer 1 · client wiring. Every value above is a live account read. Investor and manager
        views follow.
      </footer>
    </div>
  )
}
