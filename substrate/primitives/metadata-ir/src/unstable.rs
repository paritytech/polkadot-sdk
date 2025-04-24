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

//! Convert the IR to V16 metadata.

use crate::{
	DeprecationInfoIR, DeprecationStatusIR, PalletAssociatedTypeMetadataIR, PalletCallMetadataIR,
	PalletConstantMetadataIR, PalletErrorMetadataIR, PalletEventMetadataIR,
	PalletStorageMetadataIR, PalletViewFunctionMetadataIR, PalletViewFunctionParamMetadataIR,
	StorageEntryMetadataIR,
};

use super::types::{
	ExtrinsicMetadataIR, MetadataIR, PalletMetadataIR, RuntimeApiMetadataIR,
	RuntimeApiMethodMetadataIR, TransactionExtensionMetadataIR,
};

use frame_metadata::v16::{
	CustomMetadata, DeprecationInfo, DeprecationStatus, ExtrinsicMetadata, FunctionParamMetadata,
	PalletAssociatedTypeMetadata, PalletCallMetadata, PalletConstantMetadata, PalletErrorMetadata,
	PalletEventMetadata, PalletMetadata, PalletStorageMetadata, PalletViewFunctionMetadata,
	RuntimeApiMetadata, RuntimeApiMethodMetadata, RuntimeMetadataV16, StorageEntryMetadata,
	TransactionExtensionMetadata,
};

use codec::Compact;
use scale_info::form::MetaForm;

impl From<MetadataIR> for RuntimeMetadataV16 {
	fn from(ir: MetadataIR) -> Self {
		RuntimeMetadataV16::new(
			ir.pallets.into_iter().map(Into::into).collect(),
			ir.extrinsic.into(),
			ir.apis.into_iter().map(Into::into).collect(),
			ir.outer_enums.into(),
			// Substrate does not collect yet the custom metadata fields.
			// This allows us to extend the V16 easily.
			CustomMetadata { map: Default::default() },
		)
	}
}

impl From<RuntimeApiMetadataIR> for RuntimeApiMetadata {
	fn from(ir: RuntimeApiMetadataIR) -> Self {
		RuntimeApiMetadata {
			name: ir.name,
			methods: ir.methods.into_iter().map(Into::into).collect(),
			docs: ir.docs,
			deprecation_info: ir.deprecation_info.into(),
			version: ir.version.into(),
		}
	}
}

impl From<RuntimeApiMethodMetadataIR> for RuntimeApiMethodMetadata {
	fn from(ir: RuntimeApiMethodMetadataIR) -> Self {
		RuntimeApiMethodMetadata {
			name: ir.name,
			inputs: ir.inputs.into_iter().map(Into::into).collect(),
			output: ir.output,
			docs: ir.docs,
			deprecation_info: ir.deprecation_info.into(),
		}
	}
}

impl From<PalletMetadataIR> for PalletMetadata {
	fn from(ir: PalletMetadataIR) -> Self {
		PalletMetadata {
			name: ir.name,
			storage: ir.storage.map(Into::into),
			calls: ir.calls.map(Into::into),
			view_functions: ir.view_functions.into_iter().map(Into::into).collect(),
			event: ir.event.map(Into::into),
			constants: ir.constants.into_iter().map(Into::into).collect(),
			error: ir.error.map(Into::into),
			index: ir.index,
			docs: ir.docs,
			associated_types: ir.associated_types.into_iter().map(Into::into).collect(),
			deprecation_info: ir.deprecation_info.into(),
		}
	}
}

impl From<PalletStorageMetadataIR> for PalletStorageMetadata {
	fn from(ir: PalletStorageMetadataIR) -> Self {
		PalletStorageMetadata {
			prefix: ir.prefix,
			entries: ir.entries.into_iter().map(Into::into).collect(),
		}
	}
}

impl From<StorageEntryMetadataIR> for StorageEntryMetadata {
	fn from(ir: StorageEntryMetadataIR) -> Self {
		StorageEntryMetadata {
			name: ir.name,
			modifier: ir.modifier.into(),
			ty: ir.ty.into(),
			default: ir.default,
			docs: ir.docs,
			deprecation_info: ir.deprecation_info.into(),
		}
	}
}

impl From<PalletAssociatedTypeMetadataIR> for PalletAssociatedTypeMetadata {
	fn from(ir: PalletAssociatedTypeMetadataIR) -> Self {
		PalletAssociatedTypeMetadata { name: ir.name, ty: ir.ty, docs: ir.docs }
	}
}

impl From<PalletErrorMetadataIR> for PalletErrorMetadata {
	fn from(ir: PalletErrorMetadataIR) -> Self {
		PalletErrorMetadata { ty: ir.ty, deprecation_info: ir.deprecation_info.into() }
	}
}

impl From<PalletEventMetadataIR> for PalletEventMetadata {
	fn from(ir: PalletEventMetadataIR) -> Self {
		PalletEventMetadata { ty: ir.ty, deprecation_info: ir.deprecation_info.into() }
	}
}

impl From<PalletCallMetadataIR> for PalletCallMetadata {
	fn from(ir: PalletCallMetadataIR) -> Self {
		PalletCallMetadata { ty: ir.ty, deprecation_info: ir.deprecation_info.into() }
	}
}

impl From<PalletViewFunctionMetadataIR> for PalletViewFunctionMetadata {
	fn from(ir: PalletViewFunctionMetadataIR) -> Self {
		PalletViewFunctionMetadata {
			name: ir.name,
			id: ir.id,
			inputs: ir.inputs.into_iter().map(Into::into).collect(),
			output: ir.output,
			docs: ir.docs.into_iter().map(Into::into).collect(),
			deprecation_info: ir.deprecation_info.into(),
		}
	}
}

impl From<PalletViewFunctionParamMetadataIR> for FunctionParamMetadata<MetaForm> {
	fn from(ir: PalletViewFunctionParamMetadataIR) -> Self {
		FunctionParamMetadata { name: ir.name, ty: ir.ty }
	}
}

impl From<PalletConstantMetadataIR> for PalletConstantMetadata {
	fn from(ir: PalletConstantMetadataIR) -> Self {
		PalletConstantMetadata {
			name: ir.name,
			ty: ir.ty,
			value: ir.value,
			docs: ir.docs,
			deprecation_info: ir.deprecation_info.into(),
		}
	}
}

impl From<TransactionExtensionMetadataIR> for TransactionExtensionMetadata {
	fn from(ir: TransactionExtensionMetadataIR) -> Self {
		TransactionExtensionMetadata { identifier: ir.identifier, ty: ir.ty, implicit: ir.implicit }
	}
}

impl From<ExtrinsicMetadataIR> for ExtrinsicMetadata {
	fn from(ir: ExtrinsicMetadataIR) -> Self {
		// Assume version 0 for all extensions.
		let indexes = (0..ir.extensions.len()).map(|index| Compact(index as u32)).collect();
		let transaction_extensions_by_version = [(0, indexes)].iter().cloned().collect();

		ExtrinsicMetadata {
			versions: ir.versions,
			address_ty: ir.address_ty,
			signature_ty: ir.signature_ty,
			transaction_extensions_by_version,
			transaction_extensions: ir.extensions.into_iter().map(Into::into).collect(),
		}
	}
}

impl From<DeprecationStatusIR> for DeprecationStatus {
	fn from(ir: DeprecationStatusIR) -> Self {
		match ir {
			DeprecationStatusIR::NotDeprecated => DeprecationStatus::NotDeprecated,
			DeprecationStatusIR::DeprecatedWithoutNote => DeprecationStatus::DeprecatedWithoutNote,
			DeprecationStatusIR::Deprecated { since, note } =>
				DeprecationStatus::Deprecated { since, note },
		}
	}
}

impl From<DeprecationInfoIR> for DeprecationInfo {
	fn from(ir: DeprecationInfoIR) -> Self {
		match ir {
			DeprecationInfoIR::NotDeprecated => DeprecationInfo::NotDeprecated,
			DeprecationInfoIR::ItemDeprecated(status) =>
				DeprecationInfo::ItemDeprecated(status.into()),
			DeprecationInfoIR::VariantsDeprecated(btree) => DeprecationInfo::VariantsDeprecated(
				btree.into_iter().map(|(key, value)| (key.0, value.into())).collect(),
			),
		}
	}
}
