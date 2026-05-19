# Terminology Rules — CLOB

Project-wide rules for the README and code comments in this crate. Applies
throughout, not just one section. Audit for these before opening / merging
any PR that touches docs or comments.

## General rule

> If a word has two meanings within ~1 paragraph of context, use the
> explicit phrase even if it's a few chars longer.

A wrong or ambiguous statement is worse than no statement.

## Overloaded terms

### "balance" — most important

`balance` is ambiguous in a CLOB context: it can mean token balance,
account balance, **or** the red-black tree property. Pick one of:

- ✅ "tree balancing"
- ✅ "balanced tree shape"
- ✅ "keeps the tree shape balanced"
- ❌ "the red-black tree maintains balance"

For money, use **"token balance"** or **"account balance"** explicitly
whenever it's near a tree discussion.

### "book"

Usually fine in context ("order book"). Be careful in mixed sentences —
qualify when ambiguity is possible.

### "order" vs "ordering"

`order` = trading order. `ordering` = sort order. When a sentence touches
both:

- ✅ "price-sorted", "sorted by price"
- ❌ "in price order"

### "settle"

Be specific:

- ✅ `settle_funds` instruction
- ✅ "transaction settlement"
- ❌ bare "settle" / "settlement" when the meaning isn't obvious

### "key"

Never bare `key`. Use:

- "address" (a Solana public key used as an address)
- "public key" (a cryptographic key)
- "sort key" (the value the tree is sorted on)

### "node"

Qualify:

- "validator node" (Solana cluster)
- "tree node" / "slab node" (data structure)

### "position"

Qualify:

- "trading position" (financial)
- "slot position" / "array index" (slab / array)

## Reviewer checklist

Before approving a doc/comment PR:

- [ ] No bare "balance" when the red-black tree is in scope.
- [ ] No bare "key", "node", "position", "settle" in ambiguous contexts.
- [ ] "Order" / "ordering" disambiguated when both senses appear nearby.
