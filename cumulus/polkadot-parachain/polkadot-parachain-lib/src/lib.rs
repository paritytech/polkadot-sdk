// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
//! For an example, see the `polkadot-parachain-bin` crate.

#![deny(missing_docs)]

mod cli;
mod command;
mod common;
mod fake_runtime_api;
mod service;

pub use cli::CliConfig;
pub use command::{run, RunConfig};
pub use common::{chain_spec, runtime};
