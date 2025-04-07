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

//! Optional/Additional CLI options of the omni-node-lib to be used by other binaries. See [`ExtraSubcommands`].
use crate::chain_spec::LoadSpec;
use clap::{ArgMatches, Args, Command, FromArgMatches, Subcommand};
use sc_cli::{ExportChainSpecCmd, Result as CliResult};

/// A trait for injecting and handling additional CLI subcommands in a composable way.
///
/// This trait allows developers to modularly extend the CLI without modifying the core command
/// definitions. It is particularly useful in projects using `polkadot-omni-node-lib` where downstream
/// crates like parachains may want to define their own custom commands.
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
///     fn maybe_run(name: &str, matches: &ArgMatches, _loader: &dyn LoadSpec) -> Option<sc_cli::Result<()>> {
///         let cmd_enum = MyExtraCommands::from_arg_matches(matches).ok()?;
///         match cmd_enum {
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

pub trait ExtraSubcommand: Sized {
    /// Augments the CLI definition with an additional subcommand.
    ///
    /// This allows the subcommand to be shown in `--help` and parsed by Clap.
    fn augment_command(cmd: Command) -> Command;

    /// Tries to run the subcommand by matching on the name and arguments.
    ///
    /// If this subcommand is not recognized, returns `None`.
    fn maybe_run(
        name: &str,
        matches: &ArgMatches,
        chain_spec_loader: &dyn LoadSpec,
    ) -> Option<CliResult<()>>;
}

/// Enum representing all supported extra subcommands bundled in `polkadot-omni-node-lib`.
///
/// This enum is automatically used when you enable the default extra commands (like `export-chain-spec`)
/// by passing `ExtraSubcommands` to `run::<CliConfig, ExtraSubcommands>()`.
///
/// Downstream crates may define their own enums and implementations to plug in custom logic.
#[derive(Debug, Subcommand)]
pub enum ExtraCommands {
    /// Export the chain spec to JSON.
    #[command(name = "export-chain-spec")]
    ExportChainSpec(ExportChainSpecCmd),
}


/// No-op subcommand handler. Use this when a binary does not expose any extra subcommands.
///
/// Acts as the default `ExtraSubcommand` implementation when no extras are provided.
pub struct NoExtraSubcommand;

impl ExtraSubcommand for NoExtraSubcommand {
    fn augment_command(cmd: Command) -> Command {
        cmd
    }
    fn maybe_run(_name: &str, _matches: &ArgMatches, _disk_chain_spec_loader: &dyn LoadSpec) -> Option<sc_cli::Result<()>> {
        None
    }
}

/// Provides the `export-chain-spec` subcommand.
pub struct ExtraSubcommands;

impl ExtraSubcommand for ExtraSubcommands {
    fn augment_command(cmd: Command) -> Command {
        let base = cmd.version(env!("CARGO_PKG_VERSION"));
        ExtraCommands::augment_subcommands(base)
    }

    fn maybe_run(
        _name: &str,
        matches: &ArgMatches,
        chain_spec_loader: &dyn LoadSpec,
    ) -> Option<sc_cli::Result<()>> {
        match ExtraCommands::from_arg_matches(matches) {
            Ok(ExtraCommands::ExportChainSpec(cmd)) => {
                let spec = chain_spec_loader
                    .load_spec(&cmd.chain)
                    .map_err(|e| sc_cli::Error::Application(Box::new(std::io::Error::other(e))))
                    .ok()?;
                Some(cmd.run(spec))
            }
            Err(e) => {
                eprintln!("Error parsing subcommand: {e}");
                None
            }
        }
    }

}
