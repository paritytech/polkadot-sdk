// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! This variant of Malus overrides the `disabled_validators` runtime API
//! to always return an empty set of disabled validators.

use polkadot_cli::{
	service::{
		AuxStore, Error, ExtendedOverseerGenArgs, Overseer, OverseerConnector, OverseerGen,
		OverseerGenArgs, OverseerHandle,
	},
	validator_overseer_builder, Cli,
};
use polkadot_node_subsystem::SpawnGlue;
use polkadot_node_subsystem_types::{ChainApiBackend, RuntimeApiSubsystemClient};
use sp_core::traits::SpawnNamed;

use crate::interceptor::*;

use std::sync::Arc;

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct SupportDisabledOptions {
	#[clap(flatten)]
	pub cli: Cli,
}

/// Generates an overseer with a custom runtime API subsystem.
pub(crate) struct SupportDisabled;

impl OverseerGen for SupportDisabled {
	fn generate<Spawner, RuntimeClient>(
		&self,
		connector: OverseerConnector,
		args: OverseerGenArgs<'_, Spawner, RuntimeClient>,
		ext_args: Option<ExtendedOverseerGenArgs>,
	) -> Result<(Overseer<SpawnGlue<Spawner>, Arc<RuntimeClient>>, OverseerHandle), Error>
	where
		RuntimeClient: RuntimeApiSubsystemClient + ChainApiBackend + AuxStore + 'static,
		Spawner: 'static + SpawnNamed + Clone + Unpin,
	{
		validator_overseer_builder(
			args,
			ext_args.expect("Extended arguments required to build validator overseer are provided"),
		)?
		.replace_runtime_api(move |ra_subsystem| {
			InterceptedSubsystem::new(ra_subsystem, IgnoreDisabled)
		})
		.build_with_connector(connector)
		.map_err(|e| e.into())
	}
}

#[derive(Clone)]
struct IgnoreDisabled;

impl<Sender> MessageInterceptor<Sender> for IgnoreDisabled
where
	Sender: overseer::RuntimeApiSenderTrait + Clone + Send + 'static,
{
	type Message = RuntimeApiMessage;

	/// Intercept incoming runtime api requests.
	fn intercept_incoming(
		&self,
		_subsystem_sender: &mut Sender,
		msg: FromOrchestra<Self::Message>,
	) -> Option<FromOrchestra<Self::Message>> {
		match msg {
			FromOrchestra::Communication {
				msg:
					RuntimeApiMessage::Request(_relay_parent, RuntimeApiRequest::DisabledValidators(tx)),
			} => {
				let _ = tx.send(Ok(Vec::new()));
				None
			},
			FromOrchestra::Communication { msg } => Some(FromOrchestra::Communication { msg }),
			FromOrchestra::Signal(signal) => Some(FromOrchestra::Signal(signal)),
		}
	}
}
