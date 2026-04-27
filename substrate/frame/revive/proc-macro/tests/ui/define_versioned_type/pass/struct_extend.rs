use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct UiPassV1 {
		pub first: u8,
	}

	#[versioned_type(extend)]
	pub struct UiPassV2 {
		pub second: u16,
	}
}

fn main() {
	let _value = UiPassV2 { first: 1, second: 2 };
}
