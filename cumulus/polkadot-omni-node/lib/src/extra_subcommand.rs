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

//! Optional / additional CLI options for binaries built with
//! `polkadot‑omni‑node‑lib`.
/// * Binaries that need extra utilities (e.g. `export-chain-spec`) should pass
///   [`DefaultExtraSubcommands`] to `run_with_custom_cli`, which injects that one command.
/// * Binaries that should stay minimal pass [`NoExtraSubcommand`], which requests no extras at
///   all.
use clap::Subcommand;
use sc_cli::{ExportChainSpecCmd, Result};

use crate::RunConfig;

/// A trait for injecting and handling additional CLI subcommands in a composable way.
///
/// This trait allows downstream crates using `polkadot-omni-node-lib` to plug in their own custom
/// subcommands without having to modify the main CLI definition. This is especially useful for
/// parachain node binaries that want to define optional utilities.
///
/// ## Implementing a Custom Extra Command
///
/// To create your own subcommand:
///
/// 1. Define the subcommand using [`clap::Subcommand`].
/// 2. Implement this trait for it.
/// 3. Use it when running the node via `run_with_custom_cli::<CliConfig,
///    YourExtraCommand>(run_config)`
///
/// ### Minimal Example:
///
///
/// use clap::Parser;
/// use polkadot_omni_node_lib::{ExtraSubcommand, RunConfig};
///
/// #[derive(Debug, Clone, Subcommand)]
/// pub enum ExtraCmd {
///		NewCmd,
/// }
///
/// impl ExtraSubcommand for ExtraCmd {
///
///   fn handle(_cmd: ExtraCmd, _config: &RunConfig) -> sc_cli::Result<()> {
///         println!("Hello from Extra!");
///         Ok(())
///     }
/// }
///
///
/// To use this in a binary:
///
///
/// let config = RunConfig::new(...);
/// run_with_custom_cli::<CliConfig, ExtraCmd>(config)?;
///
///
/// Running it:
///
/// ```bash
/// $ your-binary new-cmd
/// Hello from Extra!
/// ```
///
/// ## Supporting Multiple Subcommands
///
/// You can compose multiple extra commands via an enum. Just derive [`clap::Subcommand`] and match
/// over the variants in `handle`.
///
///
/// #[derive(Debug, clap::Parser)]
/// pub enum MyExtras {
///     Foo(FooCmd),
///     Bar(BarCmd),
/// }
///
/// impl ExtraSubcommand for MyExtras {
///
///   fn handle(cmd: Self, config: &RunConfig) -> sc_cli::Result<()> {
///         match cmd {
///             MyExtras::Foo(foo) => { ... }
///             MyExtras::Bar(bar) => { ... }
///         }
///         Ok(())
///     }
/// }

/// Trait implemented by a set of optional sub‑commands**.
pub trait ExtraSubcommand: Subcommand {
	/// Handle the command once it's been parsed.
	fn handle(self, cfg: &RunConfig) -> Result<()>;
}

/// Built-in extra subcommands provided by `polkadot-omni-node-lib`.
///
/// Currently, it includes:
/// - `export-chain-spec`
///
/// You can use this by passing [`DefaultExtraSubcommands`] to `run_with_custom_cli`
/// or by just calling `run` as this is the default.
///
/// This enables default support for utilities like:
/// ```bash
/// $ your-binary export-chain-spec --chain westmint
/// ```
#[derive(Debug, Subcommand)]
pub enum DefaultExtraSubcommands {
	/// Export the chain spec to JSON.
	ExportChainSpec(ExportChainSpecCmd),
}

impl ExtraSubcommand for DefaultExtraSubcommands {
	fn handle(self, cfg: &RunConfig) -> Result<()> {
		match self {
			DefaultExtraSubcommands::ExportChainSpec(cmd) => {
				let spec = cfg.chain_spec_loader.load_spec(&cmd.chain)?;
				cmd.run(spec)?;
			},
		}
		Ok(())
	}
}

/// No-op subcommand handler. Use this when a binary does not expose any extra subcommands.
///
/// You can use this by passing [`NoExtraSubcommand`] to `run_with_custom_cli`.
#[derive(Debug, Subcommand)]
pub enum NoExtraSubcommand {}

impl ExtraSubcommand for NoExtraSubcommand {
	fn handle(self, _cfg: &RunConfig) -> Result<()> {
		Ok(())
	}
}
