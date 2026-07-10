//! TEMP diagnosis (remove after): load a dumped SCALE-encoded `CompactProof` and report how much
//! of it is duplicate node blobs, broken down by node kind.
//!
//! The dump is produced by the `state-sync-debug` instrumentation in `read_proof_collection`.
//! A structurally re-emitted shared subtree is byte-identical each time, so exact-byte dedup of
//! `encoded_nodes` measures the structural re-emission that value-only dedup does NOT remove.
//!
//! Run (Asset Hub uses BlakeTwo256):
//!   cargo run -p sp-trie --features std --example decode_compact_dump -- /tmp/compact-proof-oversized.scale

use codec::Decode;
use sp_trie::{CompactProof, NodeCodec};
use std::collections::HashMap;
use trie_db::node::Node;

type Hasher = sp_core::Blake2Hasher;
type Codec = NodeCodec<Hasher>;

fn kind(blob: &[u8]) -> &'static str {
	match <Codec as trie_db::NodeCodec>::decode(blob) {
		Ok(Node::Empty) => "empty",
		Ok(Node::Leaf(..)) => "leaf",
		Ok(Node::Extension(..)) => "extension",
		Ok(Node::Branch(..)) => "branch",
		Ok(Node::NibbledBranch(..)) => "nibbled-branch",
		// Detached values are pushed as raw value bytes, not trie nodes -> don't decode.
		Err(_) => "value/undecodable",
	}
}

fn mib(bytes: usize) -> String {
	format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
}

fn main() {
	let path = std::env::args()
		.nth(1)
		.or_else(|| std::env::var("STATE_SYNC_DUMP_PATH").ok())
		.unwrap_or_else(|| "/tmp/compact-proof-oversized.scale".to_string());

	let bytes = std::fs::read(&path).expect("failed to read dump file");
	let proof = CompactProof::decode(&mut &bytes[..]).expect("failed to SCALE-decode CompactProof");

	let total_nodes = proof.encoded_nodes.len();
	let total_bytes: usize = proof.encoded_nodes.iter().map(|n| n.len()).sum();

	// Group identical blobs: blob -> (occurrences, blob_len).
	let mut groups: HashMap<&Vec<u8>, usize> = HashMap::new();
	for n in &proof.encoded_nodes {
		*groups.entry(n).or_insert(0) += 1;
	}
	let distinct_nodes = groups.len();
	let distinct_bytes: usize = groups.keys().map(|n| n.len()).sum();

	println!("file: {path} ({} on disk)", mib(bytes.len()));
	println!("total_nodes    = {total_nodes}  ({})", mib(total_bytes));
	println!("distinct_nodes = {distinct_nodes}  ({})", mib(distinct_bytes));
	println!(
		"re-emission    = {:.1}x nodes, wasted {} ({:.1}% of proof)",
		total_nodes as f64 / distinct_nodes.max(1) as f64,
		mib(total_bytes - distinct_bytes),
		100.0 * (total_bytes - distinct_bytes) as f64 / total_bytes.max(1) as f64,
	);

	// Per-kind breakdown of duplication: kind -> (distinct, occurrences, wasted_bytes).
	let mut by_kind: HashMap<&'static str, (usize, usize, usize)> = HashMap::new();
	for (blob, &count) in &groups {
		let e = by_kind.entry(kind(blob)).or_insert((0, 0, 0));
		e.0 += 1;
		e.1 += count;
		e.2 += blob.len() * (count - 1); // bytes wasted by re-emitting this blob
	}
	let mut kinds: Vec<_> = by_kind.into_iter().collect();
	kinds.sort_by_key(|(_, (_, _, wasted))| std::cmp::Reverse(*wasted));
	println!("\nduplication by node kind (sorted by wasted bytes):");
	for (k, (distinct, occ, wasted)) in kinds {
		println!("  {k:<18} distinct={distinct:<8} occurrences={occ:<9} wasted={}", mib(wasted));
	}

	// Top individual offenders.
	let mut top: Vec<_> = groups.into_iter().collect();
	top.sort_by_key(|(blob, count)| std::cmp::Reverse(blob.len() * (*count - 1)));
	println!("\ntop 10 re-emitted blobs:");
	for (blob, count) in top.into_iter().take(10) {
		let prefix: String =
			blob.iter().take(8).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join("");
		println!(
			"  kind={:<18} x{count:<7} len={:<5} wasted={} prefix={prefix}..",
			kind(blob),
			blob.len(),
			mib(blob.len() * (count - 1)),
		);
	}
}
