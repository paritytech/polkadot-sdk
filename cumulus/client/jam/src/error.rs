// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Error types for JAM integration.

use jam_std_common::NodeError;

/// Errors that can occur during JAM integration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// Failed to establish or maintain RPC connection to a JAM node.
	#[error("RPC connection error: {0}")]
	Connection(#[from] jsonrpsee::core::client::Error),

	/// An error returned by the JAM node.
	#[error("JAM node error: {0}")]
	Node(#[from] NodeError),

	/// Failed to submit a work package to any guarantor.
	#[error("Work package submission failed: {0}")]
	Submission(String),

	/// The JAM node reported a work package failure.
	#[error("Work package failed: {0}")]
	WorkPackageFailed(String),

	/// Invalid configuration.
	#[error("Configuration error: {0}")]
	Config(String),

	/// Codec error during encoding/decoding.
	#[error("Codec error: {0}")]
	Codec(String),

	/// RPC URL could not be parsed.
	#[error("Invalid RPC URL: {0}")]
	InvalidUrl(#[from] url::ParseError),
}
