// Codama client generator.
//
// Reads the Shank-generated IDL (program/idl/car_rental_service.json) and emits
// a Rust client into clients/rust/src/generated/. The wrapper crate at
// clients/rust/ re-exports the generated module; the program's Rust + LiteSVM
// tests (program/tests/) drive the program through it.
//
// Flow: read IDL -> rootNodeFromAnchor (origin = "shank" so the u8 instruction
// discriminants are interpreted correctly) -> createFromRoot -> render Rust.
//
// Run with: pnpm generate-client

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { type AnchorIdl, rootNodeFromAnchor } from "@codama/nodes-from-anchor";
import { renderVisitor } from "@codama/renderers-rust";
import { createFromRoot } from "codama";

const here = dirname(fileURLToPath(import.meta.url));
const idlPath = join(here, "program", "idl", "car_rental_service.json");
const outDir = join(here, "clients", "rust", "src", "generated");

const idl = JSON.parse(readFileSync(idlPath, "utf-8")) as AnchorIdl;

// Make sure Codama treats this as a Shank IDL. Shank uses single-byte (u8)
// instruction discriminants rather than Anchor's 8-byte hashes, and the
// "origin" field is what tells nodes-from-anchor to honour the explicit
// `discriminant` values in the IDL.
const idlWithOrigin = {
  ...idl,
  metadata: { ...idl.metadata, origin: "shank" },
} as AnchorIdl;

const codama = createFromRoot(rootNodeFromAnchor(idlWithOrigin));

await codama.accept(
  renderVisitor(outDir, {
    deleteFolderBeforeRendering: true,
    crateFolder: join(here, "clients", "rust"),
  }),
);

console.log(`Codama: generated Rust client in ${outDir}`);
