use pallet_revive_proc_macro::define_versioned_interface;

define_versioned_interface! {
	pub struct UiSkippedInputPayloadV1 {
		pub value: u8,
	}

	pub struct UiSkippedOutputPayloadV1 {
		pub value: u8,
	}

	pub struct UiSkippedInputPayloadV3 {
		pub value: u8,
	}

	pub struct UiSkippedOutputPayloadV3 {
		pub value: u8,
	}
}

fn main() {}
