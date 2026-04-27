use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	#[versioned_type(extend)]
	pub struct UiOutOfOrderV4 {
		pub second: u16,
	}

	pub struct UiOutOfOrderV3 {
		pub first: u8,
	}
}

fn main() {
	let _value = UiOutOfOrderV4 { first: 1, second: 2 };
}
