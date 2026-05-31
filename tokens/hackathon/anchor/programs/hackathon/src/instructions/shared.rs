use solana_sha256_hasher::hash;

// Hash the hackathon name into a fixed-size seed so the Hackathon PDA has a
// fixed seed layout regardless of input length. `hash` lowers to the
// `sol_sha256` syscall onchain.
pub fn name_seed(name: &str) -> [u8; 32] {
    hash(name.as_bytes()).to_bytes()
}
