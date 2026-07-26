// Offline verification of the hand-authored client. No cluster needed: it re-derives
// the Anchor discriminators, builds the Program from the IDL, round-trips instruction
// encoding and account decoding through Anchor's own coder, and derives the PDAs.
//
//   node scripts/verify-client.mjs
//
// Exits non-zero on any mismatch, so it doubles as a CI gate.

import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import * as anchor from '@coral-xyz/anchor'
import { Connection, Keypair, PublicKey } from '@solana/web3.js'

const { Program, AnchorProvider, BN } = anchor

const idl = JSON.parse(readFileSync(new URL('../src/idl/vault_strategy.json', import.meta.url)))
const PROGRAM_ID = new PublicKey(idl.address)

let failures = 0
const ok = (name) => console.log(`  ✓ ${name}`)
const fail = (name, detail) => {
  failures++
  console.error(`  ✗ ${name}${detail ? ` — ${detail}` : ''}`)
}
const eqBytes = (a, b) => a.length === b.length && a.every((v, i) => v === b[i])
const snakeToCamel = (s) => s.replace(/_([a-z])/g, (_, c) => c.toUpperCase())
const disc = (prefix, name) =>
  Array.from(createHash('sha256').update(`${prefix}:${name}`).digest().subarray(0, 8))

// 1. Discriminators embedded in the IDL match sha256(global:/account:).
console.log('discriminators')
for (const ix of idl.instructions) {
  const expected = disc('global', ix.name)
  eqBytes(expected, ix.discriminator)
    ? ok(`ix ${ix.name}`)
    : fail(`ix ${ix.name}`, `expected [${expected}] got [${ix.discriminator}]`)
}
for (const acc of idl.accounts) {
  const expected = disc('account', acc.name)
  eqBytes(expected, acc.discriminator)
    ? ok(`account ${acc.name}`)
    : fail(`account ${acc.name}`, `expected [${expected}] got [${acc.discriminator}]`)
}

// 2. The Program builds from the IDL (validates the whole IDL shape + coder).
console.log('program')
const connection = new Connection('http://127.0.0.1:8899', 'confirmed') // never called
const kp = Keypair.generate()
const wallet = {
  publicKey: kp.publicKey,
  signTransaction: async (t) => t,
  signAllTransactions: async (t) => t,
}
const provider = new AnchorProvider(connection, wallet, { commitment: 'confirmed' })
let program
try {
  program = new Program(idl, provider)
  ok('new Program(idl, provider)')
} catch (e) {
  fail('new Program(idl, provider)', e.message)
}

// 3. Encode every instruction; assert the discriminator prefix matches.
if (program) {
  console.log('instruction encoding')
  const argSamples = {
    u64: () => new BN(1000),
    u16: () => 100,
    u8: () => 1,
    pubkey: () => Keypair.generate().publicKey,
  }
  for (const ix of idl.instructions) {
    try {
      const accounts = Object.fromEntries(
        ix.accounts.map((a) => [snakeToCamel(a.name), Keypair.generate().publicKey]),
      )
      const args = ix.args.map((a) => (argSamples[a.type] ?? (() => new BN(0)))())
      const built = await program.methods[snakeToCamel(ix.name)](...args)
        .accountsStrict(accounts)
        .instruction()
      const prefix = Array.from(built.data.subarray(0, 8))
      if (eqBytes(prefix, ix.discriminator)) ok(`encode ${ix.name} (${built.data.length} bytes)`)
      else fail(`encode ${ix.name}`, `data prefix [${prefix}]`)
    } catch (e) {
      fail(`encode ${ix.name}`, e.message)
    }
  }

  // 4. Round-trip a Strategy account through the coder.
  console.log('account decoding')
  try {
    const sample = {
      index: new BN(7),
      manager: PublicKey.default,
      registry: PublicKey.default,
      shareMint: PublicKey.default,
      usdcMint: PublicKey.default,
      swapRouter: PublicKey.default,
      feeBps: 100,
      maxSlippageBps: 250,
      totalShares: new BN('1350000000'),
      lastFeeAccrualTimestamp: new BN('1700000000'),
      assetCount: 2,
      totalWeightBps: 10000,
      bump: 254,
    }
    // Anchor 0.30 keys the coder by camelCase account name (matches program.account.strategy).
    const encoded = await program.coder.accounts.encode('strategy', sample)
    const decoded = program.coder.accounts.decode('strategy', encoded)
    const good =
      decoded.index.toString() === '7' &&
      decoded.feeBps === 100 &&
      decoded.maxSlippageBps === 250 &&
      decoded.assetCount === 2 &&
      decoded.totalWeightBps === 10000 &&
      decoded.totalShares.toString() === '1350000000'
    good ? ok('Strategy encode/decode round-trip') : fail('Strategy round-trip', JSON.stringify(decoded))
  } catch (e) {
    fail('Strategy round-trip', e.message)
  }
}

// 5. Derive the key PDAs (sanity + reference values).
console.log('PDAs (index 0)')
const u64le = (n) => {
  const b = Buffer.alloc(8)
  b.writeBigUInt64LE(BigInt(n))
  return b
}
const strategy = PublicKey.findProgramAddressSync(
  [Buffer.from('strategy'), u64le(0)],
  PROGRAM_ID,
)[0]
const shareMint = PublicKey.findProgramAddressSync(
  [Buffer.from('share_mint'), strategy.toBuffer()],
  PROGRAM_ID,
)[0]
const asset0 = PublicKey.findProgramAddressSync(
  [Buffer.from('asset'), strategy.toBuffer(), Buffer.from([0])],
  PROGRAM_ID,
)[0]
console.log(`  strategy   ${strategy.toBase58()}`)
console.log(`  share_mint ${shareMint.toBase58()}`)
console.log(`  asset[0]   ${asset0.toBase58()}`)
ok('PDA derivation')

console.log()
if (failures > 0) {
  console.error(`FAILED: ${failures} check(s) did not pass.`)
  process.exit(1)
}
console.log('OK: client wiring verified offline.')
