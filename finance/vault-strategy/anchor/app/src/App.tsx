import { Header } from './components/Header'
import { ErrorPanel, Loading, NotFound } from './components/states'
import { InvestorView } from './views/InvestorView'
import { useVault } from './hooks/useVault'

export default function App() {
  const vault = useVault()

  return (
    <div className="flex min-h-full flex-col">
      <Header onRefresh={vault.refresh} />

      <main className="flex-1">
        {vault.loading && <Loading />}
        {!vault.loading && vault.error && <ErrorPanel message={vault.error} />}
        {!vault.loading &&
          !vault.error &&
          vault.view &&
          (vault.view.exists ? <InvestorView vault={vault} /> : <NotFound view={vault.view} />)}
      </main>

      <footer className="border-t border-line px-6 py-4 text-[11px] leading-relaxed text-faint">
        Educational demo · devnet. Every figure is a live on-chain read; redemptions pay out each
        asset in kind, not USDC.
      </footer>
    </div>
  )
}
