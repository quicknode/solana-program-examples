import type { StrategyView } from '../solana/strategy'
import { VAULT_PROGRAM_ID } from '../solana/config'
import { Addr, Field } from './atoms'

export function Loading() {
  return (
    <div className="px-6 py-24 text-center font-mono text-[13px] text-muted">
      <span className="mr-2 inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-accent align-middle" />
      reading chain…
    </div>
  )
}

export function ErrorPanel({ message }: { message: string }) {
  return (
    <div className="px-6 py-20">
      <div className="mx-auto max-w-xl border border-loss/40 bg-panel px-6 py-6">
        <div className="mb-2 text-[11px] uppercase tracking-widest text-loss">RPC error</div>
        <p className="break-words font-mono text-[13px] text-muted">{message}</p>
        <p className="mt-4 text-[12px] leading-relaxed text-faint">
          Point <code className="text-muted">VITE_RPC_URL</code> at your Quicknode devnet endpoint
          in <code className="text-muted">.env.local</code>, then refresh.
        </p>
      </div>
    </div>
  )
}

export function NotFound({ view }: { view: StrategyView }) {
  return (
    <div className="px-6 py-20">
      <div className="mx-auto max-w-xl border border-line bg-panel px-6 py-8">
        <div className="mb-2 text-[11px] uppercase tracking-widest text-accent">No strategy</div>
        <h2 className="mb-3 font-sans text-2xl font-bold tracking-tight text-ink">
          Nothing at strategy #{view.index.toString()} on this cluster
        </h2>
        <p className="mb-6 max-w-prose text-[14px] leading-relaxed text-muted">
          The program isn't deployed here, or no strategy exists at this index yet. Deploy the
          vault-strategy program to devnet and seed it, then set the <code className="text-ink">VITE_*</code>{' '}
          variables to the new ids. See <code className="text-ink">app/README.md</code>.
        </p>
        <div className="grid grid-cols-1 gap-4 border-t border-line pt-5 sm:grid-cols-2">
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
