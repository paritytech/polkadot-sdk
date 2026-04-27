use pallet_revive_proc_macro::define_versioned_interface;

define_versioned_interface! {
	pub struct UiUnitInputPayloadV1;

	pub struct UiUnitOutputPayloadV1 {
		pub value: u8,
	}
}

fn main() {}
