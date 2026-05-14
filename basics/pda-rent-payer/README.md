# PDA Rent-Payer

Use a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) to pay [rent](https://solana.com/docs/terminology#rent) for a new [account](https://solana.com/docs/terminology#account).

Accounts on Solana are created under ownership of the System Program when you transfer [lamports](https://solana.com/docs/terminology#lamport) to them, so you can pay for a new account simply by transferring lamports from your PDA to the new account's public key.
