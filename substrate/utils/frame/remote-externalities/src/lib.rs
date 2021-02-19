// This file is part of Substrate.

// Copyright (C) 2020-2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Remote Externalities
//!
//! An equivalent of `sp_io::TestExternalities` that can load its state from a remote substrate
//! based chain, or a local cache file.
//!
//! #### Runtime to Test Against
//!
//! While not absolutely necessary, you most likely need a `Runtime` equivalent in your test setup
//! through which you can infer storage types. There are two options here:
//!
//! 1. Build a mock runtime, similar how to you would build one in a pallet test (see example
//!    below). The very important point here is that this mock needs to hold real values for types
//!    that matter for you, based on the chain of interest. Some typical ones are:
//!
//! - `sp_runtime::AccountId32` as `AccountId`.
//! - `u32` as `BlockNumber`.
//! - `u128` as Balance.
//!
//! Once you have your `Runtime`, you can use it for storage type resolution and do things like
//! `<my_pallet::Pallet<Runtime>>::storage_getter()` or `<my_pallet::StorageItem<Runtime>>::get()`.
//!
//! 2. Or, you can use a real runtime.
//!
//! ### Example
//!
//! With a test runtime
//!
//! ```ignore
//! use remote_externalities::Builder;
//!
//! #[derive(Clone, Eq, PartialEq, Debug, Default)]
//! pub struct TestRuntime;
//!
//! use frame_system as system;
//! impl_outer_origin! {
//!     pub enum Origin for TestRuntime {}
//! }
//!
//! impl frame_system::Config for TestRuntime {
//!     ..
//!     // we only care about these two for now. The rest can be mock. The block number type of
//!     // kusama is u32.
//!     type BlockNumber = u32;
//!     type Header = Header;
//!     ..
//! }
//!
//! #[test]
//! fn test_runtime_works() {
//!     let hash: Hash =
//!         hex!["f9a4ce984129569f63edc01b1c13374779f9384f1befd39931ffdcc83acf63a7"].into();
//!     let parent: Hash =
//!         hex!["540922e96a8fcaf945ed23c6f09c3e189bd88504ec945cc2171deaebeaf2f37e"].into();
//!     Builder::new()
//!         .at(hash)
//!         .module("System")
//!         .build()
//!         .execute_with(|| {
//!             assert_eq!(
//!                 // note: the hash corresponds to 3098546. We can check only the parent.
//!                 // https://polkascan.io/kusama/block/3098546
//!                 <frame_system::Module<Runtime>>::block_hash(3098545u32),
//!                 parent,
//!             )
//!         });
//! }
//! ```
//!
//! Or with the real kusama runtime.
//!
//! ```ignore
//! use remote_externalities::Builder;
//! use kusama_runtime::Runtime;
//!
//! #[test]
//! fn test_runtime_works() {
//!     let hash: Hash =
//!         hex!["f9a4ce984129569f63edc01b1c13374779f9384f1befd39931ffdcc83acf63a7"].into();
//!     Builder::new()
//!         .at(hash)
//!         .module("Staking")
//!         .build()
//!         .execute_with(|| assert_eq!(<pallet_staking::Module<Runtime>>::validator_count(), 400));
//! }
//! ```

use std::{
	fs,
	path::{Path, PathBuf},
};
use log::*;
use sp_core::{hashing::twox_128};
pub use sp_io::TestExternalities;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{StorageKey, StorageData},
};
use futures::future::Future;

type KeyPair = (StorageKey, StorageData);
type Number = u32;
type Hash = sp_core::H256;
// TODO: make these two generic.

const LOG_TARGET: &'static str = "remote-ext";

/// The execution mode.
#[derive(Clone)]
pub enum Mode {
	/// Online.
	Online(OnlineConfig),
	/// Offline. Uses a cached file and needs not any client config.
	Offline(OfflineConfig),
}

/// configuration of the online execution.
///
/// A cache config must be present.
#[derive(Clone)]
pub struct OfflineConfig {
	/// The configuration of the cache file to use. It must be present.
	pub cache: CacheConfig,
}

/// Configuration of the online execution.
///
/// A cache config may be present and will be written to in that case.
#[derive(Clone)]
pub struct OnlineConfig {
	/// The HTTP uri to use.
	pub uri: String,
	/// The block number at which to connect. Will be latest finalized head if not provided.
	pub at: Option<Hash>,
	/// An optional cache file to WRITE to, not for reading. Not cached if set to `None`.
	pub cache: Option<CacheConfig>,
	/// The modules to scrape. If empty, entire chain state will be scraped.
	pub modules: Vec<String>,
}

impl Default for OnlineConfig {
	fn default() -> Self {
		Self {
			uri: "http://localhost:9933".into(),
			at: None,
			cache: None,
			modules: Default::default(),
		}
	}
}

/// Configuration of the cache.
#[derive(Clone)]
pub struct CacheConfig {
	// TODO: I could mix these two into one filed, but I think separate is better bc one can be
	// configurable while one not.
	/// File name.
	pub name: String,
	/// Base directory.
	pub directory: String,
}

impl Default for CacheConfig {
	fn default() -> Self {
		Self { name: "CACHE".into(), directory: ".".into() }
	}
}

impl CacheConfig {
	fn path(&self) -> PathBuf {
		Path::new(&self.directory).join(self.name.clone())
	}
}

/// Builder for remote-externalities.
pub struct Builder {
	inject: Vec<KeyPair>,
	mode: Mode,
	chain: String,
}

impl Default for Builder {
	fn default() -> Self {
		Self {
			inject: Default::default(),
			mode: Mode::Online(OnlineConfig {
				at: None,
				uri: "http://localhost:9933".into(),
				cache: None,
				modules: Default::default(),
			}),
			chain: "UNSET".into(),
		}
	}
}

// Mode methods
impl Builder {
	fn as_online(&self) -> &OnlineConfig {
		match &self.mode {
			Mode::Online(config) => &config,
			_ => panic!("Unexpected mode: Online"),
		}
	}

	fn as_online_mut(&mut self) -> &mut OnlineConfig {
		match &mut self.mode {
			Mode::Online(config) => config,
			_ => panic!("Unexpected mode: Online"),
		}
	}
}

// RPC methods
impl Builder {
	async fn rpc_get_head(&self) -> Hash {
		let mut rt = tokio::runtime::Runtime::new().expect("Unable to create a runtime");
		let uri = self.as_online().uri.clone();
		rt.block_on::<_, _, ()>(futures::lazy(move || {
			trace!(target: LOG_TARGET, "rpc: finalized_head");
			let client: sc_rpc_api::chain::ChainClient<Number, Hash, (), ()> =
				jsonrpc_core_client::transports::http::connect(&uri).wait().unwrap();
			Ok(client.finalized_head().wait().unwrap())
		}))
		.unwrap()
	}

	/// Relay the request to `state_getPairs` rpc endpoint.
	///
	/// Note that this is an unsafe RPC.
	async fn rpc_get_pairs(&self, prefix: StorageKey, at: Hash) -> Vec<KeyPair> {
		let mut rt = tokio::runtime::Runtime::new().expect("Unable to create a runtime");
		let uri = self.as_online().uri.clone();
		rt.block_on::<_, _, ()>(futures::lazy(move || {
			trace!(target: LOG_TARGET, "rpc: storage_pairs: {:?} / {:?}", prefix, at);
			let client: sc_rpc_api::state::StateClient<Hash> =
				jsonrpc_core_client::transports::http::connect(&uri).wait().unwrap();
			Ok(client.storage_pairs(prefix, Some(at)).wait().unwrap())
		}))
		.unwrap()
	}

	/// Get the chain name.
	async fn chain_name(&self) -> String {
		let mut rt = tokio::runtime::Runtime::new().expect("Unable to create a runtime");
		let uri = self.as_online().uri.clone();
		rt.block_on::<_, _, ()>(futures::lazy(move || {
			trace!(target: LOG_TARGET, "rpc: system_chain");
			let client: sc_rpc_api::system::SystemClient<(), ()> =
				jsonrpc_core_client::transports::http::connect(&uri).wait().unwrap();
			Ok(client.system_chain().wait().unwrap())
		}))
		.unwrap()
	}
}

// Internal methods
impl Builder {
	/// Save the given data as cache.
	fn save_cache(&self, data: &[KeyPair], path: &Path) {
		let bdata = bincode::serialize(data).unwrap();
		info!(target: LOG_TARGET, "writing to cache file {:?}", path);
		fs::write(path, bdata).unwrap();
	}

	/// initialize `Self` from cache. Panics if the file does not exist.
	fn load_cache(&self, path: &Path) -> Vec<KeyPair> {
		info!(target: LOG_TARGET, "scraping keypairs from cache {:?}", path,);
		let bytes = fs::read(path).unwrap();
		bincode::deserialize(&bytes[..]).unwrap()
	}

	/// Build `Self` from a network node denoted by `uri`.
	async fn load_remote(&self) -> Vec<KeyPair> {
		let config = self.as_online();
		let at = self.as_online().at.unwrap().clone();
		info!(target: LOG_TARGET, "scraping keypairs from remote node {} @ {:?}", config.uri, at);

		let keys_and_values = if config.modules.len() > 0 {
			let mut filtered_kv = vec![];
			for f in config.modules.iter() {
				let hashed_prefix = StorageKey(twox_128(f.as_bytes()).to_vec());
				let module_kv = self.rpc_get_pairs(hashed_prefix.clone(), at).await;
				info!(
					target: LOG_TARGET,
					"downloaded data for module {} (count: {} / prefix: {:?}).",
					f,
					module_kv.len(),
					HexDisplay::from(&hashed_prefix),
				);
				filtered_kv.extend(module_kv);
			}
			filtered_kv
		} else {
			info!(target: LOG_TARGET, "downloading data for all modules.");
			self.rpc_get_pairs(StorageKey(vec![]), at).await.into_iter().collect::<Vec<_>>()
		};

		keys_and_values
	}

	async fn init_remote_client(&mut self) {
		self.as_online_mut().at = Some(self.rpc_get_head().await);
		self.chain = self.chain_name().await;
	}

	async fn pre_build(mut self) -> Vec<KeyPair> {
		let mut base_kv = match self.mode.clone() {
			Mode::Offline(config) => self.load_cache(&config.cache.path()),
			Mode::Online(config) => {
				self.init_remote_client().await;
				let kp = self.load_remote().await;
				if let Some(c) = config.cache {
					self.save_cache(&kp, &c.path());
				}
				kp
			}
		};

		info!(
			target: LOG_TARGET,
			"extending externalities with {} manually injected keys",
			self.inject.len()
		);
		base_kv.extend(self.inject.clone());
		base_kv
	}
}

// Public methods
impl Builder {
	/// Create a new builder.
	pub fn new() -> Self {
		Default::default()
	}

	/// Inject a manual list of key and values to the storage.
	pub fn inject(mut self, injections: &[KeyPair]) -> Self {
		for i in injections {
			self.inject.push(i.clone());
		}
		self
	}

	/// Configure a cache to be used.
	pub fn mode(mut self, mode: Mode) -> Self {
		self.mode = mode;
		self
	}

	/// Build the test externalities.
	pub async fn build(self) -> TestExternalities {
		let kv = self.pre_build().await;
		let mut ext = TestExternalities::new_empty();

		info!(target: LOG_TARGET, "injecting a total of {} keys", kv.len());
		for (k, v) in kv {
			let (k, v) = (k.0, v.0);
			ext.insert(k, v);
		}
		ext
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn init_logger() {
		let _ = env_logger::Builder::from_default_env()
			.format_module_path(false)
			.format_level(true)
			.try_init();
	}

	#[async_std::test]
	#[cfg(feature = "remote-test")]
	async fn can_build_one_pallet() {
		init_logger();
		Builder::new()
			.mode(Mode::Online(OnlineConfig {
				modules: vec!["Proxy".into()],
				..Default::default()
			}))
			.build()
			.await
			.execute_with(|| {});
	}

	#[async_std::test]
	async fn can_load_cache() {
		init_logger();
		Builder::new()
			.mode(Mode::Offline(OfflineConfig {
				cache: CacheConfig { name: "proxy_test".into(), ..Default::default() },
			}))
			.build()
			.await
			.execute_with(|| {});
	}

	#[async_std::test]
	#[cfg(feature = "remote-test")]
	async fn can_create_cache() {
		init_logger();
		Builder::new()
			.mode(Mode::Online(OnlineConfig {
				cache: Some(CacheConfig {
					name: "test_cache_to_remove.bin".into(),
					..Default::default()
				}),
				..Default::default()
			}))
			.build()
			.await
			.execute_with(|| {});

		let to_delete = std::fs::read_dir(CacheConfig::default().directory)
			.unwrap()
			.into_iter()
			.map(|d| d.unwrap())
			.filter(|p| p.path().extension().unwrap_or_default() == "bin")
			.collect::<Vec<_>>();

		assert!(to_delete.len() > 0);

		for d in to_delete {
			std::fs::remove_file(d.path()).unwrap();
		}
	}

	#[async_std::test]
	#[cfg(feature = "remote-test")]
	async fn can_build_all() {
		init_logger();
		Builder::new().build().await.execute_with(|| {});
	}
}
