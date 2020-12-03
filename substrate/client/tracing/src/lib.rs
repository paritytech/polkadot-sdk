// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Instrumentation implementation for substrate.
//!
//! This crate is unstable and the API and usage may change.
//!
//! # Usage
//!
//! See `sp-tracing` for examples on how to use tracing.
//!
//! Currently we provide `Log` (default), `Telemetry` variants for `Receiver`

pub mod logging;

use rustc_hash::FxHashMap;
use std::fmt;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::ser::{Serialize, Serializer, SerializeMap};
use tracing::{
	event::Event,
	field::{Visit, Field},
	Level,
	span::{Attributes, Id, Record},
	subscriber::Subscriber,
};
use tracing_subscriber::{
	fmt::time::ChronoLocal,
	CurrentSpan,
	EnvFilter,
	layer::{self, Layer, Context},
	fmt as tracing_fmt,
	Registry,
};

use sc_telemetry::{telemetry, SUBSTRATE_INFO};
use sp_tracing::{WASM_NAME_KEY, WASM_TARGET_KEY, WASM_TRACE_IDENTIFIER};
use tracing_subscriber::reload::Handle;
use once_cell::sync::OnceCell;
use tracing_subscriber::filter::Directive;

const ZERO_DURATION: Duration = Duration::from_nanos(0);

// The layered Subscriber as built up in `init_logger()`.
// Used in the reload `Handle`.
type SCSubscriber<
	N = tracing_fmt::format::DefaultFields,
	E = logging::EventFormat<ChronoLocal>,
	W = fn() -> std::io::Stderr
> = layer::Layered<tracing_fmt::Layer<Registry, N, E, W>, Registry>;

// Handle to reload the tracing log filter
static FILTER_RELOAD_HANDLE: OnceCell<Handle<EnvFilter, SCSubscriber>> = OnceCell::new();
// Directives that are defaulted to when resetting the log filter
static DEFAULT_DIRECTIVES: OnceCell<Mutex<Vec<String>>> = OnceCell::new();
// Current state of log filter
static CURRENT_DIRECTIVES: OnceCell<Mutex<Vec<String>>> = OnceCell::new();

/// Initialize FILTER_RELOAD_HANDLE, only possible once
pub fn set_reload_handle(handle: Handle<EnvFilter, SCSubscriber>) {
	let _ = FILTER_RELOAD_HANDLE.set(handle);
}

/// Add log filter directive(s) to the defaults
///
/// The syntax is identical to the CLI `<target>=<level>`:
///
/// `sync=debug,state=trace`
pub fn add_default_directives(directives: &str) {
	DEFAULT_DIRECTIVES.get_or_init(|| Mutex::new(Vec::new())).lock().push(directives.to_owned());
	add_directives(directives);
}

/// Add directives to current directives
pub fn add_directives(directives: &str) {
	CURRENT_DIRECTIVES.get_or_init(|| Mutex::new(Vec::new())).lock().push(directives.to_owned());
}

/// Reload the logging filter with the supplied directives added to the existing directives
pub fn reload_filter() -> Result<(), String> {
	let mut env_filter = EnvFilter::default();
	if let Some(current_directives) = CURRENT_DIRECTIVES.get() {
		// Use join and then split in case any directives added together
		for directive in current_directives.lock().join(",").split(',').map(|d| d.parse()) {
			match directive {
				Ok(dir) => env_filter = env_filter.add_directive(dir),
				Err(invalid_directive) => {
					log::warn!(
						target: "tracing",
						"Unable to parse directive while setting log filter: {:?}",
						invalid_directive,
					);
				}
			}
		}
	}
	env_filter = env_filter.add_directive(
		"sc_tracing=trace"
			.parse()
			.expect("provided directive is valid"),
	);
	log::debug!(target: "tracing", "Reloading log filter with: {}", env_filter);
	FILTER_RELOAD_HANDLE.get()
		.ok_or("No reload handle present".to_string())?
		.reload(env_filter)
		.map_err(|e| format!("{}", e))
}

/// Resets the log filter back to the original state when the node was started.
///
/// Includes substrate defaults and CLI supplied directives.
pub fn reset_log_filter() -> Result<(), String> {
	*CURRENT_DIRECTIVES
		.get_or_init(|| Mutex::new(Vec::new())).lock() =
		DEFAULT_DIRECTIVES.get_or_init(|| Mutex::new(Vec::new())).lock().clone();
	reload_filter()
}

/// Parse `Directive` and add to default directives if successful. 
///
/// Ensures the supplied directive will be restored when resetting the log filter.
pub fn parse_default_directive(directive: &str) -> Result<Directive, String> {
	let dir = directive
		.parse()
		.map_err(|_| format!("Unable to parse directive: {}", directive))?;
	add_default_directives(directive);
	Ok(dir)
}

/// Responsible for assigning ids to new spans, which are not re-used.
pub struct ProfilingLayer {
	targets: Vec<(String, Level)>,
	trace_handler: Box<dyn TraceHandler>,
	span_data: Mutex<FxHashMap<Id, SpanDatum>>,
	current_span: CurrentSpan,
}

/// Used to configure how to receive the metrics
#[derive(Debug, Clone)]
pub enum TracingReceiver {
	/// Output to logger
	Log,
	/// Output to telemetry
	Telemetry,
}

impl Default for TracingReceiver {
	fn default() -> Self {
		Self::Log
	}
}

/// A handler for tracing `SpanDatum`
pub trait TraceHandler: Send + Sync {
	/// Process a `SpanDatum`
	fn handle_span(&self, span: SpanDatum);
	/// Process a `TraceEvent`
	fn handle_event(&self, event: TraceEvent);
}

/// Represents a tracing event, complete with values
#[derive(Debug)]
pub struct TraceEvent {
	pub name: &'static str,
	pub target: String,
	pub level: Level,
	pub values: Values,
	pub parent_id: Option<Id>,
}

/// Represents a single instance of a tracing span
#[derive(Debug)]
pub struct SpanDatum {
	/// id for this span
	pub id: Id,
	/// id of the parent span, if any
	pub parent_id: Option<Id>,
	/// Name of this span
	pub name: String,
	/// Target, typically module
	pub target: String,
	/// Tracing Level - ERROR, WARN, INFO, DEBUG or TRACE
	pub level: Level,
	/// Line number in source
	pub line: u32,
	/// Time that the span was last entered
	pub start_time: Instant,
	/// Total duration of span while entered
	pub overall_time: Duration,
	/// Values recorded to this span
	pub values: Values,
}

/// Holds associated values for a tracing span
#[derive(Default, Clone, Debug)]
pub struct Values {
	/// HashMap of `bool` values
	pub bool_values: FxHashMap<String, bool>,
	/// HashMap of `i64` values
	pub i64_values: FxHashMap<String, i64>,
	/// HashMap of `u64` values
	pub u64_values: FxHashMap<String, u64>,
	/// HashMap of `String` values
	pub string_values: FxHashMap<String, String>,
}

impl Values {
	/// Returns a new instance of Values
	pub fn new() -> Self {
		Default::default()
	}

	/// Checks if all individual collections are empty
	pub fn is_empty(&self) -> bool {
		self.bool_values.is_empty() &&
			self.i64_values.is_empty() &&
			self.u64_values.is_empty() &&
			self.string_values.is_empty()
	}
}

impl Visit for Values {
	fn record_i64(&mut self, field: &Field, value: i64) {
		self.i64_values.insert(field.name().to_string(), value);
	}

	fn record_u64(&mut self, field: &Field, value: u64) {
		self.u64_values.insert(field.name().to_string(), value);
	}

	fn record_bool(&mut self, field: &Field, value: bool) {
		self.bool_values.insert(field.name().to_string(), value);
	}

	fn record_str(&mut self, field: &Field, value: &str) {
		self.string_values.insert(field.name().to_string(), value.to_owned());
	}

	fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
		self.string_values.insert(field.name().to_string(), format!("{:?}", value).to_owned());
	}
}

impl Serialize for Values {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where S: Serializer,
	{
		let len = self.bool_values.len() + self.i64_values.len() + self.u64_values.len() + self.string_values.len();
		let mut map = serializer.serialize_map(Some(len))?;
		for (k, v) in &self.bool_values {
			map.serialize_entry(k, v)?;
		}
		for (k, v) in &self.i64_values {
			map.serialize_entry(k, v)?;
		}
		for (k, v) in &self.u64_values {
			map.serialize_entry(k, v)?;
		}
		for (k, v) in &self.string_values {
			map.serialize_entry(k, v)?;
		}
		map.end()
	}
}

impl fmt::Display for Values {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let bool_iter = self.bool_values.iter().map(|(k, v)| format!("{}={}", k, v));
		let i64_iter = self.i64_values.iter().map(|(k, v)| format!("{}={}", k, v));
		let u64_iter = self.u64_values.iter().map(|(k, v)| format!("{}={}", k, v));
		let string_iter = self.string_values.iter().map(|(k, v)| format!("{}=\"{}\"", k, v));
		let values = bool_iter.chain(i64_iter).chain(u64_iter).chain(string_iter).collect::<Vec<String>>().join(", ");
		write!(f, "{}", values)
	}
}

impl slog::SerdeValue for Values {
	fn as_serde(&self) -> &dyn erased_serde::Serialize {
		self
	}

	fn to_sendable(&self) -> Box<dyn slog::SerdeValue + Send + 'static> {
		Box::new(self.clone())
	}
}

impl slog::Value for Values {
	fn serialize(
		&self,
		_record: &slog::Record,
		key: slog::Key,
		ser: &mut dyn slog::Serializer,
	) -> slog::Result {
		ser.emit_serde(key, self)
	}
}

impl ProfilingLayer {
	/// Takes a `TracingReceiver` and a comma separated list of targets,
	/// either with a level: "pallet=trace,frame=debug"
	/// or without: "pallet,frame" in which case the level defaults to `trace`.
	/// wasm_tracing indicates whether to enable wasm traces
	pub fn new(receiver: TracingReceiver, targets: &str) -> Self {
		match receiver {
			TracingReceiver::Log => Self::new_with_handler(Box::new(LogTraceHandler), targets),
			TracingReceiver::Telemetry => Self::new_with_handler(
				Box::new(TelemetryTraceHandler),
				targets,
			),
		}
	}

	/// Allows use of a custom TraceHandler to create a new instance of ProfilingSubscriber.
	/// Takes a comma separated list of targets,
	/// either with a level, eg: "pallet=trace"
	/// or without: "pallet" in which case the level defaults to `trace`.
	/// wasm_tracing indicates whether to enable wasm traces
	pub fn new_with_handler(trace_handler: Box<dyn TraceHandler>, targets: &str) -> Self {
		let targets: Vec<_> = targets.split(',').map(|s| parse_target(s)).collect();
		Self {
			targets,
			trace_handler,
			span_data: Mutex::new(FxHashMap::default()),
			current_span: Default::default(),
		}
	}

	fn check_target(&self, target: &str, level: &Level) -> bool {
		for t in &self.targets {
			if target.starts_with(t.0.as_str()) && level <= &t.1 {
				return true;
			}
		}
		false
	}
}

// Default to TRACE if no level given or unable to parse Level
// We do not support a global `Level` currently
fn parse_target(s: &str) -> (String, Level) {
	match s.find('=') {
		Some(i) => {
			let target = s[0..i].to_string();
			if s.len() > i {
				let level = s[i + 1..s.len()].parse::<Level>().unwrap_or(Level::TRACE);
				(target, level)
			} else {
				(target, Level::TRACE)
			}
		}
		None => (s.to_string(), Level::TRACE)
	}
}

impl<S: Subscriber> Layer<S> for ProfilingLayer {
	fn new_span(&self, attrs: &Attributes<'_>, id: &Id, _ctx: Context<S>) {
		let mut values = Values::default();
		attrs.record(&mut values);
		let span_datum = SpanDatum {
			id: id.clone(),
			parent_id: attrs.parent().cloned().or_else(|| self.current_span.id()),
			name: attrs.metadata().name().to_owned(),
			target: attrs.metadata().target().to_owned(),
			level: attrs.metadata().level().clone(),
			line: attrs.metadata().line().unwrap_or(0),
			start_time: Instant::now(),
			overall_time: ZERO_DURATION,
			values,
		};
		self.span_data.lock().insert(id.clone(), span_datum);
	}

	fn on_record(&self, span: &Id, values: &Record<'_>, _ctx: Context<S>) {
		let mut span_data = self.span_data.lock();
		if let Some(s) = span_data.get_mut(span) {
			values.record(&mut s.values);
		}
	}

	fn on_event(&self, event: &Event<'_>, _ctx: Context<S>) {
		let mut values = Values::default();
		event.record(&mut values);
		let trace_event = TraceEvent {
			name: event.metadata().name(),
			target: event.metadata().target().to_owned(),
			level: event.metadata().level().clone(),
			values,
			parent_id: event.parent().cloned().or_else(|| self.current_span.id()),
		};
		self.trace_handler.handle_event(trace_event);
	}

	fn on_enter(&self, span: &Id, _ctx: Context<S>) {
		self.current_span.enter(span.clone());
		let mut span_data = self.span_data.lock();
		let start_time = Instant::now();
		if let Some(mut s) = span_data.get_mut(&span) {
			s.start_time = start_time;
		}
	}

	fn on_exit(&self, span: &Id, _ctx: Context<S>) {
		self.current_span.exit();
		let end_time = Instant::now();
		let span_datum = {
			let mut span_data = self.span_data.lock();
			span_data.remove(&span)
		};

		if let Some(mut span_datum) = span_datum {
			span_datum.overall_time += end_time - span_datum.start_time;
			if span_datum.name == WASM_TRACE_IDENTIFIER {
				span_datum.values.bool_values.insert("wasm".to_owned(), true);
				if let Some(n) = span_datum.values.string_values.remove(WASM_NAME_KEY) {
					span_datum.name = n;
				}
				if let Some(t) = span_datum.values.string_values.remove(WASM_TARGET_KEY) {
					span_datum.target = t;
				}
				if self.check_target(&span_datum.target, &span_datum.level) {
					self.trace_handler.handle_span(span_datum);
				}
			} else {
				self.trace_handler.handle_span(span_datum);
			}
		};
	}

	fn on_close(&self, span: Id, ctx: Context<S>) {
		self.on_exit(&span, ctx)
	}
}

/// TraceHandler for sending span data to the logger
pub struct LogTraceHandler;

fn log_level(level: Level) -> log::Level {
	match level {
		Level::TRACE => log::Level::Trace,
		Level::DEBUG => log::Level::Debug,
		Level::INFO => log::Level::Info,
		Level::WARN => log::Level::Warn,
		Level::ERROR => log::Level::Error,
	}
}

impl TraceHandler for LogTraceHandler {
	fn handle_span(&self, span_datum: SpanDatum) {
		if span_datum.values.is_empty() {
			log::log!(
				log_level(span_datum.level),
				"{}: {}, time: {}, id: {}, parent_id: {:?}",
				span_datum.target,
				span_datum.name,
				span_datum.overall_time.as_nanos(),
				span_datum.id.into_u64(),
				span_datum.parent_id.map(|s| s.into_u64()),
			);
		} else {
			log::log!(
				log_level(span_datum.level),
				"{}: {}, time: {}, id: {}, parent_id: {:?}, values: {}",
				span_datum.target,
				span_datum.name,
				span_datum.overall_time.as_nanos(),
				span_datum.id.into_u64(),
				span_datum.parent_id.map(|s| s.into_u64()),
				span_datum.values,
			);
		}
	}

	fn handle_event(&self, event: TraceEvent) {
		log::log!(
			log_level(event.level),
			"{}, parent_id: {:?}, {}",
			event.target,
			event.parent_id.map(|s| s.into_u64()),
			event.values,
		);
	}
}

/// TraceHandler for sending span data to telemetry,
/// Please see telemetry documentation for details on how to specify endpoints and
/// set the required telemetry level to activate tracing messages
pub struct TelemetryTraceHandler;

impl TraceHandler for TelemetryTraceHandler {
	fn handle_span(&self, span_datum: SpanDatum) {
		telemetry!(SUBSTRATE_INFO; "tracing.profiling";
			"name" => span_datum.name,
			"target" => span_datum.target,
			"time" => span_datum.overall_time.as_nanos(),
			"id" => span_datum.id.into_u64(),
			"parent_id" => span_datum.parent_id.map(|i| i.into_u64()),
			"values" => span_datum.values
		);
	}

	fn handle_event(&self, event: TraceEvent) {
		telemetry!(SUBSTRATE_INFO; "tracing.event";
			"name" => event.name,
			"target" => event.target,
			"parent_id" => event.parent_id.map(|i| i.into_u64()),
			"values" => event.values
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Arc;
	use tracing_subscriber::layer::SubscriberExt;

	struct TestTraceHandler {
		spans: Arc<Mutex<Vec<SpanDatum>>>,
		events: Arc<Mutex<Vec<TraceEvent>>>,
	}

	impl TraceHandler for TestTraceHandler {
		fn handle_span(&self, sd: SpanDatum) {
			self.spans.lock().push(sd);
		}

		fn handle_event(&self, event: TraceEvent) {
			self.events.lock().push(event);
		}
	}

	type TestSubscriber = tracing_subscriber::layer::Layered<
		ProfilingLayer,
		tracing_subscriber::fmt::Subscriber
	>;

	fn setup_subscriber() -> (TestSubscriber, Arc<Mutex<Vec<SpanDatum>>>, Arc<Mutex<Vec<TraceEvent>>>) {
		let spans = Arc::new(Mutex::new(Vec::new()));
		let events = Arc::new(Mutex::new(Vec::new()));
		let handler = TestTraceHandler {
			spans: spans.clone(),
			events: events.clone(),
		};
		let layer = ProfilingLayer::new_with_handler(
			Box::new(handler),
			"test_target",
		);
		let subscriber = tracing_subscriber::fmt().finish().with(layer);
		(subscriber, spans, events)
	}

	#[test]
	fn test_span() {
		let (sub, spans, events) = setup_subscriber();
		let _sub_guard = tracing::subscriber::set_default(sub);
		let span = tracing::info_span!(target: "test_target", "test_span1");
		assert_eq!(spans.lock().len(), 0);
		assert_eq!(events.lock().len(), 0);
		let _guard = span.enter();
		assert_eq!(spans.lock().len(), 0);
		assert_eq!(events.lock().len(), 0);
		drop(_guard);
		drop(span);
		assert_eq!(spans.lock().len(), 1);
		assert_eq!(events.lock().len(), 0);
		let sd = spans.lock().remove(0);
		assert_eq!(sd.name, "test_span1");
		assert_eq!(sd.target, "test_target");
		let time: u128 = sd.overall_time.as_nanos();
		assert!(time > 0);
	}

	#[test]
	fn test_span_parent_id() {
		let (sub, spans, _events) = setup_subscriber();
		let _sub_guard = tracing::subscriber::set_default(sub);
		let span1 = tracing::info_span!(target: "test_target", "test_span1");
		let _guard1 = span1.enter();
		let span2 = tracing::info_span!(target: "test_target", "test_span2");
		let _guard2 = span2.enter();
		drop(_guard2);
		drop(span2);
		let sd2 = spans.lock().remove(0);
		drop(_guard1);
		drop(span1);
		let sd1 = spans.lock().remove(0);
		assert_eq!(sd1.id, sd2.parent_id.unwrap())
	}

	#[test]
	fn test_span_values() {
		let (sub, spans, _events) = setup_subscriber();
		let _sub_guard = tracing::subscriber::set_default(sub);
		let test_bool = true;
		let test_u64 = 1u64;
		let test_i64 = 2i64;
		let test_str = "test_str";
		let span = tracing::info_span!(
			target: "test_target",
			"test_span1",
			test_bool,
			test_u64,
			test_i64,
			test_str
		);
		let _guard = span.enter();
		drop(_guard);
		drop(span);
		let sd = spans.lock().remove(0);
		assert_eq!(sd.name, "test_span1");
		assert_eq!(sd.target, "test_target");
		let values = sd.values;
		assert_eq!(values.bool_values.get("test_bool").unwrap(), &test_bool);
		assert_eq!(values.u64_values.get("test_u64").unwrap(), &test_u64);
		assert_eq!(values.i64_values.get("test_i64").unwrap(), &test_i64);
		assert_eq!(values.string_values.get("test_str").unwrap(), &test_str.to_owned());
	}

	#[test]
	fn test_event() {
		let (sub, _spans, events) = setup_subscriber();
		let _sub_guard = tracing::subscriber::set_default(sub);
		tracing::event!(target: "test_target", tracing::Level::INFO, "test_event");
		let mut te1 = events.lock().remove(0);
		assert_eq!(te1.values.string_values.remove(&"message".to_owned()).unwrap(), "test_event".to_owned());
	}

	#[test]
	fn test_event_parent_id() {
		let (sub, spans, events) = setup_subscriber();
		let _sub_guard = tracing::subscriber::set_default(sub);

		// enter span
		let span1 = tracing::info_span!(target: "test_target", "test_span1");
		let _guard1 = span1.enter();

		// emit event
		tracing::event!(target: "test_target", tracing::Level::INFO, "test_event");

		//exit span
		drop(_guard1);
		drop(span1);

		let sd1 = spans.lock().remove(0);
		let te1 = events.lock().remove(0);

		assert_eq!(sd1.id, te1.parent_id.unwrap());
	}

	#[test]
	fn test_parent_id_with_threads() {
		use std::sync::mpsc;
		use std::thread;

		let (sub, spans, events) = setup_subscriber();
		let _sub_guard = tracing::subscriber::set_global_default(sub);
		let span1 = tracing::info_span!(target: "test_target", "test_span1");
		let _guard1 = span1.enter();

		let (tx, rx) = mpsc::channel();
		let handle = thread::spawn(move || {
			let span2 = tracing::info_span!(target: "test_target", "test_span2");
			let _guard2 = span2.enter();
			// emit event
			tracing::event!(target: "test_target", tracing::Level::INFO, "test_event1");
			for msg in rx.recv() {
				if msg == false {
					break;
				}
			}
			// gard2 and span2 dropped / exited
		});

		// wait for Event to be dispatched and stored
		while events.lock().is_empty() {
			thread::sleep(Duration::from_millis(1));
		}

		// emit new event (will be second item in Vec) while span2 still active in other thread
		tracing::event!(target: "test_target", tracing::Level::INFO, "test_event2");

		// stop thread and drop span
		let _ = tx.send(false);
		let _ = handle.join();

		// wait for Span to be dispatched and stored
		while spans.lock().is_empty() {
			thread::sleep(Duration::from_millis(1));
		}
		let span2 = spans.lock().remove(0);
		let event1 = events.lock().remove(0);
		drop(_guard1);
		drop(span1);

		// emit event with no parent
		tracing::event!(target: "test_target", tracing::Level::INFO, "test_event3");

		let span1 = spans.lock().remove(0);
		let event2 = events.lock().remove(0);

		assert_eq!(event1.values.string_values.get("message").unwrap(), "test_event1");
		assert_eq!(event2.values.string_values.get("message").unwrap(), "test_event2");
		assert!(span1.parent_id.is_none());
		assert!(span2.parent_id.is_none());
		assert_eq!(span2.id, event1.parent_id.unwrap());
		assert_eq!(span1.id, event2.parent_id.unwrap());
		assert_ne!(span2.id, span1.id);

		let event3 = events.lock().remove(0);
		assert!(event3.parent_id.is_none());
	}
}
