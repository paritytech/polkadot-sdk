// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! JAM network integration for Cumulus parachains.
//!
//! Provides a client for connecting to JAM nodes via RPC, constructing work packages
//! from parachain data, and submitting them to the JAM network for processing.

mod client;
mod error;
mod submitter;
mod work_package;

pub use client::{JamClient, JamClientConfig};
pub use error::Error;
pub use submitter::WorkPackageSubmitter;
pub use work_package::{BuiltWorkPackage, WorkPackageBuilder};

pub use jam_std_common::{self, BlockDesc, Node, NodeExt, Service, WorkPackageStatus};
pub use jam_types;
