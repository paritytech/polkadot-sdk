// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! # Polkadot Omni Node Library
//!
//! Helper library that can be used to run a parachain node.
//!
//! ## Overview
//!
//! This library can be used to run a parachain node while also customizing the chain specs
//! that are supported by default by the `--chain-spec` argument of the node's `CLI`
//! and the parameters of the runtime that is associated with each of these chain specs.
//!
//! ## API
//!
//! The library exposes the possibility to provide a [`RunConfig`]. Through this structure
//! 2 optional configurations can be provided:
//! - a chain spec loader (an implementation of [`chain_spec::LoadSpec`]): this can be used for
//!   providing the chain specs that are supported by default by the `--chain-spec` argument of the
//!   node's `CLI` and the actual chain config associated with each one.
//! - a runtime resolver (an implementation of [`runtime::RuntimeResolver`]): this can be used for
//!   providing the parameters of the runtime that is associated with each of the chain specs
//!
//! Apart from this, a [`CliConfig`] can also be provided, that can be used to customize some
//! user-facing binary author, support url, etc.
//!
//! ## Examples
//!
//! For an example, see the [`polkadot-parachain-bin`](https://crates.io/crates/polkadot-parachain-bin) crate.
//!
//! ## Binary
//!
//! It can be used to start a parachain node from a provided chain spec file.
//! It is only compatible with runtimes that use block number `u32` and `Aura` consensus.
//!
//! Example: `polkadot-omni-node --chain <chain_spec.json>`

#![deny(missing_docs)]

pub mod cli;
mod command;
mod common;
mod fake_runtime_api;
mod nodes;

pub use cli::CliConfig;
pub use command::{run, RunConfig};
pub use common::{chain_spec, runtime};
