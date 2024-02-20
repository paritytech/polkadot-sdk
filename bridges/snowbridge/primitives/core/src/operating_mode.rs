use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// Basic operating modes for a bridges module (Normal/Halted).
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BasicOperatingMode {
	/// Normal mode, when all operations are allowed.
	Normal,
	/// The pallet is halted. All non-governance operations are disabled.
	Halted,
}

impl Default for BasicOperatingMode {
	fn default() -> Self {
		Self::Normal
	}
}

impl BasicOperatingMode {
	pub fn is_halted(&self) -> bool {
		*self == BasicOperatingMode::Halted
	}
}
