# Rent

All storage on Solana costs **[rent](https://solana.com/docs/terminology#rent)**.

In practice, rent is a small amount and [accounts](https://solana.com/docs/terminology#account) that hold at least two years' worth of rent are **rent-exempt** — they pay nothing. If your account holds more [lamports](https://solana.com/docs/terminology#lamport) than the two-year cost, it isn't charged rent.

Rent is calculated from the size of the data stored in the account.
