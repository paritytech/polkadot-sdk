// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::{Error, Expr, ItemFn, Path, Result};

/// This prefixes all the log lines with `[<name>]` (after the timestamp). It works by making a
/// tracing's span that is propagated to all the child calls and child tasks (futures) if they are
/// spawned properly with the `SpawnHandle` (see `TaskManager` in sc-cli) or if the futures use
/// `.in_current_span()` (see tracing-futures).
///
/// See Tokio's [tracing documentation](https://docs.rs/tracing-core/) and
/// [tracing-futures documentation](https://docs.rs/tracing-futures/) for more details.
///
/// # Implementation notes
///
/// If there are multiple spans with a log prefix, only the latest will be shown.
///
/// # Example with a literal
///
/// ```ignore
/// Builds a new service for a light client.
/// #[sc_cli::prefix_logs_with("light")]
/// pub fn new_light(config: Configuration) -> Result<TaskManager, ServiceError> {
///     let (client, backend, keystore, mut task_manager, on_demand) =
///         sc_service::new_light_parts::<Block, RuntimeApi, Executor>(&config)?;
///
///        ...
/// }
/// ```
///
/// Will produce logs that look like this:
///
/// ```text
/// 2020-10-16 08:03:14  Substrate Node
/// 2020-10-16 08:03:14  ✌️  version 2.0.0-47f7d3f2e-x86_64-linux-gnu
/// 2020-10-16 08:03:14  ❤️  by Anonymous, 2017-2020
/// 2020-10-16 08:03:14  📋 Chain specification: Local Testnet
/// 2020-10-16 08:03:14  🏷  Node name: nice-glove-1401
/// 2020-10-16 08:03:14  👤 Role: LIGHT
/// 2020-10-16 08:03:14  💾 Database: RocksDb at /tmp/substrate95w2Dk/chains/local_testnet/db
/// 2020-10-16 08:03:14  ⛓  Native runtime: node-template-1 (node-template-1.tx1.au1)
/// 2020-10-16 08:03:14  [light] 🔨 Initializing Genesis block/state (state: 0x121d…8e36, header-hash: 0x24ef…8ff6)
/// 2020-10-16 08:03:14  [light] Loading GRANDPA authorities from genesis on what appears to be first startup.
/// 2020-10-16 08:03:15  [light] ⏱  Loaded block-time = 6000 milliseconds from genesis on first-launch
/// 2020-10-16 08:03:15  [light] Using default protocol ID "sup" because none is configured in the chain specs
/// 2020-10-16 08:03:15  [light] 🏷  Local node identity is: 12D3KooWHX4rkWT6a6N55Km7ZnvenGdShSKPkzJ3yj9DU5nqDtWR
/// 2020-10-16 08:03:15  [light] 📦 Highest known block at #0
/// 2020-10-16 08:03:15  [light] 〽️ Prometheus server started at 127.0.0.1:9615
/// 2020-10-16 08:03:15  [light] Listening for new connections on 127.0.0.1:9944.
/// ```
///
/// # Example using the actual node name
///
/// ```ignore
/// Builds a new service for a light client.
/// #[sc_cli::prefix_logs_with(config.network.node_name.as_str())]
/// pub fn new_light(config: Configuration) -> Result<TaskManager, ServiceError> {
///     let (client, backend, keystore, mut task_manager, on_demand) =
///         sc_service::new_light_parts::<Block, RuntimeApi, Executor>(&config)?;
///
///        ...
/// }
/// ```
///
/// Will produce logs that look like this:
///
/// ```text
/// 2020-10-16 08:12:57  Substrate Node
/// 2020-10-16 08:12:57  ✌️  version 2.0.0-efb9b822a-x86_64-linux-gnu
/// 2020-10-16 08:12:57  ❤️  by Anonymous, 2017-2020
/// 2020-10-16 08:12:57  📋 Chain specification: Local Testnet
/// 2020-10-16 08:12:57  🏷  Node name: open-harbor-1619
/// 2020-10-16 08:12:57  👤 Role: LIGHT
/// 2020-10-16 08:12:57  💾 Database: RocksDb at /tmp/substrate9T9Mtb/chains/local_testnet/db
/// 2020-10-16 08:12:57  ⛓  Native runtime: node-template-1 (node-template-1.tx1.au1)
/// 2020-10-16 08:12:58  [open-harbor-1619] 🔨 Initializing Genesis block/state (state: 0x121d…8e36, header-hash: 0x24ef…8ff6)
/// 2020-10-16 08:12:58  [open-harbor-1619] Loading GRANDPA authorities from genesis on what appears to be first startup.
/// 2020-10-16 08:12:58  [open-harbor-1619] ⏱  Loaded block-time = 6000 milliseconds from genesis on first-launch
/// 2020-10-16 08:12:58  [open-harbor-1619] Using default protocol ID "sup" because none is configured in the chain specs
/// 2020-10-16 08:12:58  [open-harbor-1619] 🏷  Local node identity is: 12D3KooWRzmYC8QTK1Pm8Cfvid3skTS4Hn54jc4AUtje8Rqbfgtp
/// 2020-10-16 08:12:58  [open-harbor-1619] 📦 Highest known block at #0
/// 2020-10-16 08:12:58  [open-harbor-1619] 〽️ Prometheus server started at 127.0.0.1:9615
/// 2020-10-16 08:12:58  [open-harbor-1619] Listening for new connections on 127.0.0.1:9944.
/// ```
#[proc_macro_attribute]
pub fn prefix_logs_with(arg: TokenStream, item: TokenStream) -> TokenStream {
	// Ensure an argument was provided.
	if arg.is_empty() {
		return Error::new(
			proc_macro2::Span::call_site(),
			"missing argument: prefix. Example: prefix_logs_with(\"Relaychain\")",
		)
		.to_compile_error()
		.into();
	}

	let prefix_expr = syn::parse_macro_input!(arg as Expr);
	let item_fn = syn::parse_macro_input!(item as ItemFn);

	// Resolve the proper sc_tracing path.
	let crate_name = match resolve_sc_tracing() {
		Ok(path) => path,
		Err(err) => return err.to_compile_error().into(),
	};

	let syn::ItemFn { attrs, vis, sig, block } = item_fn;

	(quote! {
		#(#attrs)*
		#vis #sig {
			let span = #crate_name::tracing::info_span!(
				#crate_name::logging::PREFIX_LOG_SPAN,
				name = #prefix_expr,
			);
			let _enter = span.enter();

			#block
		}
	})
	.into()
}

/// Resolve the correct path for sc_tracing:
/// - If `polkadot-sdk` is in scope, returns a Path corresponding to `polkadot_sdk::sc_tracing`
/// - Otherwise, falls back to `sc_tracing`
fn resolve_sc_tracing() -> Result<Path> {
	match crate_name("polkadot-sdk") {
		Ok(FoundCrate::Itself) => syn::parse_str("polkadot_sdk::sc_tracing"),
		Ok(FoundCrate::Name(sdk_name)) => syn::parse_str(&format!("{}::sc_tracing", sdk_name)),
		Err(_) => match crate_name("sc-tracing") {
			Ok(FoundCrate::Itself) => syn::parse_str("sc_tracing"),
			Ok(FoundCrate::Name(name)) => syn::parse_str(&name),
			Err(e) => Err(syn::Error::new(Span::call_site(), e)),
		},
	}
}
