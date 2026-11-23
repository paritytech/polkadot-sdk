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

//! Tracing is not a JAM host call: every function is a no-op inside the runtime. On the node
//! side (std) they forward to the `tracing` dispatcher, mirroring the `host_interface.rs` bodies.

/// Whether the span described in `WasmMetadata` should be traced wasm-side.
///
/// `false` inside the runtime; on the node side converts into a static Metadata and checks
/// against the global `tracing` dispatcher.
pub fn enabled(metadata: sp_tracing::WasmMetadata) -> bool {
	#[cfg(not(substrate_runtime))]
	{
		let metadata: &tracing_core::metadata::Metadata<'static> = (&metadata).into();
		tracing::dispatcher::get_default(|d| d.enabled(metadata))
	}
	#[cfg(substrate_runtime)]
	{
		let _ = metadata;
		false
	}
}

/// Open a new span with the given attributes. Return the u64 Id of the span.
///
/// `0` inside the runtime; on the node side this goes through the default `tracing` dispatcher
/// to register the span and then calls `clone_span` with the ID to signal that we are keeping it
/// around on the wasm-side even after the local span is dropped.
pub fn enter_span(span: sp_tracing::WasmEntryAttributes) -> u64 {
	#[cfg(not(substrate_runtime))]
	{
		let span: tracing::Span = span.into();
		match span.id() {
			Some(id) => tracing::dispatcher::get_default(|d| {
				// inform dispatch that we'll keep the ID around
				// then enter it immediately
				let final_id = d.clone_span(&id);
				d.enter(&final_id);
				final_id.into_u64()
			}),
			_ => 0,
		}
	}
	#[cfg(substrate_runtime)]
	{
		let _ = span;
		0
	}
}

/// Emit the given event to the global tracer on the native side.
///
/// No-op inside the runtime.
pub fn event(event: sp_tracing::WasmEntryAttributes) {
	#[cfg(not(substrate_runtime))]
	event.emit();
	#[cfg(substrate_runtime)]
	let _ = event;
}

/// Signal that a given span-id has been exited.
///
/// No-op inside the runtime; on the node side this directly proxies the span to the global
/// dispatcher.
pub fn exit(span: u64) {
	#[cfg(not(substrate_runtime))]
	tracing::dispatcher::get_default(|d| {
		let id = tracing_core::span::Id::from_u64(span);
		d.exit(&id);
	});
	#[cfg(substrate_runtime)]
	let _ = span;
}

#[cfg(all(test, not(substrate_runtime)))]
mod tests {
	#[test]
	fn tracing_fns_do_not_panic() {
		crate::common::init_tracing();
		let _ = super::enabled(Default::default());
		let _ = super::enter_span(Default::default());
		super::event(Default::default());
		super::exit(1); // span IDs must be > 0 (tracing-core)
	}
}
