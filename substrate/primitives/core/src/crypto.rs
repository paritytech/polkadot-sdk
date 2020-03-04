// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

// tag::description[]
//! Cryptographic utilities.
// end::description[]

use sp_std::hash::Hash;
use sp_std::vec::Vec;
#[cfg(feature = "std")]
use sp_std::convert::TryInto;
use sp_std::convert::TryFrom;
#[cfg(feature = "std")]
use parking_lot::Mutex;
#[cfg(feature = "std")]
use rand::{RngCore, rngs::OsRng};
use codec::{Encode, Decode};
#[cfg(feature = "std")]
use regex::Regex;
#[cfg(feature = "std")]
use base58::{FromBase58, ToBase58};

use zeroize::Zeroize;
#[doc(hidden)]
pub use sp_std::ops::Deref;
use sp_runtime_interface::pass_by::PassByInner;

/// The root phrase for our publicly known keys.
pub const DEV_PHRASE: &str = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";

/// The address of the associated root phrase for our publicly known keys.
pub const DEV_ADDRESS: &str = "5DfhGyQdFobKM8NsWvEeAKk5EQQgYe9AydgJ7rMB6E1EqRzV";

/// The infallible type.
#[derive(crate::RuntimeDebug)]
pub enum Infallible {}

/// The length of the junction identifier. Note that this is also referred to as the
/// `CHAIN_CODE_LENGTH` in the context of Schnorrkel.
#[cfg(feature = "full_crypto")]
pub const JUNCTION_ID_LEN: usize = 32;

/// Similar to `From`, except that the onus is on the part of the caller to ensure
/// that data passed in makes sense. Basically, you're not guaranteed to get anything
/// sensible out.
pub trait UncheckedFrom<T> {
	/// Convert from an instance of `T` to Self. This is not guaranteed to be
	/// whatever counts as a valid instance of `T` and it's up to the caller to
	/// ensure that it makes sense.
	fn unchecked_from(t: T) -> Self;
}

/// The counterpart to `UncheckedFrom`.
pub trait UncheckedInto<T> {
	/// The counterpart to `unchecked_from`.
	fn unchecked_into(self) -> T;
}

impl<S, T: UncheckedFrom<S>> UncheckedInto<T> for S {
	fn unchecked_into(self) -> T {
		T::unchecked_from(self)
	}
}

/// A store for sensitive data.
///
/// Calls `Zeroize::zeroize` upon `Drop`.
#[derive(Clone)]
pub struct Protected<T: Zeroize>(T);

impl<T: Zeroize> AsRef<T> for Protected<T> {
	fn as_ref(&self) -> &T {
		&self.0
	}
}

impl<T: Zeroize> sp_std::ops::Deref for Protected<T> {
	type Target = T;

	fn deref(&self) -> &T {
		&self.0
	}
}

#[cfg(feature = "std")]
impl<T: Zeroize> std::fmt::Debug for Protected<T> {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(fmt, "<protected>")
	}
}

impl<T: Zeroize> From<T> for Protected<T> {
	fn from(t: T) -> Self {
		Protected(t)
	}
}

impl<T: Zeroize> Zeroize for Protected<T> {
	fn zeroize(&mut self) {
		self.0.zeroize()
	}
}

impl<T: Zeroize> Drop for Protected<T> {
	fn drop(&mut self) {
		self.zeroize()
	}
}

/// An error with the interpretation of a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(feature = "full_crypto")]
pub enum SecretStringError {
	/// The overall format was invalid (e.g. the seed phrase contained symbols).
	InvalidFormat,
	/// The seed phrase provided is not a valid BIP39 phrase.
	InvalidPhrase,
	/// The supplied password was invalid.
	InvalidPassword,
	/// The seed is invalid (bad content).
	InvalidSeed,
	/// The seed has an invalid length.
	InvalidSeedLength,
	/// The derivation path was invalid (e.g. contains soft junctions when they are not supported).
	InvalidPath,
}

/// A since derivation junction description. It is the single parameter used when creating
/// a new secret key from an existing secret key and, in the case of `SoftRaw` and `SoftIndex`
/// a new public key from an existing public key.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Encode, Decode)]
#[cfg(feature = "full_crypto")]
pub enum DeriveJunction {
	/// Soft (vanilla) derivation. Public keys have a correspondent derivation.
	Soft([u8; JUNCTION_ID_LEN]),
	/// Hard ("hardened") derivation. Public keys do not have a correspondent derivation.
	Hard([u8; JUNCTION_ID_LEN]),
}

#[cfg(feature = "full_crypto")]
impl DeriveJunction {
	/// Consume self to return a soft derive junction with the same chain code.
	pub fn soften(self) -> Self { DeriveJunction::Soft(self.unwrap_inner()) }

	/// Consume self to return a hard derive junction with the same chain code.
	pub fn harden(self) -> Self { DeriveJunction::Hard(self.unwrap_inner()) }

	/// Create a new soft (vanilla) DeriveJunction from a given, encodable, value.
	///
	/// If you need a hard junction, use `hard()`.
	pub fn soft<T: Encode>(index: T) -> Self {
		let mut cc: [u8; JUNCTION_ID_LEN] = Default::default();
		index.using_encoded(|data| if data.len() > JUNCTION_ID_LEN {
			let hash_result = blake2_rfc::blake2b::blake2b(JUNCTION_ID_LEN, &[], data);
			let hash = hash_result.as_bytes();
			cc.copy_from_slice(hash);
		} else {
			cc[0..data.len()].copy_from_slice(data);
		});
		DeriveJunction::Soft(cc)
	}

	/// Create a new hard (hardened) DeriveJunction from a given, encodable, value.
	///
	/// If you need a soft junction, use `soft()`.
	pub fn hard<T: Encode>(index: T) -> Self {
		Self::soft(index).harden()
	}

	/// Consume self to return the chain code.
	pub fn unwrap_inner(self) -> [u8; JUNCTION_ID_LEN] {
		match self {
			DeriveJunction::Hard(c) | DeriveJunction::Soft(c) => c,
		}
	}

	/// Get a reference to the inner junction id.
	pub fn inner(&self) -> &[u8; JUNCTION_ID_LEN] {
		match self {
			DeriveJunction::Hard(ref c) | DeriveJunction::Soft(ref c) => c,
		}
	}

	/// Return `true` if the junction is soft.
	pub fn is_soft(&self) -> bool {
		match *self {
			DeriveJunction::Soft(_) => true,
			_ => false,
		}
	}

	/// Return `true` if the junction is hard.
	pub fn is_hard(&self) -> bool {
		match *self {
			DeriveJunction::Hard(_) => true,
			_ => false,
		}
	}
}

#[cfg(feature = "full_crypto")]
impl<T: AsRef<str>> From<T> for DeriveJunction {
	fn from(j: T) -> DeriveJunction {
		let j = j.as_ref();
		let (code, hard) = if j.starts_with("/") {
			(&j[1..], true)
		} else {
			(j, false)
		};

		let res = if let Ok(n) = str::parse::<u64>(code) {
			// number
			DeriveJunction::soft(n)
		} else {
			// something else
			DeriveJunction::soft(code)
		};

		if hard {
			res.harden()
		} else {
			res
		}
	}
}

/// An error type for SS58 decoding.
#[cfg(feature = "full_crypto")]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum PublicError {
	/// Bad alphabet.
	BadBase58,
	/// Bad length.
	BadLength,
	/// Unknown version.
	UnknownVersion,
	/// Invalid checksum.
	InvalidChecksum,
	/// Invalid format.
	InvalidFormat,
	/// Invalid derivation path.
	InvalidPath,
}

/// Key that can be encoded to/from SS58.
#[cfg(feature = "full_crypto")]
pub trait Ss58Codec: Sized + AsMut<[u8]> + AsRef<[u8]> + Default {
	/// Some if the string is a properly encoded SS58Check address.
	#[cfg(feature = "std")]
	fn from_ss58check(s: &str) -> Result<Self, PublicError> {
		Self::from_ss58check_with_version(s)
			.and_then(|(r, v)| match v {
				v if !v.is_custom() => Ok(r),
				v if v == *DEFAULT_VERSION.lock() => Ok(r),
				_ => Err(PublicError::UnknownVersion),
			})
	}
	/// Some if the string is a properly encoded SS58Check address.
	#[cfg(feature = "std")]
	fn from_ss58check_with_version(s: &str) -> Result<(Self, Ss58AddressFormat), PublicError> {
		let mut res = Self::default();
		let len = res.as_mut().len();
		let d = s.from_base58().map_err(|_| PublicError::BadBase58)?; // failure here would be invalid encoding.
		if d.len() != len + 3 {
			// Invalid length.
			return Err(PublicError::BadLength);
		}
		let ver = d[0].try_into().map_err(|_: ()| PublicError::UnknownVersion)?;

		if d[len + 1..len + 3] != ss58hash(&d[0..len + 1]).as_bytes()[0..2] {
			// Invalid checksum.
			return Err(PublicError::InvalidChecksum);
		}
		res.as_mut().copy_from_slice(&d[1..len + 1]);
		Ok((res, ver))
	}
	/// Some if the string is a properly encoded SS58Check address, optionally with
	/// a derivation path following.
	#[cfg(feature = "std")]
	fn from_string(s: &str) -> Result<Self, PublicError> {
		Self::from_string_with_version(s)
			.and_then(|(r, v)| match v {
				v if !v.is_custom() => Ok(r),
				v if v == *DEFAULT_VERSION.lock() => Ok(r),
				_ => Err(PublicError::UnknownVersion),
			})
	}

	/// Return the ss58-check string for this key.

	#[cfg(feature = "std")]
	fn to_ss58check_with_version(&self, version: Ss58AddressFormat) -> String {
		let mut v = vec![version.into()];
		v.extend(self.as_ref());
		let r = ss58hash(&v);
		v.extend(&r.as_bytes()[0..2]);
		v.to_base58()
	}
	/// Return the ss58-check string for this key.
	#[cfg(feature = "std")]
	fn to_ss58check(&self) -> String { self.to_ss58check_with_version(*DEFAULT_VERSION.lock()) }
	/// Some if the string is a properly encoded SS58Check address, optionally with
	/// a derivation path following.
	#[cfg(feature = "std")]
	fn from_string_with_version(s: &str) -> Result<(Self, Ss58AddressFormat), PublicError> {
		Self::from_ss58check_with_version(s)
	}
}

/// Derivable key trait.
pub trait Derive: Sized {
	/// Derive a child key from a series of given junctions.
	///
	/// Will be `None` for public keys if there are any hard junctions in there.
	#[cfg(feature = "std")]
	fn derive<Iter: Iterator<Item=DeriveJunction>>(&self, _path: Iter) -> Option<Self> {
		None
	}
}

#[cfg(feature = "std")]
const PREFIX: &[u8] = b"SS58PRE";

#[cfg(feature = "std")]
fn ss58hash(data: &[u8]) -> blake2_rfc::blake2b::Blake2bResult {
	let mut context = blake2_rfc::blake2b::Blake2b::new(64);
	context.update(PREFIX);
	context.update(data);
	context.finalize()
}

#[cfg(feature = "std")]
lazy_static::lazy_static! {
	static ref DEFAULT_VERSION: Mutex<Ss58AddressFormat>
		= Mutex::new(Ss58AddressFormat::SubstrateAccountDirect);
}

#[cfg(feature = "full_crypto")]
macro_rules! ss58_address_format {
	( $( $identifier:tt => ($number:expr, $name:expr, $desc:tt) )* ) => (
		/// A known address (sub)format/network ID for SS58.
		#[derive(Copy, Clone, PartialEq, Eq)]
		pub enum Ss58AddressFormat {
			$(#[doc = $desc] $identifier),*,
			/// Use a manually provided numeric value.
			Custom(u8),
		}

		static ALL_SS58_ADDRESS_FORMATS: [Ss58AddressFormat; 0 $(+ { let _ = $number; 1})*] = [
			$(Ss58AddressFormat::$identifier),*,
		];

		impl Ss58AddressFormat {
			/// All known address formats.
			pub fn all() -> &'static [Ss58AddressFormat] {
				&ALL_SS58_ADDRESS_FORMATS
			}

			/// Whether the address is custom.
			pub fn is_custom(&self) -> bool {
				match self {
					Self::Custom(_) => true,
					_ => false,
				}
			}
		}

		impl From<Ss58AddressFormat> for u8 {
			fn from(x: Ss58AddressFormat) -> u8 {
				match x {
					$(Ss58AddressFormat::$identifier => $number),*,
					Ss58AddressFormat::Custom(n) => n,
				}
			}
		}

		impl TryFrom<u8> for Ss58AddressFormat {
			type Error = ();

			fn try_from(x: u8) -> Result<Ss58AddressFormat, ()> {
				match x {
					$($number => Ok(Ss58AddressFormat::$identifier)),*,
					_ => Err(()),
				}
			}
		}

		impl<'a> TryFrom<&'a str> for Ss58AddressFormat {
			type Error = ();

			fn try_from(x: &'a str) -> Result<Ss58AddressFormat, ()> {
				match x {
					$($name => Ok(Ss58AddressFormat::$identifier)),*,
					a => a.parse::<u8>().map(Ss58AddressFormat::Custom).map_err(|_| ()),
				}
			}
		}

		#[cfg(feature = "std")]
		impl Default for Ss58AddressFormat {
			fn default() -> Self {
				*DEFAULT_VERSION.lock()
			}
		}

		#[cfg(feature = "std")]
		impl From<Ss58AddressFormat> for String {
			fn from(x: Ss58AddressFormat) -> String {
				match x {
					$(Ss58AddressFormat::$identifier => $name.into()),*,
					Ss58AddressFormat::Custom(x) => x.to_string(),
				}
			}
		}
	)
}

#[cfg(feature = "full_crypto")]
ss58_address_format!(
	SubstrateAccountDirect =>
		(42, "substrate", "Any Substrate network, direct checksum, standard account (*25519).")
	PolkadotAccountDirect =>
		(0, "polkadot", "Polkadot Relay-chain, direct checksum, standard account (*25519).")
	KusamaAccountDirect =>
		(2, "kusama", "Kusama Relay-chain, direct checksum, standard account (*25519).")
	EdgewareAccountDirect =>
		(7, "edgeware", "Edgeware mainnet, direct checksum, standard account (*25519).")
	KaruraAccountDirect =>
		(8, "karura", "Acala Karura canary network, direct checksum, standard account (*25519).")
	ReynoldsAccountDirect =>
		(9, "reynolds", "Laminar Reynolds canary network, direct checksum, standard account (*25519).")
	AcalaAccountDirect =>
		(10, "acala", "Acala mainnet, direct checksum, standard account (*25519).")
	LaminarAccountDirect =>
		(11, "laminar", "Laminar mainnet, direct checksum, standard account (*25519).")
	KulupuAccountDirect =>
		(16, "kulupu", "Kulupu mainnet, direct checksum, standard account (*25519).")
	DothereumAccountDirect =>
		(20, "dothereum", "Dothereum Para-chain, direct checksum, standard account (*25519).")
	CentrifugeAccountDirect =>
		(36, "centrifuge", "Centrifuge Chain mainnet, direct checksum, standard account (*25519).")
	SubstraTeeAccountDirect =>
		(44, "substratee", "Any SubstraTEE off-chain network private account, direct checksum, standard account (*25519).")
);

/// Set the default "version" (actually, this is a bit of a misnomer and the version byte is
/// typically used not just to encode format/version but also network identity) that is used for
/// encoding and decoding SS58 addresses. If an unknown version is provided then it fails.
///
/// See `ss58_address_format!` for all current known "versions".
#[cfg(feature = "std")]
pub fn set_default_ss58_version(version: Ss58AddressFormat) {
	*DEFAULT_VERSION.lock() = version
}

#[cfg(feature = "std")]
impl<T: Sized + AsMut<[u8]> + AsRef<[u8]> + Default + Derive> Ss58Codec for T {
	fn from_string(s: &str) -> Result<Self, PublicError> {
		let re = Regex::new(r"^(?P<ss58>[\w\d ]+)?(?P<path>(//?[^/]+)*)$")
			.expect("constructed from known-good static value; qed");
		let cap = re.captures(s).ok_or(PublicError::InvalidFormat)?;
		let re_junction = Regex::new(r"/(/?[^/]+)")
			.expect("constructed from known-good static value; qed");
		let s = cap.name("ss58")
			.map(|r| r.as_str())
			.unwrap_or(DEV_ADDRESS);
		let addr = if s.starts_with("0x") {
			let d = hex::decode(&s[2..]).map_err(|_| PublicError::InvalidFormat)?;
			let mut r = Self::default();
			if d.len() == r.as_ref().len() {
				r.as_mut().copy_from_slice(&d);
				r
			} else {
				Err(PublicError::BadLength)?
			}
		} else {
			Self::from_ss58check(s)?
		};
		if cap["path"].is_empty() {
			Ok(addr)
		} else {
			let path = re_junction.captures_iter(&cap["path"])
				.map(|f| DeriveJunction::from(&f[1]));
			addr.derive(path)
				.ok_or(PublicError::InvalidPath)
		}
	}

	fn from_string_with_version(s: &str) -> Result<(Self, Ss58AddressFormat), PublicError> {
		let re = Regex::new(r"^(?P<ss58>[\w\d ]+)?(?P<path>(//?[^/]+)*)$")
			.expect("constructed from known-good static value; qed");
		let cap = re.captures(s).ok_or(PublicError::InvalidFormat)?;
		let re_junction = Regex::new(r"/(/?[^/]+)")
			.expect("constructed from known-good static value; qed");
		let (addr, v) = Self::from_ss58check_with_version(
			cap.name("ss58")
				.map(|r| r.as_str())
				.unwrap_or(DEV_ADDRESS)
		)?;
		if cap["path"].is_empty() {
			Ok((addr, v))
		} else {
			let path = re_junction.captures_iter(&cap["path"])
				.map(|f| DeriveJunction::from(&f[1]));
			addr.derive(path)
				.ok_or(PublicError::InvalidPath)
				.map(|a| (a, v))
		}
	}
}

/// Trait suitable for typical cryptographic PKI key public type.
pub trait Public: AsRef<[u8]> + AsMut<[u8]> + Default + Derive + CryptoType + PartialEq + Eq + Clone + Send + Sync {
	/// A new instance from the given slice.
	///
	/// NOTE: No checking goes on to ensure this is a real public key. Only use it if
	/// you are certain that the array actually is a pubkey. GIGO!
	fn from_slice(data: &[u8]) -> Self;

	/// Return a `Vec<u8>` filled with raw data.
	fn to_raw_vec(&self) -> Vec<u8> { self.as_slice().to_vec() }

	/// Return a slice filled with raw data.
	fn as_slice(&self) -> &[u8] { self.as_ref() }
}

/// An opaque 32-byte cryptographic identifier.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Default, Encode, Decode)]
pub struct AccountId32([u8; 32]);

impl UncheckedFrom<crate::hash::H256> for AccountId32 {
	fn unchecked_from(h: crate::hash::H256) -> Self {
		AccountId32(h.into())
	}
}

#[cfg(feature = "std")]
impl Ss58Codec for AccountId32 {}

impl AsRef<[u8]> for AccountId32 {
	fn as_ref(&self) -> &[u8] {
		&self.0[..]
	}
}

impl AsMut<[u8]> for AccountId32 {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0[..]
	}
}

impl AsRef<[u8; 32]> for AccountId32 {
	fn as_ref(&self) -> &[u8; 32] {
		&self.0
	}
}

impl AsMut<[u8; 32]> for AccountId32 {
	fn as_mut(&mut self) -> &mut [u8; 32] {
		&mut self.0
	}
}

impl From<[u8; 32]> for AccountId32 {
	fn from(x: [u8; 32]) -> AccountId32 {
		AccountId32(x)
	}
}

impl<'a> sp_std::convert::TryFrom<&'a [u8]> for AccountId32 {
	type Error = ();
	fn try_from(x: &'a [u8]) -> Result<AccountId32, ()> {
		if x.len() == 32 {
			let mut r = AccountId32::default();
			r.0.copy_from_slice(x);
			Ok(r)
		} else {
			Err(())
		}
	}
}

impl From<AccountId32> for [u8; 32] {
	fn from(x: AccountId32) -> [u8; 32] {
		x.0
	}
}

#[cfg(feature = "std")]
impl std::fmt::Display for AccountId32 {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "{}", self.to_ss58check())
	}
}

impl sp_std::fmt::Debug for AccountId32 {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		let s = self.to_ss58check();
		write!(f, "{} ({}...)", crate::hexdisplay::HexDisplay::from(&self.0), &s[0..8])
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

#[cfg(feature = "std")]
impl serde::Serialize for AccountId32 {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: serde::Serializer {
		serializer.serialize_str(&self.to_ss58check())
	}
}

#[cfg(feature = "std")]
impl<'de> serde::Deserialize<'de> for AccountId32 {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: serde::Deserializer<'de> {
		Ss58Codec::from_ss58check(&String::deserialize(deserializer)?)
			.map_err(|e| serde::de::Error::custom(format!("{:?}", e)))
	}
}

#[cfg(feature = "std")]
pub use self::dummy::*;

#[cfg(feature = "std")]
mod dummy {
	use super::*;

	/// Dummy cryptography. Doesn't do anything.
	#[derive(Clone, Hash, Default, Eq, PartialEq)]
	pub struct Dummy;

	impl AsRef<[u8]> for Dummy {
		fn as_ref(&self) -> &[u8] { &b""[..] }
	}

	impl AsMut<[u8]> for Dummy {
		fn as_mut(&mut self) -> &mut[u8] {
			unsafe {
				#[allow(mutable_transmutes)]
				sp_std::mem::transmute::<_, &'static mut [u8]>(&b""[..])
			}
		}
	}

	impl CryptoType for Dummy {
		type Pair = Dummy;
	}

	impl Derive for Dummy {}

	impl Public for Dummy {
		fn from_slice(_: &[u8]) -> Self { Self }
		#[cfg(feature = "std")]
		fn to_raw_vec(&self) -> Vec<u8> { vec![] }
		fn as_slice(&self) -> &[u8] { b"" }
	}

	impl Pair for Dummy {
		type Public = Dummy;
		type Seed = Dummy;
		type Signature = Dummy;
		type DeriveError = ();
		#[cfg(feature = "std")]
		fn generate_with_phrase(_: Option<&str>) -> (Self, String, Self::Seed) { Default::default() }
		#[cfg(feature = "std")]
		fn from_phrase(_: &str, _: Option<&str>)
			-> Result<(Self, Self::Seed), SecretStringError>
		{
			Ok(Default::default())
		}
		fn derive<
			Iter: Iterator<Item=DeriveJunction>,
		>(&self, _: Iter, _: Option<Dummy>) -> Result<(Self, Option<Dummy>), Self::DeriveError> { Ok((Self, None)) }
		fn from_seed(_: &Self::Seed) -> Self { Self }
		fn from_seed_slice(_: &[u8]) -> Result<Self, SecretStringError> { Ok(Self) }
		fn sign(&self, _: &[u8]) -> Self::Signature { Self }
		fn verify<M: AsRef<[u8]>>(_: &Self::Signature, _: M, _: &Self::Public) -> bool { true }
		fn verify_weak<P: AsRef<[u8]>, M: AsRef<[u8]>>(_: &[u8], _: M, _: P) -> bool { true }
		fn public(&self) -> Self::Public { Self }
		fn to_raw_vec(&self) -> Vec<u8> { vec![] }
	}
}

/// Trait suitable for typical cryptographic PKI key pair type.
///
/// For now it just specifies how to create a key from a phrase and derivation path.
#[cfg(feature = "full_crypto")]
pub trait Pair: CryptoType + Sized + Clone + Send + Sync + 'static {
	/// The type which is used to encode a public key.
	type Public: Public + Hash;

	/// The type used to (minimally) encode the data required to securely create
	/// a new key pair.
	type Seed: Default + AsRef<[u8]> + AsMut<[u8]> + Clone;

	/// The type used to represent a signature. Can be created from a key pair and a message
	/// and verified with the message and a public key.
	type Signature: AsRef<[u8]>;

	/// Error returned from the `derive` function.
	type DeriveError;

	/// Generate new secure (random) key pair.
	///
	/// This is only for ephemeral keys really, since you won't have access to the secret key
	/// for storage. If you want a persistent key pair, use `generate_with_phrase` instead.
	#[cfg(feature = "std")]
	fn generate() -> (Self, Self::Seed) {
		let mut seed = Self::Seed::default();
		OsRng.fill_bytes(seed.as_mut());
		(Self::from_seed(&seed), seed)
	}

	/// Generate new secure (random) key pair and provide the recovery phrase.
	///
	/// You can recover the same key later with `from_phrase`.
	///
	/// This is generally slower than `generate()`, so prefer that unless you need to persist
	/// the key from the current session.
	#[cfg(feature = "std")]
	fn generate_with_phrase(password: Option<&str>) -> (Self, String, Self::Seed);

	/// Returns the KeyPair from the English BIP39 seed `phrase`, or `None` if it's invalid.
	#[cfg(feature = "std")]
	fn from_phrase(phrase: &str, password: Option<&str>) -> Result<(Self, Self::Seed), SecretStringError>;

	/// Derive a child key from a series of given junctions.
	fn derive<Iter: Iterator<Item=DeriveJunction>>(&self,
		path: Iter,
		seed: Option<Self::Seed>,
	) -> Result<(Self, Option<Self::Seed>), Self::DeriveError>;

	/// Generate new key pair from the provided `seed`.
	///
	/// @WARNING: THIS WILL ONLY BE SECURE IF THE `seed` IS SECURE. If it can be guessed
	/// by an attacker then they can also derive your key.
	fn from_seed(seed: &Self::Seed) -> Self;

	/// Make a new key pair from secret seed material. The slice must be the correct size or
	/// it will return `None`.
	///
	/// @WARNING: THIS WILL ONLY BE SECURE IF THE `seed` IS SECURE. If it can be guessed
	/// by an attacker then they can also derive your key.
	fn from_seed_slice(seed: &[u8]) -> Result<Self, SecretStringError>;

	/// Sign a message.
	fn sign(&self, message: &[u8]) -> Self::Signature;

	/// Verify a signature on a message. Returns true if the signature is good.
	fn verify<M: AsRef<[u8]>>(sig: &Self::Signature, message: M, pubkey: &Self::Public) -> bool;

	/// Verify a signature on a message. Returns true if the signature is good.
	fn verify_weak<P: AsRef<[u8]>, M: AsRef<[u8]>>(sig: &[u8], message: M, pubkey: P) -> bool;

	/// Get the public key.
	fn public(&self) -> Self::Public;

	/// Interprets the string `s` in order to generate a key Pair. Returns both the pair and an optional seed, in the
	/// case that the pair can be expressed as a direct derivation from a seed (some cases, such as Sr25519 derivations
	/// with path components, cannot).
	///
	/// This takes a helper function to do the key generation from a phrase, password and
	/// junction iterator.
	///
	/// - If `s` is a possibly `0x` prefixed 64-digit hex string, then it will be interpreted
	/// directly as a `MiniSecretKey` (aka "seed" in `subkey`).
	/// - If `s` is a valid BIP-39 key phrase of 12, 15, 18, 21 or 24 words, then the key will
	/// be derived from it. In this case:
	///   - the phrase may be followed by one or more items delimited by `/` characters.
	///   - the path may be followed by `///`, in which case everything after the `///` is treated
	/// as a password.
	/// - If `s` begins with a `/` character it is prefixed with the Substrate public `DEV_PHRASE` and
	/// interpreted as above.
	///
	/// In this case they are interpreted as HDKD junctions; purely numeric items are interpreted as
	/// integers, non-numeric items as strings. Junctions prefixed with `/` are interpreted as soft
	/// junctions, and with `//` as hard junctions.
	///
	/// There is no correspondence mapping between SURI strings and the keys they represent.
	/// Two different non-identical strings can actually lead to the same secret being derived.
	/// Notably, integer junction indices may be legally prefixed with arbitrary number of zeros.
	/// Similarly an empty password (ending the SURI with `///`) is perfectly valid and will generally
	/// be equivalent to no password at all.
	///
	/// `None` is returned if no matches are found.
	#[cfg(feature = "std")]
	fn from_string_with_seed(s: &str, password_override: Option<&str>)
		-> Result<(Self, Option<Self::Seed>), SecretStringError>
	{
		let re = Regex::new(r"^(?P<phrase>[\d\w ]+)?(?P<path>(//?[^/]+)*)(///(?P<password>.*))?$")
			.expect("constructed from known-good static value; qed");
		let cap = re.captures(s).ok_or(SecretStringError::InvalidFormat)?;

		let re_junction = Regex::new(r"/(/?[^/]+)")
			.expect("constructed from known-good static value; qed");
		let path = re_junction.captures_iter(&cap["path"])
			.map(|f| DeriveJunction::from(&f[1]));

		let phrase = cap.name("phrase").map(|r| r.as_str()).unwrap_or(DEV_PHRASE);
		let password = password_override.or_else(|| cap.name("password").map(|m| m.as_str()));

		let (root, seed) = if phrase.starts_with("0x") {
			hex::decode(&phrase[2..]).ok()
				.and_then(|seed_vec| {
					let mut seed = Self::Seed::default();
					if seed.as_ref().len() == seed_vec.len() {
						seed.as_mut().copy_from_slice(&seed_vec);
						Some((Self::from_seed(&seed), seed))
					} else {
						None
					}
				})
				.ok_or(SecretStringError::InvalidSeed)?
		} else {
			Self::from_phrase(phrase, password)
				.map_err(|_| SecretStringError::InvalidPhrase)?
		};
		root.derive(path, Some(seed)).map_err(|_| SecretStringError::InvalidPath)
	}

	/// Interprets the string `s` in order to generate a key pair.
	///
	/// See [`from_string_with_seed`](Self::from_string_with_seed) for more extensive documentation.
	#[cfg(feature = "std")]
	fn from_string(s: &str, password_override: Option<&str>) -> Result<Self, SecretStringError> {
		Self::from_string_with_seed(s, password_override).map(|x| x.0)
	}

	/// Return a vec filled with raw data.
	fn to_raw_vec(&self) -> Vec<u8>;
}

/// One type is wrapped by another.
pub trait IsWrappedBy<Outer>: From<Outer> + Into<Outer> {
	/// Get a reference to the inner from the outer.
	fn from_ref(outer: &Outer) -> &Self;
	/// Get a mutable reference to the inner from the outer.
	fn from_mut(outer: &mut Outer) -> &mut Self;
}

/// Opposite of `IsWrappedBy` - denotes a type which is a simple wrapper around another type.
pub trait Wraps: Sized {
	/// The inner type it is wrapping.
	type Inner: IsWrappedBy<Self>;
}

impl<T, Outer> IsWrappedBy<Outer> for T where
	Outer: AsRef<Self> + AsMut<Self> + From<Self>,
	T: From<Outer>,
{
	/// Get a reference to the inner from the outer.
	fn from_ref(outer: &Outer) -> &Self { outer.as_ref() }

	/// Get a mutable reference to the inner from the outer.
	fn from_mut(outer: &mut Outer) -> &mut Self { outer.as_mut() }
}

impl<Inner, Outer, T> UncheckedFrom<T> for Outer where
	Outer: Wraps<Inner=Inner>,
	Inner: IsWrappedBy<Outer> + UncheckedFrom<T>,
{
	fn unchecked_from(t: T) -> Self {
		let inner: Inner = t.unchecked_into();
		inner.into()
	}
}

/// Type which has a particular kind of crypto associated with it.
pub trait CryptoType {
	/// The pair key type of this crypto.
	#[cfg(feature = "full_crypto")]
	type Pair: Pair;
}

/// An identifier for a type of cryptographic key.
///
/// To avoid clashes with other modules when distributing your module publicly, register your
/// `KeyTypeId` on the list here by making a PR.
///
/// Values whose first character is `_` are reserved for private use and won't conflict with any
/// public modules.
#[derive(
	Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode, PassByInner,
	crate::RuntimeDebug
)]
pub struct KeyTypeId(pub [u8; 4]);

impl From<u32> for KeyTypeId {
	fn from(x: u32) -> Self {
		Self(x.to_le_bytes())
	}
}

impl From<KeyTypeId> for u32 {
	fn from(x: KeyTypeId) -> Self {
		u32::from_le_bytes(x.0)
	}
}

impl<'a> TryFrom<&'a str> for KeyTypeId {
	type Error = ();
	fn try_from(x: &'a str) -> Result<Self, ()> {
		let b = x.as_bytes();
		if b.len() != 4 {
			return Err(());
		}
		let mut res = KeyTypeId::default();
		res.0.copy_from_slice(&b[0..4]);
		Ok(res)
	}
}

/// Known key types; this also functions as a global registry of key types for projects wishing to
/// avoid collisions with each other.
///
/// It's not universal in the sense that *all* key types need to be mentioned here, it's just a
/// handy place to put common key types.
pub mod key_types {
	use super::KeyTypeId;

	/// Key type for Babe module, build-in.
	pub const BABE: KeyTypeId = KeyTypeId(*b"babe");
	/// Key type for Grandpa module, build-in.
	pub const GRANDPA: KeyTypeId = KeyTypeId(*b"gran");
	/// Key type for controlling an account in a Substrate runtime, built-in.
	pub const ACCOUNT: KeyTypeId = KeyTypeId(*b"acco");
	/// Key type for Aura module, built-in.
	pub const AURA: KeyTypeId = KeyTypeId(*b"aura");
	/// Key type for ImOnline module, built-in.
	pub const IM_ONLINE: KeyTypeId = KeyTypeId(*b"imon");
	/// Key type for AuthorityDiscovery module, built-in.
	pub const AUTHORITY_DISCOVERY: KeyTypeId = KeyTypeId(*b"audi");
	/// A key type ID useful for tests.
	pub const DUMMY: KeyTypeId = KeyTypeId(*b"dumy");
}

#[cfg(test)]
mod tests {
	use crate::DeriveJunction;
	use hex_literal::hex;
	use super::*;

	#[derive(Clone, Eq, PartialEq, Debug)]
	enum TestPair {
		Generated,
		GeneratedWithPhrase,
		GeneratedFromPhrase{phrase: String, password: Option<String>},
		Standard{phrase: String, password: Option<String>, path: Vec<DeriveJunction>},
		Seed(Vec<u8>),
	}
	impl Default for TestPair {
		fn default() -> Self {
			TestPair::Generated
		}
	}
	impl CryptoType for TestPair {
		type Pair = Self;
	}

	#[derive(Clone, PartialEq, Eq, Hash, Default)]
	struct TestPublic;
	impl AsRef<[u8]> for TestPublic {
		fn as_ref(&self) -> &[u8] {
			&[]
		}
	}
	impl AsMut<[u8]> for TestPublic {
		fn as_mut(&mut self) -> &mut [u8] {
			&mut []
		}
	}
	impl CryptoType for TestPublic {
		type Pair = TestPair;
	}
	impl Derive for TestPublic {}
	impl Public for TestPublic {
		fn from_slice(_bytes: &[u8]) -> Self {
			Self
		}
		fn as_slice(&self) -> &[u8] {
			&[]
		}
		fn to_raw_vec(&self) -> Vec<u8> {
			vec![]
		}
	}
	impl Pair for TestPair {
		type Public = TestPublic;
		type Seed = [u8; 8];
		type Signature = [u8; 0];
		type DeriveError = ();

		fn generate() -> (Self, <Self as Pair>::Seed) { (TestPair::Generated, [0u8; 8]) }
		fn generate_with_phrase(_password: Option<&str>) -> (Self, String, <Self as Pair>::Seed) {
			(TestPair::GeneratedWithPhrase, "".into(), [0u8; 8])
		}
		fn from_phrase(phrase: &str, password: Option<&str>)
			-> Result<(Self, <Self as Pair>::Seed), SecretStringError>
		{
			Ok((TestPair::GeneratedFromPhrase {
				phrase: phrase.to_owned(),
				password: password.map(Into::into)
			}, [0u8; 8]))
		}
		fn derive<Iter: Iterator<Item=DeriveJunction>>(&self, path_iter: Iter, _: Option<[u8; 8]>)
			-> Result<(Self, Option<[u8; 8]>), Self::DeriveError>
		{
			Ok((match self.clone() {
				TestPair::Standard {phrase, password, path} =>
					TestPair::Standard { phrase, password, path: path.into_iter().chain(path_iter).collect() },
				TestPair::GeneratedFromPhrase {phrase, password} =>
					TestPair::Standard { phrase, password, path: path_iter.collect() },
				x => if path_iter.count() == 0 { x } else { return Err(()) },
			}, None))
		}
		fn from_seed(_seed: &<TestPair as Pair>::Seed) -> Self { TestPair::Seed(_seed.as_ref().to_owned()) }
		fn sign(&self, _message: &[u8]) -> Self::Signature { [] }
		fn verify<M: AsRef<[u8]>>(_: &Self::Signature, _: M, _: &Self::Public) -> bool { true }
		fn verify_weak<P: AsRef<[u8]>, M: AsRef<[u8]>>(
			_sig: &[u8],
			_message: M,
			_pubkey: P
		) -> bool { true }
		fn public(&self) -> Self::Public { TestPublic }
		fn from_seed_slice(seed: &[u8])
			-> Result<Self, SecretStringError>
		{
			Ok(TestPair::Seed(seed.to_owned()))
		}
		fn to_raw_vec(&self) -> Vec<u8> {
			vec![]
		}
	}

	#[test]
	fn interpret_std_seed_should_work() {
		assert_eq!(
			TestPair::from_string("0x0123456789abcdef", None),
			Ok(TestPair::Seed(hex!["0123456789abcdef"][..].to_owned()))
		);
	}

	#[test]
	fn password_override_should_work() {
		assert_eq!(
			TestPair::from_string("hello world///password", None),
			TestPair::from_string("hello world", Some("password")),
		);
		assert_eq!(
			TestPair::from_string("hello world///password", None),
			TestPair::from_string("hello world///other password", Some("password")),
		);
	}

	#[test]
	fn interpret_std_secret_string_should_work() {
		assert_eq!(
			TestPair::from_string("hello world", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: None, path: vec![]})
		);
		assert_eq!(
			TestPair::from_string("hello world/1", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: None, path: vec![DeriveJunction::soft(1)]})
		);
		assert_eq!(
			TestPair::from_string("hello world/DOT", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: None, path: vec![DeriveJunction::soft("DOT")]})
		);
		assert_eq!(
			TestPair::from_string("hello world//1", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: None, path: vec![DeriveJunction::hard(1)]})
		);
		assert_eq!(
			TestPair::from_string("hello world//DOT", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: None, path: vec![DeriveJunction::hard("DOT")]})
		);
		assert_eq!(
			TestPair::from_string("hello world//1/DOT", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: None, path: vec![DeriveJunction::hard(1), DeriveJunction::soft("DOT")]})
		);
		assert_eq!(
			TestPair::from_string("hello world//DOT/1", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: None, path: vec![DeriveJunction::hard("DOT"), DeriveJunction::soft(1)]})
		);
		assert_eq!(
			TestPair::from_string("hello world///password", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: Some("password".to_owned()), path: vec![]})
		);
		assert_eq!(
			TestPair::from_string("hello world//1/DOT///password", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: Some("password".to_owned()), path: vec![DeriveJunction::hard(1), DeriveJunction::soft("DOT")]})
		);
		assert_eq!(
			TestPair::from_string("hello world/1//DOT///password", None),
			Ok(TestPair::Standard{phrase: "hello world".to_owned(), password: Some("password".to_owned()), path: vec![DeriveJunction::soft(1), DeriveJunction::hard("DOT")]})
		);
	}
}
