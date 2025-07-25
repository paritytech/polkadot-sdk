// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
	error::{Error, Result},
	params::{
		ImportParams, KeystoreParams, NetworkParams, OffchainWorkerParams, RpcEndpoint,
		SharedParams, TransactionPoolParams,
	},
	CliConfiguration, PrometheusParams, RpcParams, RuntimeParams, TelemetryParams,
};
use clap::Parser;
use regex::Regex;
use sc_service::{
	config::{
		BasePath, IpNetwork, PrometheusConfig, RpcBatchRequestConfig, TransactionPoolOptions,
	},
	ChainSpec, Role,
};
use sc_telemetry::TelemetryEndpoints;
use std::num::NonZeroU32;

/// The `run` command used to run a node.
#[derive(Debug, Clone, Parser)]
pub struct RunCmd {
	/// Enable validator mode.
	///
	/// The node will be started with the authority role and actively
	/// participate in any consensus task that it can (e.g. depending on
	/// availability of local keys).
	#[arg(long)]
	pub validator: bool,

	/// Disable GRANDPA.
	///
	/// Disables voter when running in validator mode, otherwise disable the GRANDPA
	/// observer.
	#[arg(long)]
	pub no_grandpa: bool,

	/// The human-readable name for this node.
	///
	/// It's used as network node name.
	#[arg(long, value_name = "NAME")]
	pub name: Option<String>,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub rpc_params: RpcParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub telemetry_params: TelemetryParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub prometheus_params: PrometheusParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub runtime_params: RuntimeParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub offchain_worker_params: OffchainWorkerParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub import_params: ImportParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub network_params: NetworkParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub pool_config: TransactionPoolParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub keystore_params: KeystoreParams,

	/// Shortcut for `--name Alice --validator`.
	///
	/// Session keys for `Alice` are added to keystore.
	#[arg(long, conflicts_with_all = &["bob", "charlie", "dave", "eve", "ferdie", "one", "two"])]
	pub alice: bool,

	/// Shortcut for `--name Bob --validator`.
	///
	/// Session keys for `Bob` are added to keystore.
	#[arg(long, conflicts_with_all = &["alice", "charlie", "dave", "eve", "ferdie", "one", "two"])]
	pub bob: bool,

	/// Shortcut for `--name Charlie --validator`.
	///
	/// Session keys for `Charlie` are added to keystore.
	#[arg(long, conflicts_with_all = &["alice", "bob", "dave", "eve", "ferdie", "one", "two"])]
	pub charlie: bool,

	/// Shortcut for `--name Dave --validator`.
	///
	/// Session keys for `Dave` are added to keystore.
	#[arg(long, conflicts_with_all = &["alice", "bob", "charlie", "eve", "ferdie", "one", "two"])]
	pub dave: bool,

	/// Shortcut for `--name Eve --validator`.
	///
	/// Session keys for `Eve` are added to keystore.
	#[arg(long, conflicts_with_all = &["alice", "bob", "charlie", "dave", "ferdie", "one", "two"])]
	pub eve: bool,

	/// Shortcut for `--name Ferdie --validator`.
	///
	/// Session keys for `Ferdie` are added to keystore.
	#[arg(long, conflicts_with_all = &["alice", "bob", "charlie", "dave", "eve", "one", "two"])]
	pub ferdie: bool,

	/// Shortcut for `--name One --validator`.
	///
	/// Session keys for `One` are added to keystore.
	#[arg(long, conflicts_with_all = &["alice", "bob", "charlie", "dave", "eve", "ferdie", "two"])]
	pub one: bool,

	/// Shortcut for `--name Two --validator`.
	///
	/// Session keys for `Two` are added to keystore.
	#[arg(long, conflicts_with_all = &["alice", "bob", "charlie", "dave", "eve", "ferdie", "one"])]
	pub two: bool,

	/// Enable authoring even when offline.
	#[arg(long)]
	pub force_authoring: bool,

	/// Run a temporary node.
	///
	/// A temporary directory will be created to store the configuration and will be deleted
	/// at the end of the process.
	///
	/// Note: the directory is random per process execution. This directory is used as base path
	/// which includes: database, node key and keystore.
	///
	/// When `--dev` is given and no explicit `--base-path`, this option is implied.
	#[arg(long, conflicts_with = "base_path")]
	pub tmp: bool,
}

impl RunCmd {
	/// Get the `Sr25519Keyring` matching one of the flag.
	pub fn get_keyring(&self) -> Option<sp_keyring::Sr25519Keyring> {
		use sp_keyring::Sr25519Keyring::*;

		if self.alice {
			Some(Alice)
		} else if self.bob {
			Some(Bob)
		} else if self.charlie {
			Some(Charlie)
		} else if self.dave {
			Some(Dave)
		} else if self.eve {
			Some(Eve)
		} else if self.ferdie {
			Some(Ferdie)
		} else if self.one {
			Some(One)
		} else if self.two {
			Some(Two)
		} else {
			None
		}
	}
}

impl CliConfiguration for RunCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}

	fn import_params(&self) -> Option<&ImportParams> {
		Some(&self.import_params)
	}

	fn network_params(&self) -> Option<&NetworkParams> {
		Some(&self.network_params)
	}

	fn keystore_params(&self) -> Option<&KeystoreParams> {
		Some(&self.keystore_params)
	}

	fn offchain_worker_params(&self) -> Option<&OffchainWorkerParams> {
		Some(&self.offchain_worker_params)
	}

	fn node_name(&self) -> Result<String> {
		let name: String = match (self.name.as_ref(), self.get_keyring()) {
			(Some(name), _) => name.to_string(),
			(_, Some(keyring)) => keyring.to_string(),
			(None, None) => crate::generate_node_name(),
		};

		is_node_name_valid(&name).map_err(|msg| {
			Error::Input(format!(
				"Invalid node name '{}'. Reason: {}. If unsure, use none.",
				name, msg
			))
		})?;

		Ok(name)
	}

	fn dev_key_seed(&self, is_dev: bool) -> Result<Option<String>> {
		Ok(self.get_keyring().map(|a| format!("//{}", a)).or_else(|| {
			if is_dev {
				Some("//Alice".into())
			} else {
				None
			}
		}))
	}

	fn telemetry_endpoints(
		&self,
		chain_spec: &Box<dyn ChainSpec>,
	) -> Result<Option<TelemetryEndpoints>> {
		let params = &self.telemetry_params;
		Ok(if params.no_telemetry {
			None
		} else if !params.telemetry_endpoints.is_empty() {
			Some(
				TelemetryEndpoints::new(params.telemetry_endpoints.clone())
					.map_err(|e| e.to_string())?,
			)
		} else {
			chain_spec.telemetry_endpoints().clone()
		})
	}

	fn role(&self, is_dev: bool) -> Result<Role> {
		let keyring = self.get_keyring();
		let is_authority = self.validator || is_dev || keyring.is_some();

		Ok(if is_authority { Role::Authority } else { Role::Full })
	}

	fn force_authoring(&self) -> Result<bool> {
		// Imply forced authoring on --dev
		Ok(self.shared_params.dev || self.force_authoring)
	}

	fn prometheus_config(
		&self,
		default_listen_port: u16,
		chain_spec: &Box<dyn ChainSpec>,
	) -> Result<Option<PrometheusConfig>> {
		Ok(self
			.prometheus_params
			.prometheus_config(default_listen_port, chain_spec.id().to_string()))
	}

	fn disable_grandpa(&self) -> Result<bool> {
		Ok(self.no_grandpa)
	}

	fn rpc_max_connections(&self) -> Result<u32> {
		Ok(self.rpc_params.rpc_max_connections)
	}

	fn rpc_cors(&self, is_dev: bool) -> Result<Option<Vec<String>>> {
		self.rpc_params.rpc_cors(is_dev)
	}

	fn rpc_addr(&self, default_listen_port: u16) -> Result<Option<Vec<RpcEndpoint>>> {
		self.rpc_params.rpc_addr(self.is_dev()?, self.validator, default_listen_port)
	}

	fn rpc_methods(&self) -> Result<sc_service::config::RpcMethods> {
		Ok(self.rpc_params.rpc_methods.into())
	}

	fn rpc_max_request_size(&self) -> Result<u32> {
		Ok(self.rpc_params.rpc_max_request_size)
	}

	fn rpc_max_response_size(&self) -> Result<u32> {
		Ok(self.rpc_params.rpc_max_response_size)
	}

	fn rpc_max_subscriptions_per_connection(&self) -> Result<u32> {
		Ok(self.rpc_params.rpc_max_subscriptions_per_connection)
	}

	fn rpc_buffer_capacity_per_connection(&self) -> Result<u32> {
		Ok(self.rpc_params.rpc_message_buffer_capacity_per_connection)
	}

	fn rpc_batch_config(&self) -> Result<RpcBatchRequestConfig> {
		self.rpc_params.rpc_batch_config()
	}

	fn rpc_rate_limit(&self) -> Result<Option<NonZeroU32>> {
		Ok(self.rpc_params.rpc_rate_limit)
	}

	fn rpc_rate_limit_whitelisted_ips(&self) -> Result<Vec<IpNetwork>> {
		Ok(self.rpc_params.rpc_rate_limit_whitelisted_ips.clone())
	}

	fn rpc_rate_limit_trust_proxy_headers(&self) -> Result<bool> {
		Ok(self.rpc_params.rpc_rate_limit_trust_proxy_headers)
	}

	fn transaction_pool(&self, is_dev: bool) -> Result<TransactionPoolOptions> {
		Ok(self.pool_config.transaction_pool(is_dev))
	}

	fn max_runtime_instances(&self) -> Result<Option<usize>> {
		Ok(Some(self.runtime_params.max_runtime_instances))
	}

	fn runtime_cache_size(&self) -> Result<u8> {
		Ok(self.runtime_params.runtime_cache_size)
	}

	fn base_path(&self) -> Result<Option<BasePath>> {
		Ok(if self.tmp {
			Some(BasePath::new_temp_dir()?)
		} else {
			match self.shared_params().base_path()? {
				Some(r) => Some(r),
				// If `dev` is enabled, we use the temp base path.
				None if self.shared_params().is_dev() => Some(BasePath::new_temp_dir()?),
				None => None,
			}
		})
	}
}

/// Check whether a node name is considered as valid.
pub fn is_node_name_valid(_name: &str) -> std::result::Result<(), &str> {
	let name = _name.to_string();

	if name.is_empty() {
		return Err("Node name cannot be empty");
	}

	if name.chars().count() >= crate::NODE_NAME_MAX_LENGTH {
		return Err("Node name too long");
	}

	let invalid_chars = r"[\\.@]";
	let re = Regex::new(invalid_chars).unwrap();
	if re.is_match(&name) {
		return Err("Node name should not contain invalid chars such as '.' and '@'");
	}

	let invalid_patterns = r"^https?:";
	let re = Regex::new(invalid_patterns).unwrap();
	if re.is_match(&name) {
		return Err("Node name should not contain urls");
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tests_node_name_good() {
		assert!(is_node_name_valid("short name").is_ok());
		assert!(is_node_name_valid("www").is_ok());
		assert!(is_node_name_valid("aawww").is_ok());
		assert!(is_node_name_valid("wwwaa").is_ok());
		assert!(is_node_name_valid("www aa").is_ok());
	}

	#[test]
	fn tests_node_name_bad() {
		assert!(is_node_name_valid("").is_err());
		assert!(is_node_name_valid(
			"very very long names are really not very cool for the ui at all, really they're not"
		)
		.is_err());
		assert!(is_node_name_valid("Dots.not.Ok").is_err());
		// NOTE: the urls below don't include a domain otherwise
		// they'd get filtered for including a `.`
		assert!(is_node_name_valid("http://visitme").is_err());
		assert!(is_node_name_valid("http:/visitme").is_err());
		assert!(is_node_name_valid("http:visitme").is_err());
		assert!(is_node_name_valid("https://visitme").is_err());
		assert!(is_node_name_valid("https:/visitme").is_err());
		assert!(is_node_name_valid("https:visitme").is_err());
		assert!(is_node_name_valid("www.visit.me").is_err());
		assert!(is_node_name_valid("www.visit").is_err());
		assert!(is_node_name_valid("hello\\world").is_err());
		assert!(is_node_name_valid("visit.www").is_err());
		assert!(is_node_name_valid("email@domain").is_err());
	}
}
