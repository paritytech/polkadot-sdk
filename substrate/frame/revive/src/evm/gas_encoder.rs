use core::ops::{Div, Rem};
use frame_support::ensure;
use sp_arithmetic::traits::{AtLeast32BitUnsigned, One, Zero};
use sp_core::U256;
use sp_weights::Weight;

/// Encodes gas values for use in the EVM.
///
/// The encoding follows the pattern `g...grrrpppddd`, where:
/// - `ddd`: Deposit value, encoded in the lowest 3 digits.
/// - `ppp`: Proof size, encoded in the next 3 digits.
/// - `rrr`: Reference time, encoded in the next 3 digits.
/// - `g...g`: Gas limit, encoded in the highest digits.
///
/// Each component is scaled using the `SCALE` factor.
#[derive(Debug, Clone)]
pub struct EthGasEncoder<Balance> {
	/// Encodes the raw gas limit. Rounded to the nearest non-zero multiple of this value.
	raw_gas_mask: u128,
	/// Encodes the weight reference time.
	ref_time_mask: u64,
	/// Encodes the weight proof size.
	proof_size_mask: u64,
	/// Encodes the deposit limit.
	deposit_mask: Balance,
}

/// Errors that can occur during encoding.
#[derive(Debug, PartialEq, Eq)]
pub enum GasEncodingError {
	/// Reference time exceeds the allowed limit.
	RefTimeOverflow,
	/// Proof size exceeds the allowed limit.
	ProofSizeOverflow,
	/// Deposit exceeds the allowed limit.
	DepositOverflow,
	/// Raw gas limit exceeds the allowed limit.
	RawGasLimitOverflow,
}

// We use 3 digits to store each component.
const SCALE: u64 = 1_000;

/// Rounds up the given value to the nearest multiple of the mask.
///
/// # Panics
/// Panics if the `mask` is zero.
fn round_up<T>(value: T, mask: T) -> T
where
	T: One + Zero + Copy + Rem<Output = T> + Div<Output = T>,
	<T as Rem>::Output: PartialEq,
{
	let rest = if value % mask == T::zero() { T::zero() } else { T::one() };
	value / mask + rest
}

impl<Balance> EthGasEncoder<Balance>
where
	Balance: Copy + AtLeast32BitUnsigned + TryFrom<U256> + Into<U256> + Debug,
{
	/// Returns the maximum reference time that can be encoded.
	fn max_ref_time(&self) -> u64 {
		self.ref_time_mask * SCALE
	}

	/// Returns the maximum proof size that can be encoded.
	fn max_proof_size(&self) -> u64 {
		self.proof_size_mask * SCALE
	}

	/// Returns the maximum deposit that can be encoded.
	fn max_deposit(&self) -> Balance {
		self.deposit_mask * (SCALE as u32).into()
	}

	/// Creates a new encoder with the given maximum weight and deposit limit.
	pub fn new(max_weight: Weight, max_deposit_limit: Balance) -> Self {
		let ref_time_mask = max_weight.ref_time() / SCALE;
		let proof_size_mask = max_weight.proof_size() / SCALE;
		let deposit_mask = max_deposit_limit / (SCALE as u32).into();
		let raw_gas_mask = SCALE.pow(3) as _;

		log::debug!(
			target: LOG_TARGET,
			"Creating gas encoder: ref_time_mask={ref_time_mask:?}, proof_size_mask={proof_size_mask:?}, deposit_mask={deposit_mask:?}");

		Self { raw_gas_mask, ref_time_mask, proof_size_mask, deposit_mask }
	}

	/// Encodes all components (deposit limit, weight reference time, and proof size) into a single
	/// gas value.
	pub fn encode(
		&self,
		gas_limit: U256,
		weight: Weight,
		deposit: Balance,
	) -> Result<U256, GasEncodingError> {
		let gas_limit: u128 =
			gas_limit.try_into().map_err(|_| GasEncodingError::RawGasLimitOverflow)?;
		let ref_time = weight.ref_time();
		let proof_size = weight.proof_size();

		ensure!(ref_time <= self.max_ref_time(), GasEncodingError::RefTimeOverflow);
		ensure!(proof_size <= self.max_proof_size(), GasEncodingError::ProofSizeOverflow);
		ensure!(deposit <= self.max_deposit(), GasEncodingError::DepositOverflow);

		let raw_gas_component = if gas_limit < self.raw_gas_mask {
			self.raw_gas_mask
		} else {
			round_up(gas_limit, self.raw_gas_mask).saturating_mul(self.raw_gas_mask)
		};

		let deposit_component = round_up(deposit, self.deposit_mask);
		let ref_time_component = round_up(ref_time, self.ref_time_mask);
		let proof_size_component = round_up(proof_size, self.proof_size_mask);

		let encoded = U256::from(raw_gas_component)
			.saturating_add(deposit_component.into())
			.saturating_add(U256::from(SCALE) * U256::from(proof_size_component))
			.saturating_add(U256::from(SCALE.pow(2)) * U256::from(ref_time_component));

		log::debug!(target: LOG_TARGET, "Encoding: gas_limit={gas_limit:?}, weight={weight:?}, deposit={deposit:?} in {encoded:?}");
		Ok(encoded)
	}

	/// Decodes the weight and deposit from the encoded gas value.
	pub fn decode(&self, gas: U256) -> (Weight, Balance) {
		let deposit = gas % SCALE;
		let gas_without_deposit = gas - deposit;

		// Casting with as_* is safe since all values are maxed by `SCALE`.
		let deposit: Balance = deposit.as_u32().into();
		let proof_time: u64 = ((gas_without_deposit / SCALE) % SCALE).as_u64();
		let ref_time: u64 = ((gas_without_deposit / SCALE.pow(2)) % SCALE).as_u64();

		let weight = Weight::from_parts(
			ref_time.saturating_mul(self.proof_size_mask),
			proof_time.saturating_mul(self.ref_time_mask),
		);
		let deposit = deposit.saturating_mul(self.deposit_mask);

		log::debug!(target: LOG_TARGET, "Decoding: gas: {gas:?} into weight={weight:?}, deposit={deposit:?}");
		(weight, deposit)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use frame_support::assert_err;

	#[test]
	fn test_gas_encoding_with_small_values() {
		let max_weight = Weight::from_parts(1_000_000_000, 1_000_000_000);
		let max_deposit = 1_000_000_000u128;
		let encoder = EthGasEncoder::new(max_weight, max_deposit);

		let raw_gas_limit = 444_000_000_000_000u128;
		let weight = Weight::from_parts(66_000, 44_000);
		let deposit = 22_000u128;

		let encoded_gas = encoder.encode(raw_gas_limit.into(), weight, deposit).unwrap();
		assert!(encoded_gas > raw_gas_limit.into());
		assert_eq!(encoded_gas, U256::from(444_000_001_001_001u128));

		let (decoded_weight, decoded_deposit) = encoder.decode(encoded_gas);
		assert_eq!(
			Weight::from_parts(encoder.ref_time_mask, encoder.proof_size_mask),
			decoded_weight
		);
		assert_eq!(encoder.deposit_mask, decoded_deposit);
		assert!(decoded_weight.all_gte(weight));
		assert!(decoded_deposit >= deposit);
	}

	#[test]
	fn test_gas_encoding_with_exact_values() {
		let max_weight = Weight::from_parts(1_000_000_000, 1_000_000_000);
		let max_deposit = 1_000_000_000u128;
		let encoder = EthGasEncoder::new(max_weight, max_deposit);

		let raw_gas_limit = 444_000_000_000_000u128;
		let weight = Weight::from_parts(100_000_000, 100_000_000);
		let deposit = 100_000_000u128;

		let encoded_gas = encoder.encode(raw_gas_limit.into(), weight, deposit).unwrap();
		assert_eq!(encoded_gas, U256::from(444_000_100_100_100u128));

		let (decoded_weight, decoded_deposit) = encoder.decode(encoded_gas);
		assert_eq!(weight, decoded_weight);
		assert_eq!(deposit, decoded_deposit);
	}

	#[test]
	fn test_gas_encoding_with_large_values() {
		let max_weight = Weight::from_parts(1_000_000_000, 1_000_000_000);
		let max_deposit = 1_000_000_000u128;
		let encoder = EthGasEncoder::new(max_weight, max_deposit);

		let raw_gas_limit = 111_111_999_999_999u128;
		let weight = Weight::from_parts(222_999_999, 333_999_999);
		let deposit = 444_999_999;
		let encoded_gas = encoder.encode(raw_gas_limit.into(), weight, deposit).unwrap();

		assert!(encoded_gas > raw_gas_limit.into());
		assert_eq!(encoded_gas, U256::from(111_112_223_334_445u128));

		let (decoded_weight, decoded_deposit) = encoder.decode(encoded_gas);
		assert_eq!(Weight::from_parts(223_000_000, 334_000_000), decoded_weight);
		assert_eq!(445_000_000, decoded_deposit);
		assert!(decoded_weight.all_gte(weight));
		assert!(decoded_deposit >= deposit);
	}

	#[test]
	fn test_gas_encoding_with_zero_values() {
		let max_weight = Weight::from_parts(1_000_000_000, 1_000_000_000);
		let max_deposit = 1_000_000_000u128;
		let encoder = EthGasEncoder::new(max_weight, max_deposit);

		let raw_gas_limit = 0u128;
		let weight = Weight::from_parts(0, 0);
		let deposit = 0u128;

		let encoded_gas = encoder.encode(raw_gas_limit.into(), weight, deposit).unwrap();
		assert_eq!(encoded_gas, U256::from(encoder.raw_gas_mask));

		let (decoded_weight, decoded_deposit) = encoder.decode(encoded_gas);
		assert_eq!(Weight::from_parts(0, 0), decoded_weight);
		assert_eq!(0u128, decoded_deposit);
	}

	#[test]
	fn test_encoding_invalid_values() {
		let max_weight = Weight::from_parts(1_000_000_000, 1_000_000_000);
		let max_deposit = 1_000_000_000u128;
		let encoder = EthGasEncoder::new(max_weight, max_deposit);

		assert_err!(
			encoder.encode(U256::MAX, max_weight.add_ref_time(1), 0u128),
			GasEncodingError::RawGasLimitOverflow
		);
		assert_err!(
			encoder.encode(U256::zero(), max_weight.add_ref_time(1), 0u128),
			GasEncodingError::RefTimeOverflow
		);
		assert_err!(
			encoder.encode(U256::zero(), max_weight.add_proof_size(1), 0u128),
			GasEncodingError::ProofSizeOverflow
		);
		assert_err!(
			encoder.encode(U256::zero(), max_weight, max_deposit + 1),
			GasEncodingError::DepositOverflow
		);
	}
}
