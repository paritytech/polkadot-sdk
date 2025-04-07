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
/// This trait allows developers to modularly extend the CLI without modifying the core command
/// definitions. It is particularly useful in projects using `polkadot-omni-node-lib` where
/// downstream crates like parachains may want to define their own custom commands.
///
/// You can use this by implementing `ExtraSubcommand` for your custom command,
/// and passing it into the `run::<CliConfig, YourExtraSubcommands>()` function.
///
///
/// # Example
///
/// Suppose you want to add a custom subcommand `foo` that prints `"bar"` when run,
/// and takes an optional `--foo <value>` argument.
///
/// First, define your subcommand struct:
///
/// #[derive(Debug, Clone, Parser)]
/// pub struct FooCmd {
///     #[arg(long)]
///     pub foo: Option<String>,
/// }
///
/// Then, define a subcommand enum (required by `augment_subcommands`):
///
///
/// #[derive(Debug, Subcommand)]
/// pub enum MyExtraCommands {
///     #[command(name = "foo")]
///     Foo(FooCmd),
/// }
///
/// Now implement `ExtraSubcommand`:
///
///
/// pub struct MyExtra;
///
/// impl ExtraSubcommand for MyExtra {
///     fn augment_command(cmd: Command) -> Command {
///         let base = cmd.version(env!("CARGO_PKG_VERSION"));
///         MyExtraCommands::augment_subcommands(base)
///     }
///
///     fn maybe_run(name: &str, matches: &ArgMatches, _loader: &dyn LoadSpec) ->
/// Option<sc_cli::Result<()>> {         let cmd_enum =
/// MyExtraCommands::from_arg_matches(matches).ok()?;         match cmd_enum {
///             MyExtraCommands::Foo(cmd) => {
///                 println!("Hello from foo! {:?}", cmd.foo);
///                 Some(Ok(()))
///             }
///         }
///     }
/// }
///
///
/// Finally, wire it in:
///
///
/// let config = RunConfig::new(...);
/// run::<CliConfig, MyExtra>(config)?;
///
///
/// And that's it! Your binary now supports:
///
/// ```bash
/// $ my-binary foo --foo hello
/// Hello from foo! Some("hello")
/// ```
///
/// This design is composable. If you need to support **multiple** extra commands, implement your
/// own enum to match over them.
///
/// ## Multiple Subcommands
///
/// To support multiple custom subcommands:
///
/// 1. Extend the `ExtraCommands` enum with your custom subcommands.
/// 2. Update the `maybe_run` match logic.
/// 3. Reuse the `ExtraSubcommands` struct (or define your own).
///
/// ### Example
///
/// Suppose you want to add a second subcommand `dummy`:
///
///
/// #[derive(Debug, Clone, Parser)]
/// pub struct DummyCmd {
///     #[arg(long)]
///     pub message: Option<String>,
/// }
///
/// Extend the existing enum:
///
/// #[derive(Debug, Subcommand)]
/// pub enum ExtraCommands {
///     #[command(name = "export-chain-spec")]
///     ExportChainSpec(sc_cli::ExportChainSpecCmd),
///
///     #[command(name = "dummy")]
///     Dummy(DummyCmd),
/// }
///
/// Update the `maybe_run` method:
///
/// match ExtraCommands::from_arg_matches(matches) {
///     Ok(ExtraCommands::ExportChainSpec(cmd)) => {
///         let spec = chain_spec_loader.load_spec(&cmd.chain).ok()?;
///         Some(cmd.run(spec))
///     }
///     Ok(ExtraCommands::Dummy(cmd)) => {
///         println!("Dummy command invoked with: {:?}", cmd.message);
///         Some(Ok(()))
///     }
///     Err(_) => None,
/// }
///
/// Then in your binary:
///
/// Ok(run::<MyCliConfig, ExtraSubcommands>(config)?)
///
/// Now your CLI supports:
///
/// ```bash
/// $ my-binary dummy --message hello
/// Dummy command invoked with: Some("hello")
/// ```

pub trait ExtraSubcommand {
	/// bla
	type P: Parser;

	/// bla
	fn command() -> Option<Command> {
		Some(Self::P::command())
	}

	/// bla
	fn handle_with_matches(matches: &ArgMatches, config: &RunConfig) -> Result<bool> {
		match Self::P::from_arg_matches(matches) {
			Ok(res) => Self::handle(res, config).map(|_| true),
			Err(_) => Ok(false),
		}
	}

	/// bla
	fn handle(p: Self::P, config: &RunConfig) -> Result<()>;
}

/// Enum representing all supported extra subcommands bundled in `polkadot-omni-node-lib`.
///
/// This enum is automatically used when you enable the default extra commands (like
/// `export-chain-spec`) by passing `ExtraSubcommands` to `run::<CliConfig, ExtraSubcommands>()`.
///
/// Downstream crates may define their own enums and implementations to plug in custom logic.
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
