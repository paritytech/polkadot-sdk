// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tests for various invariants for the versioned runtime API. They need to be here since this is
//! the only place where we have the metadata we need to run these tests and a runtime that we can
//! call.

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use codec::Decode;
use polkadot_sdk::{
	pallet_revive::{
		pallet_revive_types::runtime_api::ReviveRuntimeApiVersionDeclarations,
		runtime_decl_for_revive_api::ReviveApi,
	},
	sp_io::TestExternalities,
	sp_metadata_ir::frame_metadata::{
		v16::{ItemDeprecationInfo, RuntimeApiMetadata, RuntimeMetadataV16},
		RuntimeMetadata, RuntimeMetadataPrefixed,
	},
};
use scale_info::{form::PortableForm, Field, Type, TypeDef};

const UNVERSIONED_RUNTIME_APIS: [&str; 28] = [
	"eth_block",
	"eth_block_hash",
	"eth_receipt_data",
	"block_gas_limit",
	"max_extrinsic_weight_in_gas",
	"balance",
	"gas_price",
	"nonce",
	"call",
	"instantiate",
	"eth_transact",
	"eth_transact_with_config",
	"eth_estimate_gas",
	"eth_pre_dispatch_weight",
	"upload_code",
	"get_storage",
	"get_storage_var_key",
	"trace_block",
	"trace_tx",
	"trace_call",
	"trace_call_with_config",
	"block_author",
	"address",
	"account_id",
	"runtime_pallets_address",
	"code",
	"new_balance_with_dust",
	"version_declarations",
];

/// Tests all of the invariants which we want to uphold for the versioned runtime API.
///
/// 1. That all functions have a "_versioned" suffix.
/// 2. That if an unversioned counterpart exists that it's been deprecated.
/// 3. That all functions have exactly 1 input argument.
/// 4. That the input argument has the name `${function-name-cleaned:camel}VersionedInputPayload`
///    and all output types have the name `${function-name-cleaned:camel}VersionedOutputPayload`.
/// 5. That the input and output types are enums.
/// 6. That the input and output enums have an equal number of variants which is not zero.
/// 7. That the input and output variants have a scale index of N - 1 where N is the version.
/// 8. That the input and output variants are `V` followed by a non-zero number and that they're
///    contiguous.
/// 9. That the input and output variants have a single un-named field.
/// 10. That the input and output variant field is of the type
///     `${function-name-cleaned:camel}(Input|Output)PayloadV${i}`
/// 11. That the function has a versioned declared in the `ReviveRuntimeApiVersionDeclarations`.
/// 12. That the declared version matches the highest version available on the versioned payload
///     enums.
/// 13. That the `ReviveRuntimeApiVersionDeclarations` does not contain any runtime API function
///     which is not versioned.
#[test]
fn test_versioned_runtime_api_invariants() {
	let mut version_declarations = version_declarations().into_iter().collect::<BTreeMap<_, _>>();
	let (metadata, runtime_api_metadata) = metadata();
	let versioned_runtime_api_functions = runtime_api_metadata
		.methods
		.iter()
		.filter(|function| !UNVERSIONED_RUNTIME_APIS.contains(&function.name.as_str()));

	for function in versioned_runtime_api_functions {
		// Assertion 1
		let Some(function_name_no_suffix) = function.name.strip_suffix("_versioned") else {
			panic!(
				"All versioned runtime API function names must have a \"_versioned\" suffix, but \
                {} does not have it",
				function.name
			)
		};
		let function_name_no_cleaned =
			function_name_no_suffix.strip_prefix("eth_").unwrap_or(function_name_no_suffix);

		// Assertion 2
		if let Some(unversioned_function) = runtime_api_metadata
			.methods
			.iter()
			.find(|function| function.name == function_name_no_suffix)
		{
			assert!(
                matches!(unversioned_function.deprecation_info, ItemDeprecationInfo::Deprecated { .. } | ItemDeprecationInfo::DeprecatedWithoutNote),
                "All versioned runtime API functions must deprecate their unversioned counterparts \
                but {}, the unversioned variant of {} is not deprecated",
                unversioned_function.name,
                function.name
            )
		}

		// Assertion 3
		let [input] = function.inputs.as_slice() else {
			panic!(
				"All versioned runtime API functions must have exactly one input argument, but {} \
                has {} arguments.",
				function.name,
				function.inputs.len()
			)
		};

		// Assertion 4
		let output = function.output;
		let input_type = metadata
			.types
			.resolve(input.ty.id)
			.expect("impossible on well-formed metadata; qed");
		let output_type = metadata
			.types
			.resolve(output.id)
			.expect("impossible on well-formed metadata; qed");
		let output_type =
			extract_ok_type_from_result(output_type, &metadata).unwrap_or(output_type);

		let input_type_name = input_type.path.ident().expect("an input type has an ident; qed");
		let output_type_name = output_type.path.ident().expect("an output type has an ident; qed");

		assert_eq!(
			input_type_name,
			format!("{}VersionedInputPayload", snake_to_camel(function_name_no_cleaned)),
			"All versioned runtime API functions must have an input payload with the name \
            ${{function-name-no-suffix:camel}}VersionedInputPayload but {} has an input payload \
            with the name {}",
			function.name,
			input_type_name
		);
		assert_eq!(
			output_type_name,
			format!("{}VersionedOutputPayload", snake_to_camel(function_name_no_cleaned)),
			"All versioned runtime API functions must have an output payload with the name \
            ${{function-name-no-suffix:camel}}VersionedOutputPayload but {} has an output payload \
            with the name {}",
			function.name,
			output_type_name
		);

		// Assertion 5
		let TypeDef::Variant(ref input_variants) = input_type.type_def else {
			panic!(
				"All versioned runtime API functions must have an argument which is an enum. But \
                the arguments of {} is of the type definition {:?}",
				function.name, input_type.type_def
			)
		};
		let TypeDef::Variant(ref output_variants) = output_type.type_def else {
			panic!(
				"All versioned runtime API functions must have an output which is an enum. But \
                the output of {} is of the type definition {:?}",
				function.name, output_type.type_def
			)
		};

		// Assertion 6
		assert_eq!(
			input_variants.variants.len(),
			output_variants.variants.len(),
			"All versioned runtime API functions must have IO enums which have an equal number of \
            variants. But {} has {} input variants and {} output variants",
			function.name,
			input_variants.variants.len(),
			output_variants.variants.len(),
		);
		assert_ne!(
			input_variants.variants.len(),
			0,
			"All versioned runtime API functions must have IO enums with at least 1 variant. But \
            {} has zero variants",
			function.name,
		);

		for (i, (input_variant, output_variant)) in
			input_variants.variants.iter().zip(output_variants.variants.iter()).enumerate()
		{
			let version = i + 1;

			// Assertion 7
			assert_eq!(
				input_variant.index as usize, i,
				"Expected variant {} in {} to have a scale index of {} but it has an index of {}",
				input_variant.name, input_type_name, i, input_variant.index
			);
			assert_eq!(
				output_variant.index as usize, i,
				"Expected variant {} in {} to have a scale index of {} but it has an index of {}",
				output_variant.name, output_type_name, i, output_variant.index
			);

			// Assertion 8
			assert_eq!(
				input_variant.name,
				format!("V{version}"),
				"Expected variant {} in {} to be {} called",
				input_variant.name,
				input_type_name,
				format!("V{version}")
			);
			assert_eq!(
				output_variant.name,
				format!("V{version}"),
				"Expected variant {} in {} to be {} called",
				output_variant.name,
				output_type_name,
				format!("V{version}")
			);

			// Assertion 9
			let [input_field @ Field { name: None, .. }] = input_variant.fields.as_slice() else {
				panic!(
					"Expected variant {} in {} to have a single un-named field",
					input_variant.name, input_type_name
				)
			};
			let [output_field @ Field { name: None, .. }] = output_variant.fields.as_slice() else {
				panic!(
					"Expected variant {} in {} to have a single un-named field",
					output_variant.name, output_type_name
				)
			};

			// Assertion 10
			let input_field_type = metadata
				.types
				.resolve(input_field.ty.id)
				.expect("impossible on well-formed metadata; qed");
			let output_field_type = metadata
				.types
				.resolve(output_field.ty.id)
				.expect("impossible on well-formed metadata; qed");

			let input_field_type_name =
				input_field_type.path.ident().expect("an input type has an ident; qed");
			let output_field_type_name =
				output_field_type.path.ident().expect("an output type has an ident; qed");

			assert_eq!(
				input_field_type_name,
				format!("{}InputPayloadV{}", snake_to_camel(function_name_no_cleaned), version),
				"Expected variant {} of {} to have the type {} but it has the type {}",
				input_variant.name,
				input_type_name,
				format!("{}InputPayloadV{}", snake_to_camel(function_name_no_cleaned), version),
				input_field_type_name
			);
			assert_eq!(
				output_field_type_name,
				format!("{}OutputPayloadV{}", snake_to_camel(function_name_no_cleaned), version),
				"Expected variant {} of {} to have the type {} but it has the type {}",
				output_variant.name,
				output_type_name,
				format!("{}OutputPayloadV{}", snake_to_camel(function_name_no_cleaned), version),
				output_field_type_name
			);
		}

		// Assertion 11
		let Some(declared_version) = version_declarations.remove(&function.name) else {
			panic!(
                "All versioned runtime API functions must have a highest version declared for them \
                on the ReviveRuntimeApiVersionDeclarations but {} has no such declaration",
                function.name
            )
		};

		// Assertion 12
		assert_eq!(
			declared_version as usize,
			input_variants.variants.len(),
			"The declared version for {} is {} but its enum variants have up to V{}",
			function.name,
			declared_version,
			input_variants.variants.len()
		);
	}

	// Assertion 13
	assert!(
		version_declarations.is_empty(),
		"The ReviveRuntimeApiVersionDeclarations contains version declarations for unversioned \
        runtime API function: {:?}",
		version_declarations.into_keys().collect::<BTreeSet<_>>()
	)
}

fn metadata() -> (RuntimeMetadataV16, RuntimeApiMetadata<PortableForm>) {
	let metadata = TestExternalities::default().execute_with(|| {
		let opaque_metadata = revive_dev_runtime::Runtime::metadata_at_version(16)
			.expect("the runtime serves v16 metadata");
		let RuntimeMetadata::V16(metadata) =
			RuntimeMetadataPrefixed::decode(&mut &opaque_metadata[..])
				.expect("the runtime metadata decodes")
				.1
		else {
			panic!("the runtime must serve v16 metadata")
		};
		metadata
	});

	let runtime_api_metadata = metadata
		.apis
		.iter()
		.find(|api| api.name == "ReviveApi")
		.expect("the runtime exposes the ReviveApi")
		.clone();

	(metadata, runtime_api_metadata)
}

fn version_declarations() -> ReviveRuntimeApiVersionDeclarations {
	TestExternalities::default().execute_with(|| {
		<revive_dev_runtime::Runtime as ReviveApi<_, _, _, _, _, _>>::version_declarations()
	})
}

fn snake_to_camel(string: &str) -> String {
	fn internal(
		mut chars: core::iter::Peekable<impl Iterator<Item = char>>,
		mut acc: String,
	) -> String {
		let char1 = chars.next();
		let char2 = chars.peek().copied();

		match (char1, char2) {
			(Some('_'), Some(char2)) => {
				let _ = chars.next().expect("peek succeeded, next must succeed; qed");
				let char = char2.to_ascii_uppercase();
				acc.push(char);
				internal(chars, acc)
			},
			(Some(char1), Some(_) | None) => {
				let char1 = if acc.is_empty() { char1.to_ascii_uppercase() } else { char1 };
				acc.push(char1);
				internal(chars, acc)
			},
			(None, None) => acc,
			(None, Some(_)) => {
				unreachable!(
					"First pull from the iterator can't fail while subsequent one succeeds"
				)
			},
		}
	}

	internal(string.chars().peekable(), String::new())
}

fn extract_ok_type_from_result<'a>(
	ty: &Type<PortableForm>,
	metadata: &'a RuntimeMetadataV16,
) -> Option<&'a Type<PortableForm>> {
	let Some("Result") = ty.path.ident().as_deref() else { return None };

	let TypeDef::Variant(ref variants) = ty.type_def else {
		return None;
	};

	let [variant1, variant2] = variants.variants.as_slice() else {
		return None;
	};

	if variant1.name != "Ok" || variant1.index != 0 || variant2.name != "Err" || variant2.index != 1
	{
		return None;
	};

	let [field @ Field { name: None, .. }] = variant1.fields.as_slice() else { return None };

	Some(
		metadata
			.types
			.resolve(field.ty.id)
			.expect("impossible on well-formed metadata; qed"),
	)
}
