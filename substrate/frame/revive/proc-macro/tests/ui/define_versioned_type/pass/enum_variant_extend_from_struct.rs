use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct UiVariantFromStructV1 {
		pub first: u8,
		pub second: u16,
	}

	pub enum UiVariantFromStructV2 {
		#[versioned_type(extend)]
		Variant {
			third: u32,
		},
	}
}

fn main() {
	let value = UiVariantFromStructV2::Variant { first: 1, second: 2, third: 3 };

	match value {
		UiVariantFromStructV2::Variant { first, second, third } => {
			let _observed = (first, second, third);
		},
	}
}
