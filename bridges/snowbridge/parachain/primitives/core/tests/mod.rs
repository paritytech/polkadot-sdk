#[cfg(test)]
mod tests {
	use frame_support::traits::Contains;
	use snowbridge_core::AllowSiblingsOnly;
	use xcm::prelude::{Junction::Parachain, Junctions::X1, MultiLocation};

	#[test]
	fn allow_siblings_predicate_only_allows_siblings() {
		let sibling = MultiLocation::new(1, X1(Parachain(1000)));
		let child = MultiLocation::new(0, X1(Parachain(1000)));
		assert!(AllowSiblingsOnly::contains(&sibling), "Sibling returns true.");
		assert!(!AllowSiblingsOnly::contains(&child), "Child returns false.");
	}
}
