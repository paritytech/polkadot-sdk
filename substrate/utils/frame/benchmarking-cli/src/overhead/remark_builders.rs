use crate::extrinsic::ExtrinsicBuilder;
use codec::{Decode, Encode};
use sc_client_api::UsageProvider;
use sc_executor::WasmExecutor;
use sp_api::{Core, Metadata, ProvideRuntimeApi};
use sp_core::{
	traits::{CallContext, CodeExecutor, FetchRuntimeCode, RuntimeCode},
	OpaqueMetadata,
};
use sp_runtime::{traits::Block as BlockT, OpaqueExtrinsic};
use sp_state_machine::BasicExternalities;
use sp_version::RuntimeVersion;
use sp_wasm_interface::HostFunctions;
use std::{borrow::Cow, sync::Arc};
use subxt::{
	client::RuntimeVersion as SubxtRuntimeVersion,
	config::substrate::SubstrateExtrinsicParamsBuilder, Config, OfflineClient, SubstrateConfig,
};

pub type SubstrateRemarkBuilder = DynamicRemarkBuilder<SubstrateConfig>;

pub struct DynamicRemarkBuilder<C: Config> {
	offline_client: OfflineClient<C>,
}

impl<C: Config> DynamicRemarkBuilder<C> {
	pub fn new_from_client<Client, Block>(client: Arc<Client>) -> sc_cli::Result<Self>
	where
		Block: BlockT<Hash = C::Hash>,
		Client: UsageProvider<Block> + ProvideRuntimeApi<Block>,
		Client::Api: Metadata<Block> + Core<Block>,
	{
		let genesis = client.usage_info().chain.best_hash;
		let api = client.runtime_api();
		let mut supported_metadata_versions = api.metadata_versions(genesis).unwrap();
		let latest = supported_metadata_versions
			.pop()
			.ok_or("No runtime version supported".to_string())?;
		let version = api.version(genesis).unwrap();
		let runtime_version = SubxtRuntimeVersion {
			spec_version: version.spec_version,
			transaction_version: version.transaction_version,
		};
		let metadata = api
			.metadata_at_version(genesis, latest)
			.map_err(|e| format!("Unable to fetch metadata: {:?}", e))?
			.ok_or("Unable to decode metadata".to_string())?;
		let metadata = subxt::Metadata::decode(&mut (*metadata).as_slice())?;

		Ok(Self { offline_client: OfflineClient::new(genesis, runtime_version, metadata) })
	}
}

impl<C: Config> DynamicRemarkBuilder<C> {
	pub fn new(
		metadata: subxt::Metadata,
		genesis_hash: C::Hash,
		runtime_version: SubxtRuntimeVersion,
	) -> Self {
		Self { offline_client: OfflineClient::new(genesis_hash, runtime_version, metadata) }
	}
}

impl ExtrinsicBuilder for DynamicRemarkBuilder<SubstrateConfig> {
	fn pallet(&self) -> &str {
		"system"
	}

	fn extrinsic(&self) -> &str {
		"remark"
	}

	fn build(&self, nonce: u32) -> std::result::Result<OpaqueExtrinsic, &'static str> {
		let signer = subxt_signer::sr25519::dev::alice();
		let dynamic_tx = subxt::dynamic::tx("System", "remark", vec![Vec::<u8>::new()]);

		let params = SubstrateExtrinsicParamsBuilder::new().nonce(nonce.into()).build();

		// Default transaction parameters assume a nonce of 0.
		let transaction = self
			.offline_client
			.tx()
			.create_signed_offline(&dynamic_tx, &signer, params)
			.unwrap();
		let mut encoded = transaction.into_encoded();

		OpaqueExtrinsic::from_bytes(&mut encoded).map_err(|_| "Unable to construct OpaqueExtrinsic")
	}
}

struct BasicCodeFetcher<'a>(Cow<'a, [u8]>);
impl<'a> FetchRuntimeCode for BasicCodeFetcher<'a> {
	fn fetch_runtime_code(&self) -> Option<Cow<'a, [u8]>> {
		Some(self.0.clone())
	}
}
impl<'a> BasicCodeFetcher<'a> {
	pub fn runtime_code(&'a self) -> RuntimeCode<'a> {
		RuntimeCode {
			code_fetcher: self as &'a dyn FetchRuntimeCode,
			heap_pages: None,
			hash: sp_crypto_hashing::blake2_256(&self.0).to_vec(),
		}
	}
}

pub fn fetch_relevant_runtime_data<HF: HostFunctions>(
	code_bytes: &Vec<u8>,
) -> Result<(SubxtRuntimeVersion, subxt::Metadata), String> {
	let executor = WasmExecutor::<HF>::builder().with_allow_missing_host_functions(true).build();
	let version = fetch_version(&executor, code_bytes).unwrap();
	let opaque_metadata = fetch_latest_metadata_from_blob(&executor, code_bytes).unwrap();
	let metadata = subxt::Metadata::decode(&mut (*opaque_metadata).as_slice()).unwrap();
	Ok((version, metadata))
}

pub fn fetch_version<HF: HostFunctions>(
	executor: &WasmExecutor<HF>,
	code_bytes: &Vec<u8>,
) -> sc_cli::Result<SubxtRuntimeVersion> {
	let mut ext = BasicExternalities::default();
	let fetcher = BasicCodeFetcher(code_bytes.into());
	let version_result = executor
		.call(&mut ext, &fetcher.runtime_code(), "Core_version", &[], CallContext::Offchain)
		.0
		.map_err(|e| format!("Unable to fetch version from blob: {e}"))?;
	let version = RuntimeVersion::decode(&mut version_result.as_slice())?;
	Ok(SubxtRuntimeVersion {
		spec_version: version.spec_version,
		transaction_version: version.transaction_version,
	})
}

pub fn fetch_latest_metadata_from_blob<HF: HostFunctions>(
	executor: &WasmExecutor<HF>,
	code_bytes: &Vec<u8>,
) -> sc_cli::Result<OpaqueMetadata> {
	let mut ext = BasicExternalities::default();
	let fetcher = BasicCodeFetcher(code_bytes.into());
	let version_result = executor
		.call(
			&mut ext,
			&fetcher.runtime_code(),
			"Metadata_metadata_versions",
			&[],
			CallContext::Offchain,
		)
		.0;

	let opaque_metadata: Option<OpaqueMetadata> = match version_result {
		Ok(supported_versions) => {
			let versions = Vec::<u32>::decode(&mut supported_versions.as_slice())
				.map_err(|e| format!("Error {e}"))?;
			let version_to_use = versions.last().ok_or("No versions available.")?;
			let parameters = (*version_to_use).encode();
			let encoded = executor
				.call(
					&mut ext,
					&fetcher.runtime_code(),
					"Metadata_metadata_at_version",
					&parameters,
					CallContext::Offchain,
				)
				.0
				.map_err(|e| format!("Unable to fetch metadata from blob: {e}"))?;
			Decode::decode(&mut encoded.as_slice())?
		},
		Err(_) => {
			let encoded = executor
				.call(
					&mut ext,
					&fetcher.runtime_code(),
					"Metadata_metadata",
					&[],
					CallContext::Offchain,
				)
				.0
				.map_err(|e| format!("Unable to fetch metadata from blob: {e}"))?;
			Decode::decode(&mut encoded.as_slice())?
		},
	};

	opaque_metadata.ok_or_else(|| "Metadata not found".into())
}
