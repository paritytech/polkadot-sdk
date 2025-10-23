use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::ErrorObjectOwned};
use polkadot_sdk::sc_client_api::backend::Backend;
use polkadot_sdk::sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use revive_dev_runtime::OpaqueBlock;

use crate::service::FullBackend;

#[derive(Clone, Debug)]
pub struct Snapshot {
	pub best_number: u32,
}

pub struct SnapshotManager<C> {
	client: Arc<C>,
	backend: Arc<FullBackend>,
	next_snapshot_id: RwLock<u64>,
	snapshots: RwLock<BTreeMap<u64, Snapshot>>,
}

impl<C> SnapshotManager<C> {
	pub fn new(client: Arc<C>, backend: Arc<FullBackend>) -> Self {
		let snapshot = Snapshot { best_number: 0 };
		let mut map = BTreeMap::new();
		map.insert(0, snapshot);

		Self {
			client,
			backend,
			// Start with 1 to mimic Ganache
			next_snapshot_id: RwLock::new(1),
			snapshots: RwLock::new(map),
		}
	}
}

#[rpc(server)]
pub trait SnapshotRpc {
	#[method(name = "evm_snapshot")]
	fn snapshot(&self) -> RpcResult<u64>;
	#[method(name = "evm_revert")]
	fn revert(&self, id: u64) -> RpcResult<bool>;
	#[method(name = "hardhat_reset")]
	fn reset(&self) -> RpcResult<bool>;
}

impl<C> SnapshotRpcServer for SnapshotManager<C>
where
	C: HeaderBackend<OpaqueBlock>
		+ HeaderMetadata<OpaqueBlock, Error = BlockChainError>
		+ Send
		+ Sync
		+ 'static,
{
	fn snapshot(&self) -> RpcResult<u64> {
		let mut id_lock = self.next_snapshot_id.write().unwrap();
		let id = *id_lock;
		*id_lock += 1;

		let snapshot = Snapshot { best_number: self.client.info().best_number };
		self.snapshots.write().unwrap().insert(id, snapshot);

		Ok(id)
	}

	fn revert(&self, id: u64) -> RpcResult<bool> {
		let maybe_snapshot = { self.snapshots.read().unwrap().get(&id).cloned() };
		let Some(snap) = maybe_snapshot else { return Ok(false) };

		let current_best_number = self.client.info().best_number;
		let number_of_blocks_to_revert: u32 = current_best_number - snap.best_number;

		self.backend.revert(number_of_blocks_to_revert, true).map_err(|e| {
			ErrorObjectOwned::owned(-32000, "backend revert failed", Some(e.to_string()))
		})?;

		self.snapshots.write().unwrap().retain(|&k, _| k < id);

		Ok(true)
	}

	fn reset(&self) -> RpcResult<bool> {
		let current_best_number = self.client.info().best_number;

		self.backend.revert(current_best_number, true).map_err(|e| {
			ErrorObjectOwned::owned(-32000, "backend revert failed", Some(e.to_string()))
		})?;

		Ok(true)
	}
}
