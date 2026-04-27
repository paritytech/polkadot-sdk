use pallet_revive_proc_macro::define_versioned_interface;

define_versioned_interface! {
	pub struct UiMissingPairInputPayloadV1 {
		pub value: u8,
	}

	pub struct UiMissingPairOutputPayloadV2 {
		pub value: u8,
	}
}

fn main() {}
