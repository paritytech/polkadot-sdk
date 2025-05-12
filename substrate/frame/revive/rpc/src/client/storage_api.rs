use crate::{
	subxt_client::{self, runtime_types::pallet_revive::storage::ContractInfo, SrcChainConfig},
	ClientError, H160,
};
use subxt::{storage::Storage, OnlineClient};

/// A wrapper around the Substrate Storage API.
#[derive(Clone)]
pub struct StorageApi(Storage<SrcChainConfig, OnlineClient<SrcChainConfig>>);

impl StorageApi {
	/// Create a new instance of the StorageApi.
	pub fn new(api: Storage<SrcChainConfig, OnlineClient<SrcChainConfig>>) -> Self {
		Self(api)
	}

	/// Get the contract info for the given contract address.
	pub async fn get_contract_info(
		&self,
		contract_address: &H160,
	) -> Result<ContractInfo, ClientError> {
		// TODO: remove once subxt is updated
		let contract_address: subxt::utils::H160 = contract_address.0.into();

		let query = subxt_client::storage().revive().contract_info_of(contract_address);
		let Some(info) = self.0.fetch(&query).await? else {
			return Err(ClientError::ContractNotFound);
		};

		Ok(info)
	}

	/// Get the contract code for the given contract address.
	pub async fn get_contract_code(
		&self,
		contract_address: &H160,
	) -> Result<Option<Vec<u8>>, ClientError> {
		let ContractInfo { code_hash, .. } = self.get_contract_info(contract_address).await?;
		let query = subxt_client::storage().revive().pristine_code(code_hash);
		let result = self.0.fetch(&query).await?.map(|v| v.0);
		Ok(result)
	}

	/// Get the contract trie id for the given contract address.
	pub async fn get_contract_trie_id(&self, address: &H160) -> Result<Vec<u8>, ClientError> {
		let ContractInfo { trie_id, .. } = self.get_contract_info(address).await?;
		Ok(trie_id.0)
	}
}
