#![cfg_attr(not(feature = "std"), no_std)]

#[sp_runtime_interface::runtime_interface]
pub trait ProxyApi<RuntimeCall, ProxyType> {
	/// Check if a `RuntimeCall` is allowed by the `InstanceFilter` of a given `ProxyType`.
	fn check_permissions(call: RuntimeCall, proxy_type: ProxyType) -> bool;

	/// Determine if a `ProxyType` is a superset of another `ProxyType`.
	fn is_superset(proxy_type: ProxyType, against: ProxyType) -> bool;
}
