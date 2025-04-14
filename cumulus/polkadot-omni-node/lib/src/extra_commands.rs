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

//! Optional/Additional CLI options of the omni-node-lib to be used by other binaries. See
//! [`ExtraSubcommands`].
use clap::{ArgMatches, Command, CommandFactory, FromArgMatches, Parser};
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
/// 1. Define the subcommand using [`clap::Parser`].
/// 2. Implement this trait for it.
/// 3. Use it when running the node via `run::<CliConfig, YourExtraCommand>(run_config)`
///
/// ### Minimal Example:
///
///
/// use clap::Parser;
/// use polkadot_omni_node_lib::{ExtraSubcommand, RunConfig};
///
/// #[derive(Debug, Clone, Parser)]
/// pub struct FooCmd {
///     /// Prints a foo message
///     #[arg(long)]
///     pub foo: Option<String>,
/// }
///
/// pub struct FooCommand;
///
/// impl ExtraSubcommand for FooCommand {
///     type P = FooCmd;
///
///     fn handle(cmd: FooCmd, _config: &RunConfig) -> sc_cli::Result<()> {
///         println!("Hello from Foo! {:?}", cmd.foo);
///         Ok(())
///     }
/// }
///
///
/// To use this in a binary:
///
///
/// let config = RunConfig::new(...);
/// run::<CliConfig, FooCommand>(config)?;
///
///
/// Running it:
///
/// ```bash
/// $ your-binary foo --foo bar
/// Hello from Foo! Some("bar")
/// ```
///
/// ## Supporting Multiple Subcommands
///
/// You can compose multiple extra commands via an enum. Just derive [`clap::Parser`] and match
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
///     type P = Self;
///
///     fn handle(cmd: Self, config: &RunConfig) -> sc_cli::Result<()> {
///         match cmd {
///             MyExtras::Foo(foo) => { ... }
///             MyExtras::Bar(bar) => { ... }
///         }
///         Ok(())
///     }
/// }
///


/// A trait for CLI subcommands that can be optionally added by downstream consumers.
pub trait ExtraSubcommand {
	/// The clap [`Parser`] type representing this extra command (usually a struct or enum).
	type P: Parser;

	/// Optionally override the subcommand metadata (name, version, help).
	///
	/// Defaults to [`Parser::command`] on `Self::P`.
	fn command() -> Option<Command> {
		Some(Self::P::command())
	}

	/// Parse and handle this extra subcommand, if recognized.
	///
	/// Returns `Ok(true)` if this subcommand matches and is handled.
	/// Returns `Ok(false)` if this subcommand is unrelated to the extra handler.
	/// Returns `Err(_)` if the extra command failed at runtime or parsing.
	fn handle_with_matches(matches: &ArgMatches, config: &RunConfig) -> Result<bool> {
		match Self::P::from_arg_matches(matches) {
			Ok(res) => Self::handle(res, config).map(|_| true),
			Err(_) => Ok(false),
		}
	}

	/// Handle the command once it's been parsed.
	fn handle(p: Self::P, config: &RunConfig) -> Result<()>;
}

/// Built-in extra subcommands provided by `polkadot-omni-node-lib`.
///
/// Currently includes:
/// - `export-chain-spec`
///
/// You can use this by passing [`ExtraSubcommands`] to `run::<CliConfig, ExtraSubcommands>()`.
/// This enables default support for utilities like:
///
/// ```bash
/// $ your-binary export-chain-spec --chain westmint
/// ```
///
/// Downstream crates may use this enum directly or extend it with their own subcommands.
#[derive(Debug, Parser)]
pub enum ExtraSubcommands {
	/// Export the chain spec to JSON.
	ExportChainSpec(ExportChainSpecCmd),
}

/// No-op subcommand handler. Use this when a binary does not expose any extra subcommands.
///
/// Acts as the default `ExtraSubcommand` implementation when no extras are provided.
#[derive(Parser)]
pub struct NoExtraSubcommand;

impl ExtraSubcommand for NoExtraSubcommand {
	type P = Self;
	fn handle(_p: NoExtraSubcommand, _config: &RunConfig) -> Result<()> {
		Ok(())
	}
}

impl ExtraSubcommand for ExtraSubcommands {
	type P = Self;

	fn handle(p: ExtraSubcommands, config: &RunConfig) -> Result<()> {
		match p {
			ExtraSubcommands::ExportChainSpec(cmd) => {
				let spec = config.chain_spec_loader.load_spec(&cmd.chain)?;
				cmd.run(spec)?;
			},
		}

		Ok(())
	}
}
