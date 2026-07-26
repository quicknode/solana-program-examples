import { useCallback, useEffect, useMemo, useState } from 'react'
import { useAnchorWallet, useConnection, useWallet } from '@solana/wallet-adapter-react'
import { getProgram } from '../solana/program'
import { loadPosition, loadStrategyView, type Position, type StrategyView } from '../solana/strategy'
import { readTokenAmount } from '../solana/pyth'
import { userAta } from '../solana/pdas'
import { buildDepositIx, buildWithdrawIxs } from '../solana/instructions'
import { sendIxs } from '../lib/tx'

interface Data {
  loading: boolean
  error: string | null
  view: StrategyView | null
  position: Position | null
  walletUsdc: bigint | null
}

export interface VaultState extends Data {
  connected: boolean
  refresh: () => void
  deposit: (usdcMinor: bigint, minShares: bigint) => Promise<string>
  redeem: (sharesMinor: bigint, minUsdcOut: bigint) => Promise<string>
}

/** Loads the strategy, the connected wallet's position + USDC balance, and exposes the
 *  deposit/redeem actions. Every field is a live account read; actions rebuild against a
 *  fresh view before sending, then refresh. */
export function useVault(): VaultState {
  const { connection } = useConnection()
  const anchorWallet = useAnchorWallet()
  const { publicKey } = useWallet()
  const program = useMemo(
    () => getProgram(connection, anchorWallet ?? undefined),
    [connection, anchorWallet],
  )
  const [tick, setTick] = useState(0)
  const [data, setData] = useState<Data>({
    loading: true,
    error: null,
    view: null,
    position: null,
    walletUsdc: null,
  })

  useEffect(() => {
    let cancelled = false
    setData((d) => ({ ...d, loading: true, error: null }))
    ;(async () => {
      try {
        const view = await loadStrategyView(connection, program)
        let position: Position | null = null
        let walletUsdc: bigint | null = null
        if (view.exists && publicKey) {
          position = await loadPosition(connection, view, publicKey)
          const info = await connection.getAccountInfo(userAta(view.account!.usdcMint, publicKey))
          walletUsdc = info ? readTokenAmount(info.data) : 0n
        }
        if (!cancelled) setData({ loading: false, error: null, view, position, walletUsdc })
      } catch (err) {
        if (!cancelled) {
          setData({
            loading: false,
            error: err instanceof Error ? err.message : String(err),
            view: null,
            position: null,
            walletUsdc: null,
          })
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [connection, program, publicKey, tick])

  const refresh = useCallback(() => setTick((t) => t + 1), [])

  const deposit = useCallback(
    async (usdcMinor: bigint, minShares: bigint) => {
      if (!anchorWallet) throw new Error('Connect a wallet to deposit.')
      const view = await loadStrategyView(connection, program) // build against fresh state
      if (!view.exists) throw new Error('Strategy not found on this cluster.')
      const ix = await buildDepositIx(program, view, anchorWallet.publicKey, usdcMinor, minShares)
      const sig = await sendIxs(program, [ix])
      refresh()
      return sig
    },
    [anchorWallet, connection, program, refresh],
  )

  const redeem = useCallback(
    async (sharesMinor: bigint, minUsdcOut: bigint) => {
      if (!anchorWallet) throw new Error('Connect a wallet to redeem.')
      const view = await loadStrategyView(connection, program)
      if (!view.exists) throw new Error('Strategy not found on this cluster.')
      const ixs = await buildWithdrawIxs(program, view, anchorWallet.publicKey, sharesMinor, minUsdcOut)
      const sig = await sendIxs(program, ixs)
      refresh()
      return sig
    },
    [anchorWallet, connection, program, refresh],
  )

  return { ...data, connected: !!publicKey, refresh, deposit, redeem }
}
