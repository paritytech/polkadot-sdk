use pallet_revive_proc_macro::define_versioned_interface;

define_versioned_interface! {
	pub struct UiDuplicateInputPayloadV1 {
		pub value: u8,
	}

	pub struct UiDuplicateInputPayloadV1 {
		pub other: u16,
	}

	pub struct UiDuplicateOutputPayloadV1 {
		pub value: u8,
	}
}

fn main() {}
