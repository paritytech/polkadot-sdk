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
use clap::{ArgMatches, Command, FromArgMatches, Subcommand};
use sc_cli::{ExportChainSpecCmd, Result as CliResult, Error as CliError};
use crate::chain_spec::LoadSpec;
use std::collections::HashMap;
use std::io;
use clap::Args;
/// Enum for all supported extra subcommands
#[derive(Debug, Subcommand)]
pub enum ExtraSubcommands {
    /// Export chain spec
    ExportChainSpec(ExportChainSpecCmd),
}

/// Struct that holds command handlers and config
pub struct ExtraCommandsHandler {
    /// Chain Spec loading
    pub chain_spec_loader: Box<dyn LoadSpec>,
}

impl ExtraCommandsHandler {
    /// A new `RunConfig` instance configured with the given components.
    pub fn new(chain_spec_loader: Box<dyn LoadSpec>) -> Self {
        Self { chain_spec_loader }
    }

    /// Returns all supported subcommands as a map for dynamic matching
    pub fn available_commands() -> HashMap<&'static str, Command> {
        let mut map = HashMap::new();
        map.insert(
            "export-chain-spec",
            ExportChainSpecCmd::augment_args_for_update(Command::new("export-chain-spec")),
        );
        // In future: add more commands here.
        map
    }

    /// Handles the execution of a recognized extra command
    pub fn handle(&self, name: &str, matches: &ArgMatches) -> CliResult<()> {
        match name {
            "export-chain-spec" => {
                let cmd = ExportChainSpecCmd::from_arg_matches(matches)
                    .map_err(|e| CliError::Cli(e.into()))?;
                let spec = self.chain_spec_loader
                    .load_spec(&cmd.chain)
                    .map_err(|e| CliError::Application(Box::new(io::Error::other(e))))?;
                cmd.run(spec)
            }
            // Future: add more matches here.
            _ => Err(CliError::Input(format!("Unknown extra command: {}", name))),
        }
    }
}
