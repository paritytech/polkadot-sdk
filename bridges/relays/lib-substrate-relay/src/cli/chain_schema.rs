// Copyright 2019-2022 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives related to chain CLI options.

use relay_substrate_client::{AccountKeyPairOf, ChainWithTransactions};
use structopt::StructOpt;
use strum::{EnumString, VariantNames};

use relay_substrate_client::{ChainRuntimeVersion, ChainWithRuntimeVersion, SimpleRuntimeVersion};

use crate::TransactionParams;

#[doc = "Runtime version params."]
#[derive(StructOpt, Debug, PartialEq, Eq, Clone, Copy, EnumString, VariantNames)]
pub enum RuntimeVersionType {
	/// Auto query version from chain
	Auto,
	/// Custom `spec_version` and `transaction_version`
	Custom,
	/// Read version from bundle dependencies directly.
	Bundle,
}

/// Create chain-specific set of runtime version parameters.
#[macro_export]
macro_rules! declare_chain_runtime_version_params_cli_schema {
	($chain:ident, $chain_prefix:ident) => {
		bp_runtime::paste::item! {
			#[doc = $chain " runtime version params."]
			#[derive(StructOpt, Debug, PartialEq, Eq, Clone, Copy)]
			pub struct [<$chain RuntimeVersionParams>] {
				#[doc = "The type of runtime version for chain " $chain]
				#[structopt(long, default_value = "Bundle")]
				pub [<$chain_prefix _version_mode>]: RuntimeVersionType,
				#[doc = "The custom sepc_version for chain " $chain]
				#[structopt(long)]
				pub [<$chain_prefix _spec_version>]: Option<u32>,
				#[doc = "The custom transaction_version for chain " $chain]
				#[structopt(long)]
				pub [<$chain_prefix _transaction_version>]: Option<u32>,
			}

			impl [<$chain RuntimeVersionParams>] {
				/// Converts self into `ChainRuntimeVersion`.
				pub fn into_runtime_version(
					self,
					bundle_runtime_version: Option<SimpleRuntimeVersion>,
				) -> anyhow::Result<ChainRuntimeVersion> {
					Ok(match self.[<$chain_prefix _version_mode>] {
						RuntimeVersionType::Auto => ChainRuntimeVersion::Auto,
						RuntimeVersionType::Custom => {
							let custom_spec_version = self.[<$chain_prefix _spec_version>]
								.ok_or_else(|| anyhow::Error::msg(format!("The {}-spec-version is required when choose custom mode", stringify!($chain_prefix))))?;
							let custom_transaction_version = self.[<$chain_prefix _transaction_version>]
								.ok_or_else(|| anyhow::Error::msg(format!("The {}-transaction-version is required when choose custom mode", stringify!($chain_prefix))))?;
							ChainRuntimeVersion::Custom(
								SimpleRuntimeVersion {
									spec_version: custom_spec_version,
									transaction_version: custom_transaction_version
								}
							)
						},
						RuntimeVersionType::Bundle => match bundle_runtime_version {
							Some(runtime_version) => ChainRuntimeVersion::Custom(runtime_version),
							None => {
								return Err(anyhow::format_err!("Cannot use bundled runtime version of {}: it is not known to the relay", stringify!($chain_prefix)));
							}
						},
					})
				}
			}
		}
	};
}

/// Create chain-specific set of runtime version parameters.
#[macro_export]
macro_rules! declare_chain_connection_params_cli_schema {
	($chain:ident, $chain_prefix:ident) => {
		bp_runtime::paste::item! {
			// TODO: https://github.com/paritytech/parity-bridges-common/issues/2909
			// remove all obsolete arguments (separate URI components)

			#[doc = $chain " connection params."]
			#[derive(StructOpt, Debug, PartialEq, Eq, Clone)]
			pub struct [<$chain ConnectionParams>] {
				#[doc = "WS endpoint of " $chain ": full URI. Overrides all other connection string components (host, port, path, secure)."]
				#[structopt(long)]
				pub [<$chain_prefix _uri>]: Option<String>,
				#[doc = "WS endpoint of " $chain ": host component."]
				#[structopt(long, default_value = "127.0.0.1")]
				pub [<$chain_prefix _host>]: String,
				#[doc = "WS endpoint of " $chain ": port component."]
				#[structopt(long, default_value = "9944")]
				pub [<$chain_prefix _port>]: u16,
				#[doc = "WS endpoint of " $chain ": path component."]
				#[structopt(long)]
				pub [<$chain_prefix _path>]: Option<String>,
				#[doc = "Use secure websocket connection."]
				#[structopt(long)]
				pub [<$chain_prefix _secure>]: bool,
				#[doc = "Custom runtime version"]
				#[structopt(flatten)]
				pub [<$chain_prefix _runtime_version>]: [<$chain RuntimeVersionParams>],
			}

			impl [<$chain ConnectionParams>] {
				/// Convert connection params into Substrate client.
				#[allow(dead_code)]
				pub async fn into_client<Chain: ChainWithRuntimeVersion>(
					self,
				) -> anyhow::Result<relay_substrate_client::Client<Chain>> {
					let chain_runtime_version = self
						.[<$chain_prefix _runtime_version>]
						.into_runtime_version(Chain::RUNTIME_VERSION)?;
					Ok(relay_substrate_client::Client::new(relay_substrate_client::ConnectionParams {
						uri: self.[<$chain_prefix _uri>],
						host: self.[<$chain_prefix _host>],
						port: self.[<$chain_prefix _port>],
						path: self.[<$chain_prefix _path>],
						secure: self.[<$chain_prefix _secure>],
						chain_runtime_version,
					})
					.await
					)
				}
			}
		}
	};
}

/// Create chain-specific set of signing parameters.
#[macro_export]
macro_rules! declare_chain_signing_params_cli_schema {
	($chain:ident, $chain_prefix:ident) => {
		bp_runtime::paste::item! {
			#[doc = $chain " signing params."]
			#[derive(StructOpt, Debug, PartialEq, Eq, Clone)]
			pub struct [<$chain SigningParams>] {
				#[doc = "The SURI of secret key to use when transactions are submitted to the " $chain " node."]
				#[structopt(long)]
				pub [<$chain_prefix _signer>]: Option<String>,
				#[doc = "The password for the SURI of secret key to use when transactions are submitted to the " $chain " node."]
				#[structopt(long)]
				pub [<$chain_prefix _signer_password>]: Option<String>,

				#[doc = "Path to the file, that contains SURI of secret key to use when transactions are submitted to the " $chain " node. Can be overridden with " $chain_prefix "_signer option."]
				#[structopt(long)]
				pub [<$chain_prefix _signer_file>]: Option<std::path::PathBuf>,
				#[doc = "Path to the file, that password for the SURI of secret key to use when transactions are submitted to the " $chain " node. Can be overridden with " $chain_prefix "_signer_password option."]
				#[structopt(long)]
				pub [<$chain_prefix _signer_password_file>]: Option<std::path::PathBuf>,

				#[doc = "Transactions mortality period, in blocks. MUST be a power of two in [4; 65536] range. MAY NOT be larger than `BlockHashCount` parameter of the chain system module."]
				#[structopt(long)]
				pub [<$chain_prefix _transactions_mortality>]: Option<u32>,
			}

			impl [<$chain SigningParams>] {
				/// Return transactions mortality.
				#[allow(dead_code)]
				pub fn transactions_mortality(&self) -> anyhow::Result<Option<u32>> {
					self.[<$chain_prefix _transactions_mortality>]
						.map(|transactions_mortality| {
							if !(4..=65536).contains(&transactions_mortality)
								|| !transactions_mortality.is_power_of_two()
							{
								Err(anyhow::format_err!(
									"Transactions mortality {} is not a power of two in a [4; 65536] range",
									transactions_mortality,
								))
							} else {
								Ok(transactions_mortality)
							}
						})
						.transpose()
				}

				/// Parse signing params into chain-specific KeyPair.
				#[allow(dead_code)]
				pub fn to_keypair<Chain: ChainWithTransactions>(&self) -> anyhow::Result<AccountKeyPairOf<Chain>> {
					let suri = match (self.[<$chain_prefix _signer>].as_ref(), self.[<$chain_prefix _signer_file>].as_ref()) {
						(Some(suri), _) => suri.to_owned(),
						(None, Some(suri_file)) => std::fs::read_to_string(suri_file)
							.map_err(|err| anyhow::format_err!(
								"Failed to read SURI from file {:?}: {}",
								suri_file,
								err,
							))?,
						(None, None) => return Err(anyhow::format_err!(
							"One of options must be specified: '{}' or '{}'",
							stringify!([<$chain_prefix _signer>]),
							stringify!([<$chain_prefix _signer_file>]),
						)),
					};

					let suri_password = match (
						self.[<$chain_prefix _signer_password>].as_ref(),
						self.[<$chain_prefix _signer_password_file>].as_ref(),
					) {
						(Some(suri_password), _) => Some(suri_password.to_owned()),
						(None, Some(suri_password_file)) => std::fs::read_to_string(suri_password_file)
							.map(Some)
							.map_err(|err| anyhow::format_err!(
								"Failed to read SURI password from file {:?}: {}",
								suri_password_file,
								err,
							))?,
						_ => None,
					};

					use sp_core::crypto::Pair;

					AccountKeyPairOf::<Chain>::from_string(
						&suri,
						suri_password.as_deref()
					).map_err(|e| anyhow::format_err!("{:?}", e))
				}

				/// Return transaction parameters.
				#[allow(dead_code)]
				pub fn transaction_params<Chain: ChainWithTransactions>(
					&self,
				) -> anyhow::Result<TransactionParams<AccountKeyPairOf<Chain>>> {
					Ok(TransactionParams {
						mortality: self.transactions_mortality()?,
						signer: self.to_keypair::<Chain>()?,
					})
				}
			}
		}
	};
}

/// Create chain-specific set of configuration objects: connection parameters,
/// signing parameters and bridge initialization parameters.
#[macro_export]
macro_rules! declare_chain_cli_schema {
	($chain:ident, $chain_prefix:ident) => {
		$crate::declare_chain_runtime_version_params_cli_schema!($chain, $chain_prefix);
		$crate::declare_chain_connection_params_cli_schema!($chain, $chain_prefix);
		$crate::declare_chain_signing_params_cli_schema!($chain, $chain_prefix);
	};
}

declare_chain_cli_schema!(Source, source);
declare_chain_cli_schema!(Target, target);
