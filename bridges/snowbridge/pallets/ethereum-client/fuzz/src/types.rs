use libfuzzer_sys::arbitrary;

#[derive(arbitrary::Arbitrary, Debug, Clone)]
pub struct FuzzCheckpointUpdate {
	pub header: FuzzBeaconHeader,
	pub current_sync_committee: FuzzSyncCommittee,
	pub current_sync_committee_branch: Vec<[u8; 32]>,
	pub validators_root: [u8; 32],
	pub block_roots_root: [u8; 32],
	pub block_roots_branch: Vec<[u8; 32]>,
}

#[derive(arbitrary::Arbitrary, Debug, Clone)]
pub struct FuzzBeaconHeader {
	pub slot: u64,
	pub proposer_index: u64,
	pub parent_root: [u8; 32],
	pub state_root: [u8; 32],
	pub body_root: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct FuzzSyncCommittee {
	pub pubkeys: [[u8; 48]; 32],
	pub aggregate_pubkey: [u8; 48],
}

#[derive(arbitrary::Arbitrary, Debug, Clone)]
pub struct FuzzUpdate {
	pub attested_header: FuzzBeaconHeader,
	pub sync_aggregate: FuzzSyncAggregate,
	pub signature_slot: u64,
	pub next_sync_committee_update: Option<FuzzNextSyncCommitteeUpdate>,
	pub finalized_header: FuzzBeaconHeader,
	pub finality_branch: Vec<[u8; 32]>,
	pub block_roots_root: [u8; 32],
	pub block_roots_branch: Vec<[u8; 32]>,
}

#[derive(arbitrary::Arbitrary, Debug, Clone)]
pub struct FuzzSyncAggregate {
	pub sync_committee_bits: [u8; 4],
	pub sync_committee_signature: [u8; 96],
}

#[derive(arbitrary::Arbitrary, Debug, Clone)]
pub struct FuzzNextSyncCommitteeUpdate {
	pub next_sync_committee: FuzzSyncCommittee,
	pub next_sync_committee_branch: Vec<[u8; 32]>,
}

#[derive(arbitrary::Arbitrary, Debug, Clone)]
pub struct FuzzExecutionHeaderUpdate {
	pub header: FuzzBeaconHeader,
	pub ancestry_proof: Option<FuzzAncestryProof>,
	pub execution_header: FuzzExecutionPayloadHeader,
	pub execution_branch: Vec<[u8; 32]>,
}

#[derive(arbitrary::Arbitrary, Debug, Clone)]
pub struct FuzzAncestryProof {
	pub header_branch: Vec<[u8; 32]>,
	pub finalized_block_root: [u8; 32],
}

#[derive(arbitrary::Arbitrary, Debug, Clone)]
pub struct FuzzExecutionPayloadHeader {
	pub parent_hash: [u8; 32],
	pub fee_recipient: [u8; 20],
	pub state_root: [u8; 32],
	pub receipts_root: [u8; 32],
	pub logs_bloom: Vec<u8>,
	pub prev_randao: [u8; 32],
	pub block_number: u64,
	pub gas_limit: u64,
	pub gas_used: u64,
	pub timestamp: u64,
	pub extra_data: Vec<u8>,
	pub base_fee_per_gas: [u8; 32],
	pub block_hash: [u8; 32],
	pub transactions_root: [u8; 32],
	pub withdrawals_root: [u8; 32],
}
