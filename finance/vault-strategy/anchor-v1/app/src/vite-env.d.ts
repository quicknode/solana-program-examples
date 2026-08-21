/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_RPC_URL?: string;
  readonly VITE_VAULT_PROGRAM_ID?: string;
  readonly VITE_ROUTER_PROGRAM_ID?: string;
  readonly VITE_USDC_MINT?: string;
  readonly VITE_STRATEGY_INDEX?: string;
  readonly VITE_CLUSTER?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
