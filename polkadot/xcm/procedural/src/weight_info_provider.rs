// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use quote::{format_ident, quote};
use syn::{parse_macro_input, Type};

fn impl_weight_provider(
	input: proc_macro::TokenStream,
	trait_name: &str,
	methods: &[&str],
) -> proc_macro::TokenStream {
	let ty = parse_macro_input!(input as Type);
	let trait_ident = format_ident!("{}", trait_name);
	let method_idents = methods.iter().map(|name| format_ident!("{}", name));

	quote! {
		impl ::pallet_xcm_benchmarks::xcm_weights::#trait_ident for #ty {
			#(
				fn #method_idents() -> frame_support::weights::Weight {
					<#ty>::#method_idents()
				}
			)*
		}
	}
	.into()
}

pub fn impl_generic_provider(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	impl_weight_provider(
		input,
		"XcmGenericWeightInfo",
		&[
			"query_response",
			"transact",
			"clear_origin",
			"descend_origin",
			"report_error",
			"report_holding",
			"buy_execution",
			"pay_fees",
			"refund_surplus",
			"set_error_handler",
			"set_appendix",
			"clear_error",
			"asset_claimer",
			"claim_asset",
			"trap",
			"subscribe_version",
			"unsubscribe_version",
			"burn_asset",
			"expect_asset",
			"expect_origin",
			"expect_error",
			"expect_transact_status",
			"query_pallet",
			"expect_pallet",
			"report_transact_status",
			"clear_transact_status",
			"universal_origin",
			"set_fees_mode",
			"set_topic",
			"clear_topic",
			"alias_origin",
			"unpaid_execution",
			"execute_with_origin",
			"exchange_asset",
		],
	)
}

pub fn impl_fungible_provider(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	impl_weight_provider(
		input,
		"XcmFungibleWeightInfo",
		&[
			"withdraw_asset",
			"reserve_asset_deposited",
			"receive_teleported_asset",
			"transfer_asset",
			"transfer_reserve_asset",
			"deposit_asset",
			"deposit_reserve_asset",
			"initiate_reserve_withdraw",
			"initiate_teleport",
			"initiate_transfer",
		],
	)
}
