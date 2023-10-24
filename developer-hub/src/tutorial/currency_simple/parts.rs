use frame::prelude::*;

pub(crate) mod config_basic_outer {
	use super::*;

	#[pallet_section]
	mod config_basic {
		#[docify::export]
		#[pallet::config]
		pub trait Config: frame_system::Config {}
	}
}

pub(crate) mod config_event_outer {
	use super::*;

	#[pallet_section]
	mod config_event {
		#[pallet::config]
		pub trait Config: frame_system::Config {
			type RuntimeEvent: From<Event<Self>>
				+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
		}
	}
}
