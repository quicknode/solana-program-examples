import { useState } from "react";
import { Header } from "./components/Header";
import { CreateFundForm, ManagerView } from "./components/ManagerView";
import { ErrorPanel, Loading, NotFound } from "./components/states";
import { useFund } from "./hooks/useFund";
import { InvestorView } from "./views/InvestorView";

type Tab = "investor" | "manager";

function TabBar({ tab, onTab }: { tab: Tab; onTab: (t: Tab) => void }) {
  const item = (key: Tab, label: string) => (
    <button
      type="button"
      onClick={() => onTab(key)}
      className={`-mb-px border-b-2 px-4 py-3 font-sans text-[12px] font-semibold uppercase tracking-widest transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 ${
        tab === key ? "border-accent text-ink" : "border-transparent text-faint hover:text-muted"
      }`}
    >
      {label}
    </button>
  );
  return (
    <nav className="flex border-b border-line px-6">
      {item("investor", "Investor")}
      {item("manager", "Manager")}
    </nav>
  );
}

export default function App() {
  const fund = useFund();
  const [tab, setTab] = useState<Tab>("investor");

  const showManagerTab = Boolean(fund.view?.exists && fund.isManager);
  const activeTab: Tab = showManagerTab ? tab : "investor";

  return (
    <div className="flex min-h-full flex-col">
      <Header onRefresh={fund.refresh} />
      {showManagerTab && <TabBar tab={activeTab} onTab={setTab} />}

      <main className="flex-1">
        {fund.loading && <Loading />}
        {!fund.loading && fund.error && <ErrorPanel message={fund.error} />}
        {!fund.loading && !fund.error && fund.view && (
          <div key={activeTab} className="animate-rise">
            {fund.view.exists ? (
              activeTab === "manager" ? (
                <ManagerView fund={fund} />
              ) : (
                <InvestorView fund={fund} />
              )
            ) : (
              <>
                <NotFound view={fund.view} />
                {fund.connected && (
                  <div className="px-6 pb-12">
                    <CreateFundForm fund={fund} />
                  </div>
                )}
              </>
            )}
          </div>
        )}
      </main>

      <footer className="border-t border-line px-6 py-4 text-[11px] leading-relaxed text-faint">
        Educational demo · devnet. Every figure is a live on-chain read; redemptions pay out each asset in kind, not
        USDC.
      </footer>
    </div>
  );
}
