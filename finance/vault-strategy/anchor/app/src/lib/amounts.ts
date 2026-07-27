import type { StrategyView } from '../solana/strategy'

/** Parse a decimal string into minor units. Returns null if malformed or over-precise. */
export function parseAmount(input: string, decimals: number): bigint | null {
  const t = input.trim()
  if (t === '' || t === '.') return null
  if (!/^\d*\.?\d*$/.test(t)) return null
  const [whole, frac = ''] = t.split('.')
  if (frac.length > decimals) return null
  const w = whole === '' ? 0n : BigInt(whole)
  const f = frac === '' ? 0n : BigInt(frac.padEnd(decimals, '0'))
  return w * 10n ** BigInt(decimals) + f
}

/** Shares a deposit would mint, matching the program: first deposit 1:1, else usdc*shares/nav. */
export function estimateSharesOut(
  usdcMinor: bigint,
  navMinor: bigint,
  totalShares: bigint,
): bigint {
  if (totalShares === 0n) return usdcMinor
  if (navMinor === 0n) return 0n
  return (usdcMinor * totalShares) / navMinor
}

export interface RedeemLeg {
  index: number
  mint: string
  amountMinor: bigint // 6dp
}

export interface RedeemEstimate {
  usdcMinor: bigint
  legs: RedeemLeg[]
}

/** The in-kind slice a redemption pays out, matching withdraw's proportional math. */
export function estimateRedeem(sharesMinor: bigint, view: StrategyView): RedeemEstimate {
  if (view.totalShares === 0n || sharesMinor <= 0n) {
    return { usdcMinor: 0n, legs: view.assets.map((a) => ({ index: a.index, mint: a.mint.toBase58(), amountMinor: 0n })) }
  }
  const usdcMinor = (view.usdcAmount * sharesMinor) / view.totalShares
  const legs = view.assets.map((a) => ({
    index: a.index,
    mint: a.mint.toBase58(),
    amountMinor: (a.vaultAmount * sharesMinor) / view.totalShares,
  }))
  return { usdcMinor, legs }
}

/** Apply a bps slippage tolerance to a floor (e.g. minimum shares out). */
export function applyToleranceFloor(amount: bigint, toleranceBps: number): bigint {
  return (amount * BigInt(10_000 - toleranceBps)) / 10_000n
}

/** Ungrouped decimal string for filling an input (no thousands separators). */
export function toAmountInput(minor: bigint, decimals = 6): string {
  const base = 10n ** BigInt(decimals)
  const whole = (minor / base).toString()
  const frac = (minor % base).toString().padStart(decimals, '0').replace(/0+$/, '')
  return frac ? `${whole}.${frac}` : whole
}

/** Parse a percent string (e.g. "40" or "12.5") into basis points, or null if invalid. */
export function parsePercentToBps(input: string): number | null {
  const t = input.trim()
  if (t === '' || !/^\d*\.?\d*$/.test(t)) return null
  const pct = Number(t)
  if (!Number.isFinite(pct)) return null
  return Math.round(pct * 100)
}
