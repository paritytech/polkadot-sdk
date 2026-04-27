use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub enum UiVariantOverrideExtendV1 {
		Variant {
			first: u8,
			second: u16,
		},
	}

	#[versioned_type(extend)]
	pub enum UiVariantOverrideExtendV2 {
		#[versioned_type(override, extend)]
		Variant {
			#[versioned_type(override)]
			second: u32,
			third: u64,
		},
	}
}

fn main() {
	let value = UiVariantOverrideExtendV2::Variant { first: 1, second: 2, third: 3 };

	match value {
		UiVariantOverrideExtendV2::Variant { first, second, third } => {
			let _observed = (first, second, third);
		},
	}
}
