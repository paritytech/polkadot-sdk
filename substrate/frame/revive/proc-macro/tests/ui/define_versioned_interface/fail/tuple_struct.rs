use pallet_revive_proc_macro::define_versioned_interface;

define_versioned_interface! {
	pub struct UiTupleInputPayloadV1(pub u8);

	pub struct UiTupleOutputPayloadV1 {
		pub value: u8,
	}
}

fn main() {}
