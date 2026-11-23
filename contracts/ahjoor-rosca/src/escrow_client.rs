use soroban_sdk::{contractclient, Address, BytesN, Env, Vec};

/// Minimal cross-contract interface used to route a ROSCA payout into an
/// ahjoor-escrow agreement.
#[contractclient(name = "PayoutEscrowClient")]
pub trait PayoutEscrowInterface {
    fn create_escrow(
        env: Env,
        buyer: Address,
        seller: Address,
        arbiter: Address,
        amount: i128,
        token: Address,
        deadline: u64,
        metadata_hash: Option<BytesN<32>>,
        sellers: Vec<(Address, u32)>,
        auto_renew: bool,
        renewal_count: u32,
    ) -> u32;
}
