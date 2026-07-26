import type { ReactNode } from 'react'
import type { VaultState } from '../hooks/useVault'
import type { StrategyView } from '../solana/strategy'
import { VAULT_PROGRAM_ID } from '../solana/config'
import { formatRatioPct, formatShares, formatUnits, formatUsdc } from '../solana/format'
import { AllocationPanel } from '../components/Allocation'
import { ActionTicket } from '../components/ActionTicket'
import { Addr, Field } from '../components/atoms'

function HeroCell({
  label,
  value,
  sub,
  delta,
}: {
  label: string
  value: string
  sub?: ReactNode
  delta?: { ratio: number }
}) {
  return (
    <div className="flex flex-col gap-2 px-6 py-7">
      <span className="text-[11px] uppercase tracking-widest text-faint">{label}</span>
      <span className="font-mono text-stat leading-none tabular-nums text-ink sm:text-hero">{value}</span>
      <span className="flex items-center gap-3 font-mono text-[12px] text-muted">
        {sub}
        {delta && (
          <span className={delta.ratio >= 0 ? 'text-gain' : 'text-loss'}>
            {delta.ratio >= 0 ? '▲' : '▼'} {formatRatioPct(Math.abs(delta.ratio), 2)}
          </span>
        )}
      </span>
    </div>
  )
}

function HeroBand({ view, vault }: { view: StrategyView; vault: VaultState }) {
  const perShareRatio = Number(view.navPerShareMinor) / 1e6 - 1

  const positionValue = !vault.connected
    ? '—'
    : vault.position
      ? `$${formatUsdc(vault.position.valueMinor)}`
      : '$0.00'
  const positionSub = !vault.connected ? (
    <span className="text-faint">connect wallet</span>
  ) : vault.position && vault.position.shares > 0n ? (
    `${formatShares(vault.position.shares)} shares · ${formatRatioPct(vault.position.ownership)}`
  ) : (
    'no shares yet'
  )

  return (
    <div className="grid grid-cols-1 divide-y divide-line border-b border-line sm:grid-cols-3 sm:divide-x sm:divide-y-0">
      <HeroCell
        label="AUM"
        value={`$${formatUsdc(view.navMinor)}`}
        sub={
          view.navComplete ? (
            `${view.assets.length} asset${view.assets.length === 1 ? '' : 's'} + USDC`
          ) : (
            <span className="text-accent">partial · unpriced holdings</span>
          )
        }
      />
      <HeroCell
        label="NAV / share"
        value={formatUnits(view.navPerShareMinor, 6, 4)}
        sub="USDC / share"
        delta={view.totalShares > 0n ? { ratio: perShareRatio } : undefined}
      />
      <HeroCell label="My position" value={positionValue} sub={positionSub} />
    </div>
  )
}

function DetailsStrip({ view }: { view: StrategyView }) {
  const s = view.account!
  return (
    <div className="grid grid-cols-2 gap-x-8 gap-y-4 border-t border-line px-6 py-5 sm:grid-cols-3 lg:grid-cols-6">
      <Field label="Manager">
        <Addr value={s.manager} />
      </Field>
      <Field label="USDC mint">
        <Addr value={s.usdcMint} />
      </Field>
      <Field label="Share mint">
        <Addr value={view.shareMint} />
      </Field>
      <Field label="Registry">
        <Addr value={s.registry} />
      </Field>
      <Field label="Strategy PDA">
        <Addr value={view.strategy} />
      </Field>
      <Field label="Program">
        <Addr value={VAULT_PROGRAM_ID} />
      </Field>
    </div>
  )
}

export function InvestorView({ vault }: { vault: VaultState }) {
  const view = vault.view!
  return (
    <div>
      <HeroBand view={view} vault={vault} />
      <div className="grid grid-cols-1 lg:grid-cols-[1fr_380px]">
        <div className="lg:border-r lg:border-line">
          <AllocationPanel view={view} />
          <DetailsStrip view={view} />
        </div>
        <div className="self-start p-6">
          <ActionTicket
            view={view}
            connected={vault.connected}
            walletUsdc={vault.walletUsdc}
            position={vault.position}
            onDeposit={vault.deposit}
            onRedeem={vault.redeem}
          />
        </div>
      </div>
    </div>
  )
}
