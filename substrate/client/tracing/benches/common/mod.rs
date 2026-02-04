// This file is part of Substrate.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Common utilities for interest cache benchmarks.

use log::{Level, LevelFilter};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// Initialize logger with interest cache configuration from INTEREST_CACHE environment variable.
pub fn init_logger() {
	let mut log_tracer = tracing_log::LogTracer::builder().with_max_level(LevelFilter::Trace);

	if let Some(config) = parse_interest_cache_config() {
		eprintln!("Interest cache config: {config:?}");
		log_tracer = log_tracer.with_interest_cache(config);
	}

	log_tracer.init().expect("Failed to init LogTracer");

	let env_filter = EnvFilter::default()
		.add_directive("info".parse().unwrap())
		.add_directive("dummy=trace".parse().unwrap());

	let subscriber = FmtSubscriber::builder()
		.with_env_filter(env_filter)
		.with_writer(std::io::sink)
		.with_filter_reloading()
		.finish();

	tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");
}

/// Parse interest cache configuration from INTEREST_CACHE environment variable.
///
/// Returns `None` if cache should be disabled, `Some(config)` otherwise.
///
/// Supported formats:
/// - "disabled", "none", "no", "false" - cache disabled
/// - "default" - cache enabled with default config
/// - "key=value,key=value" - cache enabled with custom config
///   - lru_cache_size=<number> - set LRU cache size
///   - min_verbosity=<level> - set minimum verbosity level (error, warn, info, debug, trace)
fn parse_interest_cache_config() -> Option<tracing_log::InterestCacheConfig> {
	let config_str = std::env::var("INTEREST_CACHE").ok()?;
	let config_lower = config_str.to_lowercase();
	let mut config = tracing_log::InterestCacheConfig::default();

	// Check if disabled
	if matches!(config_lower.as_str(), "none" | "disabled" | "no" | "false") {
		eprintln!("Interest cache: disabled");
		return Some(config.with_lru_cache_size(0));
	}

	// If "default", use default config
	if config_lower == "default" {
		eprintln!("Interest cache: default config");
		return Some(config);
	}

	// Parse key=value pairs
	for pair in config_str.split(',') {
		let parts: Vec<&str> = pair.trim().split('=').collect();
		if parts.len() != 2 {
			eprintln!("Invalid config pair '{}', expected key=value", pair);
			continue;
		}

		let key = parts[0].trim();
		let value = parts[1].trim();

		match key {
			"lru_cache_size" =>
				if let Ok(size) = value.parse::<usize>() {
					config = config.with_lru_cache_size(size);
					eprintln!("Interest cache: lru_cache_size = {}", size);
				} else {
					eprintln!("Invalid lru_cache_size value '{}'", value);
				},
			"min_verbosity" => {
				let level = match value.to_lowercase().as_str() {
					"error" => Some(Level::Error),
					"warn" => Some(Level::Warn),
					"info" => Some(Level::Info),
					"debug" => Some(Level::Debug),
					"trace" => Some(Level::Trace),
					_ => {
						eprintln!("Invalid min_verbosity '{}', using default", value);
						None
					},
				};
				if let Some(level) = level {
					config = config.with_min_verbosity(level);
					eprintln!("Interest cache: min_verbosity = {:?}", level);
				}
			},
			_ => {
				eprintln!("Unknown config key '{}'", key);
			},
		}
	}

	Some(config)
}
