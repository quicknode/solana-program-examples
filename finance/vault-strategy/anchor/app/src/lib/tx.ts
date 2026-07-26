import { AnchorError, AnchorProvider } from '@coral-xyz/anchor'
import {
  ComputeBudgetProgram,
  Transaction,
  type TransactionInstruction,
} from '@solana/web3.js'
import type { VaultProgram } from '../solana/program'

/**
 * Build a legacy transaction from instructions (with a compute-unit bump — deposit and
 * rebalance run swaps + a mint and exceed the 200k default), sign with the provider's
 * wallet, send, and confirm. Returns the signature.
 *
 * Note: baskets beyond ~3 assets exceed the legacy 1232-byte limit and need a v0
 * transaction with an Address Lookup Table; that's out of scope for the demo basket.
 */
export async function sendIxs(
  program: VaultProgram,
  ixs: TransactionInstruction[],
  computeUnits = 400_000,
): Promise<string> {
  const provider = program.provider as AnchorProvider
  const tx = new Transaction()
  if (computeUnits > 0) {
    tx.add(ComputeBudgetProgram.setComputeUnitLimit({ units: computeUnits }))
  }
  for (const ix of ixs) tx.add(ix)
  return provider.sendAndConfirm(tx)
}

/** Turn a thrown transaction error into a human message: problem, and where possible recovery. */
export function describeError(err: unknown): string {
  const e = err as {
    error?: { errorMessage?: string }
    logs?: string[]
    message?: string
  }
  if (e?.error?.errorMessage) return e.error.errorMessage
  if (Array.isArray(e?.logs)) {
    const parsed = AnchorError.parse(e.logs)
    if (parsed) return parsed.error.errorMessage
    const line = e.logs.find((l) => l.includes('Error Message:'))
    if (line) return line.split('Error Message:')[1].trim()
  }
  if (typeof e?.message === 'string') {
    if (/user rejected|rejected the request/i.test(e.message)) return 'Transaction rejected in wallet.'
    return e.message.replace(/^failed to send transaction:\s*/i, '')
  }
  return String(err)
}
