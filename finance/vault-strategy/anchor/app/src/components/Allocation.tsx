import type { StrategyView } from '../solana/strategy'
import { formatBps, formatRatioPct, formatUnits, formatUsdc } from '../solana/format'
import { Addr } from './atoms'

// A curated, desaturated "instrument" palette — matte, not neon — starting from the
// accent amber. Real weight encoding, not decoration: the table swatches reuse it.
const PALETTE = ['#F0B429', '#5E8CA6', '#7FA06B', '#C77F4E', '#9C7BA6', '#4FA39A', '#B5606B']
const assetColor = (i: number): string => PALETTE[i % PALETTE.length]
const IDLE_COLOR = 'rgba(233, 230, 225, 0.16)'
const UNALLOC_COLOR = 'rgba(233, 230, 225, 0.06)'

interface Segment {
  key: string
  label: string
  pct: number
  color: string
}

function buildSegments(view: StrategyView): Segment[] {
  const nav = Number(view.navMinor)
  const useActual = view.navComplete && view.navMinor > 0n
  const segments: Segment[] = []

  view.assets.forEach((a, i) => {
    const pct = useActual
      ? a.valueUsdc !== null
        ? Number(a.valueUsdc) / nav
        : 0
      : a.weightBps / 10_000
    if (pct > 0) {
      segments.push({ key: `a${a.index}`, label: `#${a.index}`, pct, color: assetColor(i) })
    }
  })

  if (useActual) {
    const idle = Number(view.usdcAmount) / nav
    if (idle > 0.0005) segments.push({ key: 'usdc', label: 'USDC', pct: idle, color: IDLE_COLOR })
  } else {
    const allocated = (view.account?.totalWeightBps ?? 0) / 10_000
    if (allocated < 1) {
      segments.push({ key: 'unalloc', label: 'Unallocated', pct: 1 - allocated, color: UNALLOC_COLOR })
    }
  }
  return segments
}

function priceUsd(price: bigint | null): string {
  return price === null ? '—' : `$${formatUnits(price, 8, 2)}`
}

export function AllocationPanel({ view }: { view: StrategyView }) {
  const s = view.account!
  const segments = buildSegments(view)
  const basisLabel = view.navComplete && view.navMinor > 0n ? 'live weights' : 'target weights'

  return (
    <section className="px-6 py-6">
      <div className="mb-4 flex flex-col gap-1 sm:flex-row sm:items-baseline sm:justify-between sm:gap-4">
        <h2 className="font-sans text-[13px] font-semibold uppercase tracking-widest text-muted">
          Allocation
        </h2>
        <span className="font-mono text-[12px] text-faint">
          fee {formatBps(s.feeBps)} / yr · slippage {formatBps(s.maxSlippageBps)}
        </span>
      </div>

      {segments.length > 0 && (
        <div className="mb-6 flex h-2.5 w-full gap-px overflow-hidden bg-graphite" role="img" aria-label={`allocation by ${basisLabel}`}>
          {segments.map((seg) => (
            <div key={seg.key} style={{ width: `${seg.pct * 100}%`, backgroundColor: seg.color }} title={`${seg.label} · ${formatRatioPct(seg.pct)}`} />
          ))}
        </div>
      )}

      {view.assets.length === 0 ? (
        <p className="font-mono text-[13px] text-muted">No assets added yet.</p>
      ) : (
        <div className="-mx-6 overflow-x-auto px-6">
        <table className="w-full min-w-[560px] border-collapse font-mono text-[13px]">
          <thead>
            <tr className="text-[10px] uppercase tracking-widest text-faint">
              <th className="w-6 py-2 text-left font-normal" />
              <th className="py-2 text-left font-normal">Asset</th>
              <th className="py-2 text-right font-normal">Target</th>
              <th className="py-2 text-right font-normal">Live</th>
              <th className="py-2 text-right font-normal">Vault balance</th>
              <th className="py-2 text-right font-normal">Price</th>
              <th className="py-2 text-right font-normal">Value</th>
            </tr>
          </thead>
          <tbody>
            {view.assets.map((a, i) => (
              <tr key={a.index} className="border-t border-line">
                <td className="py-2.5">
                  <span className="inline-block h-2.5 w-2.5" style={{ backgroundColor: assetColor(i) }} />
                </td>
                <td className="py-2.5 text-left">
                  <span className="text-faint">#{a.index}</span> <Addr value={a.mint} />
                  {a.weightBps === 0 && (
                    <span className="ml-2 text-[10px] uppercase tracking-widest text-faint">retired</span>
                  )}
                </td>
                <td className="py-2.5 text-right text-muted">{formatBps(a.weightBps, 0)}</td>
                <td className="py-2.5 text-right text-ink">
                  {a.actualWeight === null ? '—' : formatRatioPct(a.actualWeight)}
                </td>
                <td className="py-2.5 text-right text-ink">{formatUnits(a.vaultAmount, 6, 6)}</td>
                <td className="py-2.5 text-right">
                  <span className="text-ink">{priceUsd(a.price)}</span>
                  {a.stale && (
                    <span className="ml-2 text-[10px] uppercase tracking-widest text-loss">stale</span>
                  )}
                </td>
                <td className="py-2.5 text-right text-ink">
                  {a.valueUsdc === null ? '—' : `$${formatUsdc(a.valueUsdc)}`}
                </td>
              </tr>
            ))}
            {view.usdcAmount > 0n && (
              <tr className="border-t border-line text-muted">
                <td className="py-2.5">
                  <span className="inline-block h-2.5 w-2.5" style={{ backgroundColor: IDLE_COLOR }} />
                </td>
                <td className="py-2.5 text-left">USDC (idle)</td>
                <td className="py-2.5 text-right">—</td>
                <td className="py-2.5 text-right">
                  {view.navMinor > 0n ? formatRatioPct(Number(view.usdcAmount) / Number(view.navMinor)) : '—'}
                </td>
                <td className="py-2.5 text-right">{formatUnits(view.usdcAmount, 6, 6)}</td>
                <td className="py-2.5 text-right">$1.00</td>
                <td className="py-2.5 text-right text-ink">${formatUsdc(view.usdcAmount)}</td>
              </tr>
            )}
          </tbody>
        </table>
        </div>
      )}
    </section>
  )
}
