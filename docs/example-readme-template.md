# Example README template

Copy this structure for every example's framework directory (`anchor/`, `quasar/`, `native/`, etc.). Replace the placeholder text; delete sections that genuinely do not apply.

## Conventions

- **H1 format**: `# Solana <Example> (<Framework>)`, for example `# Solana Escrow (Anchor)`. One H1 per file. Each example directory is its own indexable page, so the H1 names the example the way someone would search for it.
- **Definition-first opener**: the first sentence must say what the program is, in a way that stands alone when quoted out of context, and include the word "Solana". Good: "A Solana escrow is a program that holds a maker's tokens in a vault until a taker delivers the requested tokens." Bad: "This directory contains the Anchor version."
- **No em-dashes** in prose. Use a colon, comma, or a new sentence.
- Write American English. Fenced code blocks include a language tag. Link canonical Solana terms to the [terminology page](https://solana.com/docs/references/terminology) on first mention.
- Name the actual instruction handlers (`make_offer`, `take_offer`) in lifecycle prose so readers can connect the narrative to the code. Confirm each identifier exists in the source before naming it.

## Template

```markdown
# Solana <Example> (<Framework>)

<One-paragraph definition-first opener: what the program is, what real-world
mechanism it mirrors, and which production Solana protocols use the design.>

<Links to the other framework variants of the same example.>

## Major concepts

<Key accounts and PDAs with their seeds, the state structures, and the
program logic. For finance examples, explain the financial mechanics in
plain terms before the code.>

## Lifecycle

<Who calls which instruction handler, in order, and what each one does.>

## Setup

<Prerequisites and build command, e.g. `anchor build`.>

## Testing

<How to run the tests (e.g. `cargo test` against LiteSVM) and what the
suite covers.>

## FAQ

<Finance examples only, and only in the anchor/ variant to avoid duplicate
content: three or four questions phrased the way someone would ask them
("How does an escrow work on Solana?"), each answered in two to four
self-contained sentences that name the relevant instruction handlers.>
```
