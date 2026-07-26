import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { nodePolyfills } from 'vite-plugin-node-polyfills';
// @solana/web3.js and @coral-xyz/anchor expect Node's Buffer/process in the browser.
export default defineConfig({
    plugins: [
        react(),
        nodePolyfills({
            include: ['buffer', 'process'],
            globals: { Buffer: true, global: true, process: true },
        }),
    ],
});
