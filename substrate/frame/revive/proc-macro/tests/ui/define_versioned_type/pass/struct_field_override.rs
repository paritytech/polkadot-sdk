use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct UiStructFieldOverrideV1 {
		pub first: u8,
		pub second: u16,
	}

	#[versioned_type(extend)]
	pub struct UiStructFieldOverrideV2 {
		#[versioned_type(override)]
		pub second: u32,
		pub third: u64,
	}
}

fn main() {
	let _value = UiStructFieldOverrideV2 { first: 1, second: 2, third: 3 };
}
