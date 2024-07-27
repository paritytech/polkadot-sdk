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

//! This variant of Malus backs/approves all malicious candidates crafted by
//! `suggest-garbage-candidate` variant and behaves honestly with other
//! candidates.

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

use crate::{
	interceptor::*,
	variants::{FakeCandidateValidation, FakeCandidateValidationError, ReplaceValidationResult},
};

use std::sync::Arc;

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct BackGarbageCandidateOptions {
	/// Determines the percentage of garbage candidates that should be backed.
	/// Defaults to 100% of garbage candidates being backed.
	#[clap(short, long, ignore_case = true, default_value_t = 100, value_parser = clap::value_parser!(u8).range(0..=100))]
	pub percentage: u8,

	#[clap(flatten)]
	pub cli: Cli,
}

/// Generates an overseer that replaces the candidate validation subsystem with our malicious
/// variant.
pub(crate) struct BackGarbageCandidates {
	/// The probability of behaving maliciously.
	pub percentage: u8,
}

impl OverseerGen for BackGarbageCandidates {
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
		let spawner = args.spawner.clone();
		let validation_filter = ReplaceValidationResult::new(
			FakeCandidateValidation::BackingAndApprovalValid,
			FakeCandidateValidationError::InvalidOutputs,
			f64::from(self.percentage),
			SpawnGlue(spawner),
		);

		validator_overseer_builder(
			args,
			ext_args.expect("Extended arguments required to build validator overseer are provided"),
		)?
		.replace_candidate_validation(move |cv_subsystem| {
			InterceptedSubsystem::new(cv_subsystem, validation_filter)
		})
		.build_with_connector(connector)
		.map_err(|e| e.into())
	}
}
