use pallet_revive_proc_macro::define_versioned_interface;

define_versioned_interface! {
	pub struct UiGenericConflictInputPayloadV1<T> {
		pub value: T,
	}

	pub struct UiGenericConflictOutputPayloadV1 {
		pub value: u8,
	}

	pub struct UiGenericConflictInputPayloadV2<const T: usize> {
		pub value: [u8; T],
	}

	pub struct UiGenericConflictOutputPayloadV2 {
		pub value: u8,
	}
}

fn main() {}
