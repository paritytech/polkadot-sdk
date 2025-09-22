//! Runtime API definition for oracle module.

#![cfg_attr(not(feature = "std"), no_std)]
// The `too_many_arguments` warning originates from `decl_runtime_apis` macro.
#![allow(clippy::too_many_arguments)]
// The `unnecessary_mut_passed` warning originates from `decl_runtime_apis` macro.
#![allow(clippy::unnecessary_mut_passed)]

use codec::Codec;
use sp_std::prelude::Vec;

sp_api::decl_runtime_apis! {
	pub trait OracleApi<ProviderId, Key, Value> where
		ProviderId: Codec,
		Key: Codec,
		Value: Codec,
	{
		fn get_value(provider_id: ProviderId, key: Key) -> Option<Value>;
		fn get_all_values(provider_id: ProviderId) -> Vec<(Key, Option<Value>)>;
	}
}
