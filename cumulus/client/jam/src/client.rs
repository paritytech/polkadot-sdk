// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Error;
use jam_std_common::{BlockDesc, Node, NodeExt, Service, WorkPackageStatus};
use jam_types::{
	CoreIndex, HeaderHash, MmrPeakHash, ServiceId, Slot, StateRootHash, WorkPackageHash,
};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

const MAX_RPC_REQUEST_SIZE: u32 = 100 * 1024 * 1024;
const MAX_RPC_RESPONSE_SIZE: u32 = 100 * 1024 * 1024;

/// Connection configuration for a JAM node.
#[derive(Debug, Clone)]
pub struct JamClientConfig {
	pub url: String,
}

impl Default for JamClientConfig {
	fn default() -> Self {
		Self { url: "ws://localhost:19800".into() }
	}
}

/// Client for interacting with a JAM node via WebSocket RPC.
///
/// Wraps a `jsonrpsee::WsClient` and provides the full JAM `Node` interface
/// for querying chain state, submitting work packages, and monitoring status.
pub struct JamClient {
	inner: WsClient,
	config: JamClientConfig,
}

impl JamClient {
	pub async fn connect(config: JamClientConfig) -> Result<Self, Error> {
		let url = url::Url::parse(&config.url)?;
		let client = WsClientBuilder::default()
			.max_request_size(MAX_RPC_REQUEST_SIZE)
			.max_response_size(MAX_RPC_RESPONSE_SIZE)
			.build(url.as_str())
			.await?;

		tracing::info!(target: "cumulus-jam", url = %config.url, "Connected to JAM node");

		Ok(Self { inner: client, config })
	}

	pub fn config(&self) -> &JamClientConfig {
		&self.config
	}

	pub async fn best_block(&self) -> Result<BlockDesc, Error> {
		Ok(Node::best_block(&self.inner).await?)
	}

	pub async fn finalized_block(&self) -> Result<BlockDesc, Error> {
		Ok(Node::finalized_block(&self.inner).await?)
	}
	pub async fn parent(&self, header_hash: HeaderHash) -> Result<BlockDesc, Error> {
		Ok(Node::parent(&self.inner, header_hash).await?)
	}

	pub async fn service_data(
		&self,
		header_hash: HeaderHash,
		id: ServiceId,
	) -> Result<Option<Service>, Error> {
		Ok(NodeExt::service_data(&self.inner, header_hash, id).await?)
	}

	pub async fn state_root(&self, header_hash: HeaderHash) -> Result<StateRootHash, Error> {
		Ok(Node::state_root(&self.inner, header_hash).await?)
	}

	pub async fn beefy_root(&self, header_hash: HeaderHash) -> Result<MmrPeakHash, Error> {
		Ok(Node::beefy_root(&self.inner, header_hash).await?)
	}

	pub async fn submit_work_package(
		&self,
		core: CoreIndex,
		package: bytes::Bytes,
		extrinsics: &[bytes::Bytes],
	) -> Result<(), Error> {
		Node::submit_encoded_work_package(&self.inner, core, package, extrinsics).await?;
		Ok(())
	}

	pub async fn work_package_status(
		&self,
		header_hash: HeaderHash,
		hash: WorkPackageHash,
		anchor: HeaderHash,
	) -> Result<WorkPackageStatus, Error> {
		Ok(Node::work_package_status(&self.inner, header_hash, hash, anchor).await?)
	}

	pub async fn submit_preimage(
		&self,
		requester: ServiceId,
		preimage: bytes::Bytes,
	) -> Result<(), Error> {
		Node::submit_preimage(&self.inner, requester, preimage).await?;
		Ok(())
	}

	pub async fn service_request(
		&self,
		header_hash: HeaderHash,
		id: ServiceId,
		hash: jam_types::Hash,
		len: u32,
	) -> Result<Option<Vec<Slot>>, Error> {
		Ok(Node::service_request(&self.inner, header_hash, id, hash, len).await?)
	}
	pub async fn list_services(
		&self,
		header_hash: HeaderHash,
	) -> Result<Vec<ServiceId>, Error> {
		Ok(Node::list_services(&self.inner, header_hash).await?)
	}

	pub fn inner(&self) -> &WsClient {
		&self.inner
	}
}
