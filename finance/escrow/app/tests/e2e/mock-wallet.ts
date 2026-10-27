// A Wallet Standard wallet for tests, as a script to run in the page before
// the app loads. It holds one Ed25519 key, imported into the browser's
// WebCrypto, approves every connection, and signs every transaction it is
// asked to, so a test can drive the app the way a person approving every
// prompt would.
//
// It registers itself the two ways the Wallet Standard defines: by
// dispatching `wallet-standard:register-wallet`, and by answering an app's
// `wallet-standard:app-ready`.

// PKCS#8 wraps a raw 32-byte Ed25519 private key behind this fixed prefix.
const PKCS8_ED25519_PREFIX = [
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

export const mockWalletScript = (privateKeyBytes: Uint8Array, address: string, chain: string): string => `
(() => {
  const PKCS8 = new Uint8Array(${JSON.stringify([...PKCS8_ED25519_PREFIX, ...privateKeyBytes])});
  const ADDRESS = ${JSON.stringify(address)};
  const CHAIN = ${JSON.stringify(chain)};
  const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

  const base58Decode = (text) => {
    let value = 0n;
    for (const character of text) value = value * 58n + BigInt(ALPHABET.indexOf(character));
    const bytes = [];
    while (value > 0n) { bytes.unshift(Number(value % 256n)); value /= 256n; }
    for (const character of text) { if (character !== "1") break; bytes.unshift(0); }
    return new Uint8Array(bytes);
  };

  const publicKey = base58Decode(ADDRESS);
  const keyPromise = crypto.subtle.importKey("pkcs8", PKCS8, { name: "Ed25519" }, false, ["sign"]);

  // A compact-u16 is how the wire format writes short lengths.
  const readCompactU16 = (bytes, offset) => {
    let value = 0, size = 0;
    for (;;) {
      const byte = bytes[offset + size];
      value |= (byte & 0x7f) << (7 * size);
      size += 1;
      if ((byte & 0x80) === 0) return [value, size];
    }
  };

  // A transaction is its signatures, then the message they sign. The
  // message's account list starts with the signers, in signature order.
  const signTransaction = async (transaction) => {
    const [signatureCount, prefixSize] = readCompactU16(transaction, 0);
    const messageOffset = prefixSize + signatureCount * 64;
    const message = transaction.slice(messageOffset);
    const isVersioned = (message[0] & 0x80) !== 0;
    const headerOffset = isVersioned ? 1 : 0;
    const [accountCount, accountCountSize] = readCompactU16(message, headerOffset + 3);
    const keysOffset = headerOffset + 3 + accountCountSize;
    let signerIndex = -1;
    for (let index = 0; index < Math.min(accountCount, signatureCount); index++) {
      const key = message.slice(keysOffset + index * 32, keysOffset + (index + 1) * 32);
      if (key.every((byte, byteIndex) => byte === publicKey[byteIndex])) signerIndex = index;
    }
    if (signerIndex === -1) throw new Error("The test wallet is not a signer of this transaction");
    const signature = new Uint8Array(await crypto.subtle.sign("Ed25519", await keyPromise, message));
    const signed = transaction.slice();
    signed.set(signature, prefixSize + signerIndex * 64);
    return signed;
  };

  const listeners = { change: new Set() };
  const account = Object.freeze({
    address: ADDRESS,
    publicKey,
    chains: [CHAIN],
    features: ["solana:signTransaction"],
    label: "Test account",
  });

  const wallet = {
    version: "1.0.0",
    name: "Test Wallet",
    icon: "data:image/svg+xml;base64," + btoa('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="6" fill="#1e7a3c"/></svg>'),
    chains: [CHAIN],
    accounts: [],
    features: {
      "standard:connect": {
        version: "1.0.0",
        connect: async () => {
          wallet.accounts = [account];
          for (const listener of listeners.change) listener({ accounts: wallet.accounts });
          return { accounts: wallet.accounts };
        },
      },
      "standard:disconnect": {
        version: "1.0.0",
        disconnect: async () => {
          wallet.accounts = [];
          for (const listener of listeners.change) listener({ accounts: wallet.accounts });
        },
      },
      "standard:events": {
        version: "1.0.0",
        on: (event, listener) => {
          listeners[event]?.add(listener);
          return () => listeners[event]?.delete(listener);
        },
      },
      "solana:signTransaction": {
        version: "1.0.0",
        supportedTransactionVersions: ["legacy", 0],
        signTransaction: async (...inputs) => {
          window.__testWalletSignCount = (window.__testWalletSignCount ?? 0) + inputs.length;
          return Promise.all(inputs.map(async ({ transaction }) => ({ signedTransaction: await signTransaction(transaction) })));
        },
      },
    },
  };

  const register = ({ register }) => register(wallet);
  window.addEventListener("wallet-standard:app-ready", ({ detail }) => register(detail));
  window.dispatchEvent(new CustomEvent("wallet-standard:register-wallet", { detail: register }));
})();
`;
