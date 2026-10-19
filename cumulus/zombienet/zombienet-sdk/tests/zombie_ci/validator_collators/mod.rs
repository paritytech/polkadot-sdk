// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Validator collators on Asset Hub and People.
//!
//! * `flow`: relay chain validators elected on Asset Hub collate on Asset Hub and People.
//! * `scale`: both chains rotate to an authority list of Polkadot Asset Hub's size.
//! * `drop`: People drops a validator that has session keys and runs no collator node.

mod common;
mod drop;
mod flow;
mod scale;
