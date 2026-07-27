import type { ChangeEvent, ReactNode } from 'react'
import type { PublicKey } from '@solana/web3.js'
import { explorerAddress, explorerTx, shortAddress } from '../solana/format'

export function Chip({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 border border-line px-2 py-1 text-[11px] uppercase tracking-widest text-muted">
      {children}
    </span>
  )
}

export function Addr({ value, label }: { value: PublicKey | string; label?: string }) {
  return (
    <a
      href={explorerAddress(value)}
      target="_blank"
      rel="noreferrer"
      className="font-mono text-[12px] text-muted underline-offset-2 transition-colors hover:text-accent hover:underline"
    >
      {label ?? shortAddress(value)}
    </a>
  )
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[10px] uppercase tracking-widest text-faint">{label}</span>
      <span className="font-mono text-[13px] text-ink">{children}</span>
    </div>
  )
}

export function Button({
  children,
  onClick,
  disabled,
  type = 'button',
  variant = 'primary',
}: {
  children: ReactNode
  onClick?: () => void
  disabled?: boolean
  type?: 'button' | 'submit'
  variant?: 'primary' | 'ghost'
}) {
  const base =
    'inline-flex h-11 w-full items-center justify-center rounded-[3px] px-4 font-sans text-[13px] font-semibold uppercase tracking-widest transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 disabled:cursor-not-allowed'
  const styles =
    variant === 'primary'
      ? 'bg-accent text-graphite hover:bg-[#f4c356] disabled:bg-panel2 disabled:text-faint'
      : 'border border-line2 text-ink hover:border-muted disabled:border-line disabled:text-faint'
  return (
    <button type={type} onClick={onClick} disabled={disabled} className={`${base} ${styles}`}>
      {children}
    </button>
  )
}

export function TextField({
  label,
  value,
  onChange,
  right,
  placeholder,
  suffix,
  invalid,
}: {
  label: string
  value: string
  onChange: (value: string) => void
  right?: ReactNode
  placeholder?: string
  suffix?: string
  invalid?: boolean
}) {
  return (
    <label className="block">
      <span className="mb-2 flex items-baseline justify-between">
        <span className="text-[10px] uppercase tracking-widest text-faint">{label}</span>
        {right && <span className="text-[11px] text-muted">{right}</span>}
      </span>
      <span className="flex items-center border border-line bg-panel2 focus-within:border-accent has-[input:focus]:border-accent">
        <input
          value={value}
          onChange={(e: ChangeEvent<HTMLInputElement>) => onChange(e.target.value)}
          placeholder={placeholder}
          inputMode="decimal"
          autoComplete="off"
          spellCheck={false}
          className={`h-11 w-full bg-transparent px-3 font-mono text-[15px] tabular-nums text-ink placeholder:text-faint focus:outline-none ${
            invalid ? 'text-loss' : ''
          }`}
        />
        {suffix && <span className="px-3 font-mono text-[12px] uppercase tracking-widest text-faint">{suffix}</span>}
      </span>
    </label>
  )
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { key: T; label: string }[]
  value: T
  onChange: (key: T) => void
}) {
  return (
    <div className="flex border-b border-line">
      {options.map((o) => {
        const active = o.key === value
        return (
          <button
            key={o.key}
            onClick={() => onChange(o.key)}
            className={`-mb-px border-b-2 px-4 py-2.5 font-sans text-[13px] font-semibold uppercase tracking-widest transition-colors ${
              active
                ? 'border-accent text-ink'
                : 'border-transparent text-faint hover:text-muted'
            }`}
          >
            {o.label}
          </button>
        )
      })}
    </div>
  )
}

export function Select<T extends string>({
  label,
  value,
  onChange,
  options,
}: {
  label: string
  value: T
  onChange: (value: T) => void
  options: { value: T; label: string }[]
}) {
  return (
    <label className="block">
      <span className="mb-2 block text-[10px] uppercase tracking-widest text-faint">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
        className="h-11 w-full appearance-none border border-line bg-panel2 px-3 font-mono text-[14px] text-ink focus:border-accent focus:outline-none"
      >
        {options.map((o) => (
          <option key={o.value} value={o.value} className="bg-panel2 text-ink">
            {o.label}
          </option>
        ))}
      </select>
    </label>
  )
}

export function Panel({
  title,
  hint,
  children,
}: {
  title: string
  hint?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="border border-line bg-panel">
      <div className="flex items-baseline justify-between gap-3 border-b border-line px-5 py-3">
        <h3 className="font-sans text-[12px] font-semibold uppercase tracking-widest text-ink">{title}</h3>
        {hint && <span className="text-right font-mono text-[11px] text-faint">{hint}</span>}
      </div>
      <div className="space-y-4 px-5 py-5">{children}</div>
    </div>
  )
}

export type TxStatus =
  | { kind: 'idle' }
  | { kind: 'pending'; message: string }
  | { kind: 'success'; message: string; signature: string }
  | { kind: 'error'; message: string }

export function StatusLine({ status }: { status: TxStatus }) {
  if (status.kind === 'idle') return null
  if (status.kind === 'pending') {
    return (
      <p className="flex items-center gap-2 font-mono text-[12px] text-muted">
        <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-accent" aria-hidden />
        {status.message}
      </p>
    )
  }
  if (status.kind === 'success') {
    return (
      <p className="font-mono text-[12px] text-gain">
        {status.message}{' '}
        <a
          href={explorerTx(status.signature)}
          target="_blank"
          rel="noreferrer"
          className="text-muted underline underline-offset-2 hover:text-accent"
        >
          {shortAddress(status.signature, 6)} ↗
        </a>
      </p>
    )
  }
  return <p className="break-words font-mono text-[12px] text-loss">{status.message}</p>
}
