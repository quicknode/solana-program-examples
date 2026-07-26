import { useState } from 'react'
import type { Position, StrategyView } from '../solana/strategy'
import { formatShares, formatUnits, formatUsdc, shortAddress } from '../solana/format'
import {
  applyToleranceFloor,
  estimateRedeem,
  estimateSharesOut,
  parseAmount,
} from '../lib/amounts'
import { describeError } from '../lib/tx'
import { Button, Segmented, StatusLine, TextField, type TxStatus } from './atoms'

const DEPOSIT_TOLERANCE_BPS = 100 // 1% — floor on shares out to protect the depositor

type Mode = 'deposit' | 'redeem'

/** Ungrouped decimal string for filling the input (no thousands commas). */
function rawAmount(minor: bigint, decimals = 6): string {
  const base = 10n ** BigInt(decimals)
  const whole = (minor / base).toString()
  const frac = (minor % base).toString().padStart(decimals, '0').replace(/0+$/, '')
  return frac ? `${whole}.${frac}` : whole
}

export interface ActionTicketProps {
  view: StrategyView
  connected: boolean
  walletUsdc: bigint | null
  position: Position | null
  onDeposit: (usdcMinor: bigint, minShares: bigint) => Promise<string>
  onRedeem: (sharesMinor: bigint, minUsdcOut: bigint) => Promise<string>
}

export function ActionTicket({
  view,
  connected,
  walletUsdc,
  position,
  onDeposit,
  onRedeem,
}: ActionTicketProps) {
  const [mode, setMode] = useState<Mode>('deposit')
  const [depositInput, setDepositInput] = useState('')
  const [redeemInput, setRedeemInput] = useState('')
  const [status, setStatus] = useState<TxStatus>({ kind: 'idle' })
  const [busy, setBusy] = useState(false)

  const shares = position?.shares ?? 0n
  const activeUnpriced = view.assets.some((a) => a.weightBps > 0 && (a.price === null || a.stale))

  // deposit derivations
  const usdcMinor = parseAmount(depositInput, 6)
  const depositInvalid = depositInput.trim() !== '' && usdcMinor === null
  const expectedShares =
    usdcMinor !== null && usdcMinor > 0n
      ? estimateSharesOut(usdcMinor, view.navMinor, view.totalShares)
      : null
  const minShares = expectedShares !== null ? applyToleranceFloor(expectedShares, DEPOSIT_TOLERANCE_BPS) : 0n

  const depositBlock: string | null = !connected
    ? 'Connect a wallet to deposit.'
    : !view.fullyAllocated
      ? 'Strategy isn’t fully allocated — deposits are closed until target weights total 100%.'
      : activeUnpriced
        ? 'Oracle prices are stale or missing — a deposit would revert on-chain.'
        : usdcMinor !== null && usdcMinor > 0n && walletUsdc !== null && usdcMinor > walletUsdc
          ? 'Amount exceeds your USDC balance.'
          : null
  const depositReady =
    connected &&
    view.fullyAllocated &&
    !activeUnpriced &&
    usdcMinor !== null &&
    usdcMinor > 0n &&
    (walletUsdc === null || usdcMinor <= walletUsdc)

  // redeem derivations
  const sharesMinor = parseAmount(redeemInput, 6)
  const redeemInvalid = redeemInput.trim() !== '' && sharesMinor === null
  const redeemEst =
    sharesMinor !== null && sharesMinor > 0n ? estimateRedeem(sharesMinor, view) : null

  const redeemBlock: string | null = !connected
    ? 'Connect a wallet to redeem.'
    : shares === 0n
      ? 'You hold no shares in this strategy.'
      : sharesMinor !== null && sharesMinor > 0n && sharesMinor > shares
        ? 'Amount exceeds your shares.'
        : null
  const redeemReady =
    connected && shares > 0n && sharesMinor !== null && sharesMinor > 0n && sharesMinor <= shares

  async function run(action: () => Promise<string>, verb: string) {
    setBusy(true)
    setStatus({ kind: 'pending', message: 'Confirm in your wallet…' })
    try {
      const signature = await action()
      setStatus({ kind: 'success', message: `${verb} confirmed.`, signature })
      setDepositInput('')
      setRedeemInput('')
    } catch (err) {
      setStatus({ kind: 'error', message: describeError(err) })
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="border border-line bg-panel">
      <Segmented<Mode>
        options={[
          { key: 'deposit', label: 'Deposit' },
          { key: 'redeem', label: 'Redeem' },
        ]}
        value={mode}
        onChange={(k) => {
          setMode(k)
          setStatus({ kind: 'idle' })
        }}
      />

      <div className="space-y-4 px-5 py-5">
        {mode === 'deposit' ? (
          <>
            <TextField
              label="Deposit"
              value={depositInput}
              onChange={setDepositInput}
              suffix="USDC"
              invalid={depositInvalid}
              placeholder="0.00"
              right={
                walletUsdc !== null ? (
                  <button
                    type="button"
                    onClick={() => setDepositInput(rawAmount(walletUsdc))}
                    className="tabular-nums transition-colors hover:text-accent"
                  >
                    Balance {formatUsdc(walletUsdc)} · Max
                  </button>
                ) : (
                  'Balance —'
                )
              }
            />

            {expectedShares !== null && (
              <div className="space-y-1 font-mono text-[12px]">
                <div className="flex justify-between">
                  <span className="text-faint">You receive</span>
                  <span className="tabular-nums text-ink">≈ {formatShares(expectedShares)} shares</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-faint">Minimum (1% tol.)</span>
                  <span className="tabular-nums text-muted">{formatShares(minShares)} shares</span>
                </div>
              </div>
            )}

            {depositBlock && <p className="text-[12px] leading-relaxed text-muted">{depositBlock}</p>}

            <Button onClick={() => run(() => onDeposit(usdcMinor!, minShares), 'Deposit')} disabled={!depositReady || busy}>
              {busy ? 'Working…' : 'Deposit USDC'}
            </Button>
          </>
        ) : (
          <>
            <TextField
              label="Redeem"
              value={redeemInput}
              onChange={setRedeemInput}
              suffix="SHARES"
              invalid={redeemInvalid}
              placeholder="0.00"
              right={
                <button
                  type="button"
                  onClick={() => setRedeemInput(rawAmount(shares))}
                  className="tabular-nums transition-colors hover:text-accent"
                >
                  Balance {formatShares(shares)} · Max
                </button>
              }
            />

            {redeemEst && (
              <div className="space-y-1.5 font-mono text-[12px]">
                <div className="text-faint">Paid out in kind — each asset, not USDC:</div>
                {redeemEst.legs.map((leg) => (
                  <div key={leg.index} className="flex justify-between">
                    <span className="text-muted">
                      #{leg.index} · {shortAddress(leg.mint)}
                    </span>
                    <span className="tabular-nums text-ink">{formatUnits(leg.amountMinor, 6, 6)}</span>
                  </div>
                ))}
                <div className="flex justify-between border-t border-line pt-1.5">
                  <span className="text-muted">USDC</span>
                  <span className="tabular-nums text-ink">{formatUsdc(redeemEst.usdcMinor)}</span>
                </div>
              </div>
            )}

            {redeemBlock && <p className="text-[12px] leading-relaxed text-muted">{redeemBlock}</p>}

            <Button onClick={() => run(() => onRedeem(sharesMinor!, 0n), 'Redemption')} disabled={!redeemReady || busy}>
              {busy ? 'Working…' : 'Redeem shares'}
            </Button>
          </>
        )}

        <StatusLine status={status} />
      </div>
    </div>
  )
}
