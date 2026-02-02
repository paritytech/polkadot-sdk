//! Functions for partial state.

use super::PrefixedMemoryDB;
use hash_db::Hasher;

/// Subset of nodes allowing reading part of state trie.
/// This state belongs to block with specified hash and number.
pub struct PartialState<H: Hasher, BlockNumber> {
  pub block_hash: H::Out,
  pub block_number: BlockNumber,
  pub nodes: PrefixedMemoryDB<H>,
}
