// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts XCM messages into simpler commands that can be processed by the Gateway contract

#[cfg(test)]
mod tests;

pub mod convert;
pub mod shape;
mod syntax;
pub use convert::XcmConverter;
pub use shape::snowbridge_v2_outbound_xcm_shape;

pub use crate::v2::exporter::{
	snowbridge_v2_export_blob_contains_alias_origin,
	snowbridge_v2_instructions_contain_alias_origin, validate_ethereum_blob_exporter_v2_route,
	EthereumBlobExporter, XcmFilterExporter, XcmForSnowbridgeV2,
};
