import { useCallback, useEffect, useMemo, useState } from 'react'
import { useAnchorWallet, useConnection, useWallet } from '@solana/wallet-adapter-react'
import type { PublicKey } from '@solana/web3.js'
import { getProgram } from '../solana/program'
import { loadPosition, loadStrategyView, type Position, type StrategyView } from '../solana/strategy'
import { readTokenAmount } from '../solana/pyth'
import { userAta } from '../solana/pdas'
import {
  buildAddAssetIx,
  buildCollectFeesIx,
  buildDepositIx,
  buildInitializeStrategyIx,
  buildRebalanceIx,
  buildSetWeightIx,
  buildWithdrawIxs,
  type InitializeStrategyParams,
} from '../solana/instructions'
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
  isManager: boolean
  refresh: () => void
  // depositor
  deposit: (usdcMinor: bigint, minShares: bigint) => Promise<string>
  redeem: (sharesMinor: bigint, minUsdcOut: bigint) => Promise<string>
  // manager
  rebalance: (sellIndex: number, buyIndex: number, sellAmount: bigint, usdcToInvest: bigint) => Promise<string>
  setWeight: (assetIndex: number, weightBps: number) => Promise<string>
  addAsset: (mint: PublicKey, weightBps: number) => Promise<string>
  collectFees: () => Promise<string>
  createStrategy: (params: Omit<InitializeStrategyParams, 'manager'>) => Promise<string>
}

/** Loads the strategy, the connected wallet's position + USDC balance, and exposes both
 *  depositor and manager actions. Every field is a live read; actions rebuild against a
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

  // All senders rebuild against a freshly-loaded view so account derivations (asset
  // count, mints, router) reflect the latest chain state, then refresh the UI.
  const withFreshView = useCallback(
    async (send: (view: StrategyView, manager: PublicKey) => Promise<string>) => {
      if (!anchorWallet) throw new Error('Connect a wallet first.')
      const view = await loadStrategyView(connection, program)
      if (!view.exists) throw new Error('Strategy not found on this cluster.')
      const sig = await send(view, anchorWallet.publicKey)
      refresh()
      return sig
    },
    [anchorWallet, connection, program, refresh],
  )

  const deposit = useCallback(
    (usdcMinor: bigint, minShares: bigint) =>
      withFreshView(async (view, wallet) =>
        sendIxs(program, [await buildDepositIx(program, view, wallet, usdcMinor, minShares)]),
      ),
    [program, withFreshView],
  )

  const redeem = useCallback(
    (sharesMinor: bigint, minUsdcOut: bigint) =>
      withFreshView(async (view, wallet) =>
        sendIxs(program, await buildWithdrawIxs(program, view, wallet, sharesMinor, minUsdcOut)),
      ),
    [program, withFreshView],
  )

  const rebalance = useCallback(
    (sellIndex: number, buyIndex: number, sellAmount: bigint, usdcToInvest: bigint) =>
      withFreshView(async (view, wallet) =>
        sendIxs(program, [
          await buildRebalanceIx(program, view, wallet, sellIndex, buyIndex, sellAmount, usdcToInvest),
        ]),
      ),
    [program, withFreshView],
  )

  const setWeight = useCallback(
    (assetIndex: number, weightBps: number) =>
      withFreshView(async (view, wallet) =>
        sendIxs(program, [await buildSetWeightIx(program, view, wallet, assetIndex, weightBps)], 0),
      ),
    [program, withFreshView],
  )

  const addAsset = useCallback(
    (mint: PublicKey, weightBps: number) =>
      withFreshView(async (view, wallet) =>
        sendIxs(program, [await buildAddAssetIx(program, view, wallet, mint, weightBps)], 0),
      ),
    [program, withFreshView],
  )

  const collectFees = useCallback(
    () =>
      withFreshView(async (view, wallet) =>
        sendIxs(program, [await buildCollectFeesIx(program, view, wallet)], 0),
      ),
    [program, withFreshView],
  )

  const createStrategy = useCallback(
    async (params: Omit<InitializeStrategyParams, 'manager'>) => {
      if (!anchorWallet) throw new Error('Connect a wallet first.')
      const ix = await buildInitializeStrategyIx(program, { ...params, manager: anchorWallet.publicKey })
      const sig = await sendIxs(program, [ix], 0)
      refresh()
      return sig
    },
    [anchorWallet, program, refresh],
  )

  const isManager = !!(
    publicKey &&
    data.view?.exists &&
    data.view.account &&
    data.view.account.manager.equals(publicKey)
  )

  return {
    ...data,
    connected: !!publicKey,
    isManager,
    refresh,
    deposit,
    redeem,
    rebalance,
    setWeight,
    addAsset,
    collectFees,
    createStrategy,
  }
}
