import { WalletMultiButton } from '@solana/wallet-adapter-react-ui'
import { CLUSTER, STRATEGY_INDEX } from '../solana/config'
import { Chip } from './atoms'

export function Header({ onRefresh }: { onRefresh: () => void }) {
  return (
    <header className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-6 py-4">
      <div className="flex items-baseline gap-3">
        <span className="inline-block h-3.5 w-3.5 translate-y-[1px] bg-accent" aria-hidden />
        <span className="font-sans text-[15px] font-bold tracking-tight text-ink">VAULT STRATEGY</span>
        <span className="font-mono text-[13px] text-faint">/ #{STRATEGY_INDEX.toString()}</span>
      </div>
      <div className="flex items-center gap-3">
        <Chip>
          <span className="h-1.5 w-1.5 rounded-full bg-gain" aria-hidden />
          {CLUSTER}
        </Chip>
        <button
          onClick={onRefresh}
          className="h-[34px] border border-line px-3 text-[11px] uppercase tracking-widest text-muted transition-colors hover:border-line2 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        >
          Refresh
        </button>
        <WalletMultiButton />
      </div>
    </header>
  )
}
