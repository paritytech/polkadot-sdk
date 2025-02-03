#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};
// use sp_std::prelude::*;
use frame::traits::InstanceFilter;

#[sp_runtime_interface::runtime_interface]
pub trait ProxyApi<RuntimeCall, ProxyType> {
	/// Check if a RuntimeCall is allowed by the InstanceFilter of a given ProxyType.
	fn check_permissions(call: RuntimeCall, proxy_type: ProxyType) -> bool {
		// Use the Proxy pallet's `InstanceFilter` to check if the call is allowed.
		<ProxyType as InstanceFilter<RuntimeCall>>::filter(&call)
	}

	/// Determine if a ProxyType is a superset of another ProxyType.
	fn is_superset(proxy_type: ProxyType, against: ProxyType) -> bool {
		// Use the Proxy pallet's logic to determine if `proxy_type` is a superset of `against`.
		proxy_type.is_superset(&against)
	}
}