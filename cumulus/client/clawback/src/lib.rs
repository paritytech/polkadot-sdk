// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.
//
#![cfg_attr(not(feature = "std"), no_std)]

extern crate sp_api;
extern crate sp_core;
extern crate sp_externalities;
extern crate sp_runtime;
extern crate sp_runtime_interface;
extern crate sp_std;
extern crate sp_trie;

use sp_externalities::Extension;
use sp_runtime_interface::runtime_interface;
use sp_trie::ProofSizeEstimationProvider;
#[cfg(feature = "std")]
use std::sync::Arc;

#[cfg(feature = "std")]
use sp_api::ExtensionProducer;

use sp_std::boxed::Box;

#[cfg(feature = "std")]
use sp_runtime_interface::ExternalitiesExt;

#[runtime_interface]
pub trait ClawbackHostFunctions {
	fn current_storage_proof_size(&mut self) -> u32 {
		match self.extension::<PovUsageExt>() {
			Some(ext) => ext.current_storage_proof_size(),
			None => 0,
		}
	}
}

pub trait ReportPovUsage: Send + Sync {
	fn current_storage_proof_size(&self) -> u32;
}

#[cfg(feature = "std")]
sp_externalities::decl_extension! {
	pub struct PovUsageExt(PovUsageReporter);
}

pub struct PovUsageReporter {
	recorder: Box<dyn ProofSizeEstimationProvider + Sync + Send>,
}

impl PovUsageReporter {
	fn new(recorder: Box<dyn ProofSizeEstimationProvider + Sync + Send>) -> Self {
		PovUsageReporter { recorder }
	}

	fn current_storage_proof_size(&self) -> u32 {
		self.recorder.estimate_proof_size() as u32
	}
}

#[cfg(feature = "std")]
pub fn get_extension_factory() -> ExtensionProducer {
	std::sync::Arc::new(|recorder| {
		(
			core::any::TypeId::of::<PovUsageExt>(),
			Box::new(PovUsageExt(PovUsageReporter::new(recorder)))
				as Box<dyn Extension + Send + Sync + 'static>,
		)
	}) as Arc<_>
}
