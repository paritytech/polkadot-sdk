use pallet_revive_proc_macro::define_versioned_interface;

define_versioned_interface! {
	#[derive(Clone, Debug())]
	pub struct UiMalformedDeriveInputPayloadV1 {
		pub value: u8,
	}

	pub struct UiMalformedDeriveOutputPayloadV1 {
		pub value: u8,
	}
}

fn main() {}
