// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Utility for logging transaction collections.

/// Logs every transaction from given `tx_collection` with given level.
macro_rules! log_xt {
	(data: hash, target: $target:expr, $level:expr, $tx_collection:expr, $text_with_format:expr) => {
		if log::log_enabled!(target: $target, $level) {
			for tx in $tx_collection {
				log::log!(target: $target, $level, $text_with_format, tx);
			}
		}
	};
	(data: hash, target: $target:expr, $level:expr, $tx_collection:expr, $text_with_format:expr,  $($arg:expr),*) => {
		if log::log_enabled!(target: $target, $level) {
			for tx in $tx_collection {
				log::log!(target: $target, $level, $text_with_format, tx,  $($arg),*);
			}
		}
	};
	(data: tuple, target: $target:expr, $level:expr, $tx_collection:expr, $text_with_format:expr) => {
		if log::log_enabled!(target: $target, $level) {
			for tx in $tx_collection {
				log::log!(target: $target, $level, $text_with_format, tx.0, tx.1)
			}
		}
	};
}

/// Logs every transaction from given `tx_collection` with trace level.
macro_rules! log_xt_trace {
    (data: $datatype:ident, target: $target:expr, $($arg:tt)+) => ($crate::common::log_xt::log_xt!(data: $datatype, target: $target, log::Level::Trace, $($arg)+));
    (target: $target:expr, $tx_collection:expr, $text_with_format:expr) => ($crate::common::log_xt::log_xt!(data: hash, target: $target, log::Level::Trace, $tx_collection, $text_with_format));
    (target: $target:expr, $tx_collection:expr, $text_with_format:expr, $($arg:expr)*) => ($crate::common::log_xt::log_xt!(data: hash, target: $target, log::Level::Trace, $tx_collection, $text_with_format, $($arg)*));
}

pub(crate) use log_xt;
pub(crate) use log_xt_trace;
