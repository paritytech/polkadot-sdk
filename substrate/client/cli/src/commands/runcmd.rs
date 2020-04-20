// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use crate::error::{Error, Result};
use crate::params::ImportParams;
use crate::params::KeystoreParams;
use crate::params::NetworkParams;
use crate::params::SharedParams;
use crate::params::TransactionPoolParams;
use crate::CliConfiguration;
use regex::Regex;
use sc_service::{
	config::{MultiaddrWithPeerId, PrometheusConfig, TransactionPoolOptions},
	ChainSpec, Role,
};
use sc_telemetry::TelemetryEndpoints;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use structopt::{clap::arg_enum, StructOpt};

arg_enum! {
	/// Whether off-chain workers are enabled.
	#[allow(missing_docs)]
	#[derive(Debug, Clone)]
	pub enum OffchainWorkerEnabled {
		Always,
		Never,
		WhenValidating,
	}
}

/// The `run` command used to run a node.
#[derive(Debug, StructOpt, Clone)]
pub struct RunCmd {
	/// Enable validator mode.
	///
	/// The node will be started with the authority role and actively
	/// participate in any consensus task that it can (e.g. depending on
	/// availability of local keys).
	#[structopt(
		long = "validator",
		conflicts_with_all = &[ "sentry" ]
	)]
	pub validator: bool,

	/// Enable sentry mode.
	///
	/// The node will be started with the authority role and participate in
	/// consensus tasks as an "observer", it will never actively participate
	/// regardless of whether it could (e.g. keys are available locally). This
	/// mode is useful as a secure proxy for validators (which would run
	/// detached from the network), since we want this node to participate in
	/// the full consensus protocols in order to have all needed consensus data
	/// available to relay to private nodes.
	#[structopt(
		long = "sentry",
		conflicts_with_all = &[ "validator", "light" ],
		parse(try_from_str)
	)]
	pub sentry: Vec<MultiaddrWithPeerId>,

	/// Disable GRANDPA voter when running in validator mode, otherwise disable the GRANDPA observer.
	#[structopt(long = "no-grandpa")]
	pub no_grandpa: bool,

	/// Experimental: Run in light client mode.
	#[structopt(long = "light", conflicts_with = "sentry")]
	pub light: bool,

	/// Listen to all RPC interfaces.
	///
	/// Default is local. Note: not all RPC methods are safe to be exposed publicly. Use an RPC proxy
	/// server to filter out dangerous methods. More details: https://github.com/paritytech/substrate/wiki/Public-RPC.
	/// Use `--unsafe-rpc-external` to suppress the warning if you understand the risks.
	#[structopt(long = "rpc-external")]
	pub rpc_external: bool,

	/// Listen to all RPC interfaces.
	///
	/// Same as `--rpc-external`.
	#[structopt(long = "unsafe-rpc-external")]
	pub unsafe_rpc_external: bool,

	/// Don't deny potentially unsafe RPCs when listening on external interfaces.
	///
	/// Default is false. This allows exposing RPC methods publicly (same as `--unsafe-{rpc,ws}-external` )
	/// but will allow doing so even on validator nodes, which is prohibited by default.
	/// Please do this if you know what you're doing.
	#[structopt(long = "unsafe-rpc-expose")]
	pub unsafe_rpc_expose: bool,

	/// Listen to all Websocket interfaces.
	///
	/// Default is local. Note: not all RPC methods are safe to be exposed publicly. Use an RPC proxy
	/// server to filter out dangerous methods. More details: https://github.com/paritytech/substrate/wiki/Public-RPC.
	/// Use `--unsafe-ws-external` to suppress the warning if you understand the risks.
	#[structopt(long = "ws-external")]
	pub ws_external: bool,

	/// Listen to all Websocket interfaces.
	///
	/// Same as `--ws-external` but doesn't warn you about it.
	#[structopt(long = "unsafe-ws-external")]
	pub unsafe_ws_external: bool,

	/// Listen to all Prometheus data source interfaces.
	///
	/// Default is local.
	#[structopt(long = "prometheus-external")]
	pub prometheus_external: bool,

	/// Specify HTTP RPC server TCP port.
	#[structopt(long = "rpc-port", value_name = "PORT")]
	pub rpc_port: Option<u16>,

	/// Specify WebSockets RPC server TCP port.
	#[structopt(long = "ws-port", value_name = "PORT")]
	pub ws_port: Option<u16>,

	/// Maximum number of WS RPC server connections.
	#[structopt(long = "ws-max-connections", value_name = "COUNT")]
	pub ws_max_connections: Option<usize>,

	/// Specify browser Origins allowed to access the HTTP & WS RPC servers.
	///
	/// A comma-separated list of origins (protocol://domain or special `null`
	/// value). Value of `all` will disable origin validation. Default is to
	/// allow localhost and https://polkadot.js.org origins. When running in
	/// --dev mode the default is to allow all origins.
	#[structopt(long = "rpc-cors", value_name = "ORIGINS", parse(try_from_str = parse_cors))]
	pub rpc_cors: Option<Cors>,

	/// Specify Prometheus data source server TCP Port.
	#[structopt(long = "prometheus-port", value_name = "PORT")]
	pub prometheus_port: Option<u16>,

	/// Do not expose a Prometheus metric endpoint.
	///
	/// Prometheus metric endpoint is enabled by default.
	#[structopt(long = "no-prometheus")]
	pub no_prometheus: bool,

	/// The human-readable name for this node.
	///
	/// The node name will be reported to the telemetry server, if enabled.
	#[structopt(long = "name", value_name = "NAME")]
	pub name: Option<String>,

	/// Disable connecting to the Substrate telemetry server.
	///
	/// Telemetry is on by default on global chains.
	#[structopt(long = "no-telemetry")]
	pub no_telemetry: bool,

	/// The URL of the telemetry server to connect to.
	///
	/// This flag can be passed multiple times as a means to specify multiple
	/// telemetry endpoints. Verbosity levels range from 0-9, with 0 denoting
	/// the least verbosity.
	/// Expected format is 'URL VERBOSITY', e.g. `--telemetry-url 'wss://foo/bar 0'`.
	#[structopt(long = "telemetry-url", value_name = "URL VERBOSITY", parse(try_from_str = parse_telemetry_endpoints))]
	pub telemetry_endpoints: Vec<(String, u8)>,

	/// Should execute offchain workers on every block.
	///
	/// By default it's only enabled for nodes that are authoring new blocks.
	#[structopt(
		long = "offchain-worker",
		value_name = "ENABLED",
		possible_values = &OffchainWorkerEnabled::variants(),
		case_insensitive = true,
		default_value = "WhenValidating"
	)]
	pub offchain_worker: OffchainWorkerEnabled,

	#[allow(missing_docs)]
	#[structopt(flatten)]
	pub shared_params: SharedParams,

	#[allow(missing_docs)]
	#[structopt(flatten)]
	pub import_params: ImportParams,

	#[allow(missing_docs)]
	#[structopt(flatten)]
	pub network_params: NetworkParams,

	#[allow(missing_docs)]
	#[structopt(flatten)]
	pub pool_config: TransactionPoolParams,

	/// Shortcut for `--name Alice --validator` with session keys for `Alice` added to keystore.
	#[structopt(long, conflicts_with_all = &["bob", "charlie", "dave", "eve", "ferdie", "one", "two"])]
	pub alice: bool,

	/// Shortcut for `--name Bob --validator` with session keys for `Bob` added to keystore.
	#[structopt(long, conflicts_with_all = &["alice", "charlie", "dave", "eve", "ferdie", "one", "two"])]
	pub bob: bool,

	/// Shortcut for `--name Charlie --validator` with session keys for `Charlie` added to keystore.
	#[structopt(long, conflicts_with_all = &["alice", "bob", "dave", "eve", "ferdie", "one", "two"])]
	pub charlie: bool,

	/// Shortcut for `--name Dave --validator` with session keys for `Dave` added to keystore.
	#[structopt(long, conflicts_with_all = &["alice", "bob", "charlie", "eve", "ferdie", "one", "two"])]
	pub dave: bool,

	/// Shortcut for `--name Eve --validator` with session keys for `Eve` added to keystore.
	#[structopt(long, conflicts_with_all = &["alice", "bob", "charlie", "dave", "ferdie", "one", "two"])]
	pub eve: bool,

	/// Shortcut for `--name Ferdie --validator` with session keys for `Ferdie` added to keystore.
	#[structopt(long, conflicts_with_all = &["alice", "bob", "charlie", "dave", "eve", "one", "two"])]
	pub ferdie: bool,

	/// Shortcut for `--name One --validator` with session keys for `One` added to keystore.
	#[structopt(long, conflicts_with_all = &["alice", "bob", "charlie", "dave", "eve", "ferdie", "two"])]
	pub one: bool,

	/// Shortcut for `--name Two --validator` with session keys for `Two` added to keystore.
	#[structopt(long, conflicts_with_all = &["alice", "bob", "charlie", "dave", "eve", "ferdie", "one"])]
	pub two: bool,

	/// Enable authoring even when offline.
	#[structopt(long = "force-authoring")]
	pub force_authoring: bool,

	#[allow(missing_docs)]
	#[structopt(flatten)]
	pub keystore_params: KeystoreParams,

	/// The size of the instances cache for each runtime.
	///
	/// The default value is 8 and the values higher than 256 are ignored.
	#[structopt(long)]
	pub max_runtime_instances: Option<usize>,

	/// Specify a list of sentry node public addresses.
	#[structopt(
		long = "sentry-nodes",
		value_name = "ADDR",
		conflicts_with_all = &[ "sentry" ]
	)]
	pub sentry_nodes: Vec<MultiaddrWithPeerId>,
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
			));
		})?;

		Ok(name)
	}

	fn dev_key_seed(&self, is_dev: bool) -> Result<Option<String>> {
		Ok(self.get_keyring().map(|a| format!("//{}", a)).or_else(|| {
			if is_dev && !self.light {
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
		Ok(if self.no_telemetry {
			None
		} else if !self.telemetry_endpoints.is_empty() {
			Some(
				TelemetryEndpoints::new(self.telemetry_endpoints.clone())
					.map_err(|e| e.to_string())?,
			)
		} else {
			chain_spec.telemetry_endpoints().clone()
		})
	}

	fn role(&self, is_dev: bool) -> Result<Role> {
		let keyring = self.get_keyring();
		let is_light = self.light;
		let is_authority = (self.validator || is_dev || keyring.is_some()) && !is_light;

		Ok(if is_light {
			sc_service::Role::Light
		} else if is_authority {
			sc_service::Role::Authority {
				sentry_nodes: self.sentry_nodes.clone(),
			}
		} else if !self.sentry.is_empty() {
			sc_service::Role::Sentry {
				validators: self.sentry.clone(),
			}
		} else {
			sc_service::Role::Full
		})
	}

	fn force_authoring(&self) -> Result<bool> {
		// Imply forced authoring on --dev
		Ok(self.shared_params.dev || self.force_authoring)
	}

	fn prometheus_config(&self) -> Result<Option<PrometheusConfig>> {
		Ok(if self.no_prometheus {
			None
		} else {
			let interface = if self.prometheus_external {
				Ipv4Addr::UNSPECIFIED
			} else {
				Ipv4Addr::LOCALHOST
			};

			Some(PrometheusConfig::new_with_default_registry(
				SocketAddr::new(interface.into(), self.prometheus_port.unwrap_or(9615))
			))
		})
	}

	fn disable_grandpa(&self) -> Result<bool> {
		Ok(self.no_grandpa)
	}

	fn rpc_ws_max_connections(&self) -> Result<Option<usize>> {
		Ok(self.ws_max_connections)
	}

	fn rpc_cors(&self, is_dev: bool) -> Result<Option<Vec<String>>> {
		Ok(self
			.rpc_cors
			.clone()
			.unwrap_or_else(|| {
				if is_dev {
					log::warn!("Running in --dev mode, RPC CORS has been disabled.");
					Cors::All
				} else {
					Cors::List(vec![
						"http://localhost:*".into(),
						"http://127.0.0.1:*".into(),
						"https://localhost:*".into(),
						"https://127.0.0.1:*".into(),
						"https://polkadot.js.org".into(),
					])
				}
			})
			.into())
	}

	fn rpc_http(&self) -> Result<Option<SocketAddr>> {
		let interface = rpc_interface(
			self.rpc_external,
			self.unsafe_rpc_external,
			self.unsafe_rpc_expose,
			self.validator
		)?;

		Ok(Some(SocketAddr::new(interface, self.rpc_port.unwrap_or(9933))))
	}

	fn rpc_ws(&self) -> Result<Option<SocketAddr>> {
		let interface = rpc_interface(
			self.ws_external,
			self.unsafe_ws_external,
			self.unsafe_rpc_expose,
			self.validator
		)?;

		Ok(Some(SocketAddr::new(interface, self.ws_port.unwrap_or(9944))))
	}

	fn unsafe_rpc_expose(&self) -> Result<bool> {
		Ok(self.unsafe_rpc_expose)
	}

	fn offchain_worker(&self, role: &Role) -> Result<bool> {
		Ok(match (&self.offchain_worker, role) {
			(OffchainWorkerEnabled::WhenValidating, Role::Authority { .. }) => true,
			(OffchainWorkerEnabled::Always, _) => true,
			(OffchainWorkerEnabled::Never, _) => false,
			(OffchainWorkerEnabled::WhenValidating, _) => false,
		})
	}

	fn transaction_pool(&self) -> Result<TransactionPoolOptions> {
		Ok(self.pool_config.transaction_pool())
	}

	fn max_runtime_instances(&self) -> Result<Option<usize>> {
		Ok(self.max_runtime_instances.map(|x| x.min(256)))
	}
}

/// Check whether a node name is considered as valid.
pub fn is_node_name_valid(_name: &str) -> std::result::Result<(), &str> {
	let name = _name.to_string();
	if name.chars().count() >= crate::NODE_NAME_MAX_LENGTH {
		return Err("Node name too long");
	}

	let invalid_chars = r"[\\.@]";
	let re = Regex::new(invalid_chars).unwrap();
	if re.is_match(&name) {
		return Err("Node name should not contain invalid chars such as '.' and '@'");
	}

	let invalid_patterns = r"(https?:\\/+)?(www)+";
	let re = Regex::new(invalid_patterns).unwrap();
	if re.is_match(&name) {
		return Err("Node name should not contain urls");
	}

	Ok(())
}

fn rpc_interface(
	is_external: bool,
	is_unsafe_external: bool,
	is_unsafe_rpc_expose: bool,
	is_validator: bool,
) -> Result<IpAddr> {
	if is_external && is_validator && !is_unsafe_rpc_expose {
		return Err(Error::Input(
			"--rpc-external and --ws-external options shouldn't be \
		used if the node is running as a validator. Use `--unsafe-rpc-external` if you understand \
		the risks. See the options description for more information."
				.to_owned(),
		));
	}

	if is_external || is_unsafe_external {
		log::warn!(
			"It isn't safe to expose RPC publicly without a proxy server that filters \
		available set of RPC methods."
		);

		Ok(Ipv4Addr::UNSPECIFIED.into())
	} else {
		Ok(Ipv4Addr::LOCALHOST.into())
	}
}

#[derive(Debug)]
enum TelemetryParsingError {
	MissingVerbosity,
	VerbosityParsingError(std::num::ParseIntError),
}

impl std::error::Error for TelemetryParsingError {}

impl std::fmt::Display for TelemetryParsingError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match &*self {
			TelemetryParsingError::MissingVerbosity => write!(f, "Verbosity level missing"),
			TelemetryParsingError::VerbosityParsingError(e) => write!(f, "{}", e),
		}
	}
}

fn parse_telemetry_endpoints(s: &str) -> std::result::Result<(String, u8), TelemetryParsingError> {
	let pos = s.find(' ');
	match pos {
		None => Err(TelemetryParsingError::MissingVerbosity),
		Some(pos_) => {
			let url = s[..pos_].to_string();
			let verbosity = s[pos_ + 1..]
				.parse()
				.map_err(TelemetryParsingError::VerbosityParsingError)?;
			Ok((url, verbosity))
		}
	}
}

/// CORS setting
///
/// The type is introduced to overcome `Option<Option<T>>`
/// handling of `structopt`.
#[derive(Clone, Debug)]
pub enum Cors {
	/// All hosts allowed.
	All,
	/// Only hosts on the list are allowed.
	List(Vec<String>),
}

impl From<Cors> for Option<Vec<String>> {
	fn from(cors: Cors) -> Self {
		match cors {
			Cors::All => None,
			Cors::List(list) => Some(list),
		}
	}
}

/// Parse cors origins.
fn parse_cors(s: &str) -> std::result::Result<Cors, Box<dyn std::error::Error>> {
	let mut is_all = false;
	let mut origins = Vec::new();
	for part in s.split(',') {
		match part {
			"all" | "*" => {
				is_all = true;
				break;
			}
			other => origins.push(other.to_owned()),
		}
	}

	Ok(if is_all {
		Cors::All
	} else {
		Cors::List(origins)
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tests_node_name_good() {
		assert!(is_node_name_valid("short name").is_ok());
	}

	#[test]
	fn tests_node_name_bad() {
		assert!(is_node_name_valid("long names are not very cool for the ui").is_err());
		assert!(is_node_name_valid("Dots.not.Ok").is_err());
		assert!(is_node_name_valid("http://visit.me").is_err());
		assert!(is_node_name_valid("https://visit.me").is_err());
		assert!(is_node_name_valid("www.visit.me").is_err());
		assert!(is_node_name_valid("email@domain").is_err());
	}
}
