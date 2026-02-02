//! Functions for partial state.

use hash_db::Hasher;
use sp_state_machine::Backend;
use sp_state_machine::IterArgs;
use sp_storage::ChildInfo;
use sp_storage::ChildType;
use sp_storage::PrefixedStorageKey;

/// Checks that database has all state trie nodes for given state root.
pub fn check_have_complete_state<H, B>(
	state: B,
) -> sp_blockchain::Result<()>
where
	H: Hasher,
	B: Backend<H>,
{
	let map_err = |e| sp_blockchain::Error::Storage(format!("{e}"));
	for kv in state.pairs(IterArgs::default()).map_err(map_err)? {
		let (key, _value) = kv.map_err(map_err)?;
		let child_info = match ChildType::from_prefixed_key(PrefixedStorageKey::new_ref(&key)) {
			Some((ChildType::ParentKeyId, key)) => ChildInfo::new_default(key),
			None => continue,
		};
		let mut iter_args = IterArgs::default();
		iter_args.child_info = Some(child_info);
		for kv in state.pairs(iter_args).map_err(map_err)? {
			let _ = kv.map_err(map_err)?;
		}
	}
	Ok(())
}
