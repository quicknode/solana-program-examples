use sha2::{Digest, Sha256};

// Hash the hackathon name into a fixed-size seed slice so the Hackathon PDA
// has a fixed seed layout regardless of input length. We use SHA-256 via the
// `sha2` crate because Anchor 1.0's curated `solana_program` re-export does
// not include a hash module.
pub fn name_seed(name: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.finalize().into()
}
