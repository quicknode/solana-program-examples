import React, { useMemo } from 'react'
import ReactDOM from 'react-dom/client'
import { ConnectionProvider, WalletProvider } from '@solana/wallet-adapter-react'
import { WalletModalProvider } from '@solana/wallet-adapter-react-ui'
import { PhantomWalletAdapter, SolflareWalletAdapter } from '@solana/wallet-adapter-wallets'
import '@solana/wallet-adapter-react-ui/styles.css'
import './index.css'
import App from './App'
import { COMMITMENT, RPC_URL } from './solana/config'

function Root() {
  // Phantom/Solflare also register via Wallet Standard; listed for reliability in the demo.
  const wallets = useMemo(() => [new PhantomWalletAdapter(), new SolflareWalletAdapter()], [])
  return (
    <ConnectionProvider endpoint={RPC_URL} config={{ commitment: COMMITMENT }}>
      <WalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>
          <App />
        </WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  )
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
)
