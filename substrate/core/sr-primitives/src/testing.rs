// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

//! Testing utilities.

use serde::{Serialize, Serializer, Deserialize, de::Error as DeError, Deserializer};
use std::{fmt::Debug, ops::Deref, fmt, cell::RefCell};
use crate::codec::{Codec, Encode, Decode};
use crate::traits::{
	self, Checkable, Applyable, BlakeTwo256, OpaqueKeys, ValidateUnsigned,
	SignedExtension, Dispatchable,
};
use crate::{generic, KeyTypeId, ApplyResult};
use crate::weights::{GetDispatchInfo, DispatchInfo};
pub use primitives::{H256, sr25519};
use primitives::{crypto::{CryptoType, Dummy, key_types, Public}, U256};
use crate::transaction_validity::{TransactionValidity, TransactionValidityError};

/// Authority Id
#[derive(Default, PartialEq, Eq, Clone, Encode, Decode, Debug, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct UintAuthorityId(pub u64);

impl From<u64> for UintAuthorityId {
	fn from(id: u64) -> Self {
		UintAuthorityId(id)
	}
}

impl From<UintAuthorityId> for u64 {
	fn from(id: UintAuthorityId) -> u64 {
		id.0
	}
}

impl UintAuthorityId {
	/// Convert this authority id into a public key.
	pub fn to_public_key<T: Public>(&self) -> T {
		let bytes: [u8; 32] = U256::from(self.0).into();
		T::from_slice(&bytes)
	}
}

impl CryptoType for UintAuthorityId {
	type Pair = Dummy;
}

impl AsRef<[u8]> for UintAuthorityId {
	fn as_ref(&self) -> &[u8] {
		// Unsafe, i know, but it's test code and it's just there because it's really convenient to
		// keep `UintAuthorityId` as a u64 under the hood.
		unsafe {
			std::slice::from_raw_parts(&self.0 as *const u64 as *const _, std::mem::size_of::<u64>())
		}
	}
}

thread_local! {
	/// A list of all UintAuthorityId keys returned to the runtime.
	static ALL_KEYS: RefCell<Vec<UintAuthorityId>> = RefCell::new(vec![]);
}

impl UintAuthorityId {
	/// Set the list of keys returned by the runtime call for all keys of that type.
	pub fn set_all_keys<T: Into<UintAuthorityId>>(keys: impl IntoIterator<Item=T>) {
		ALL_KEYS.with(|l| *l.borrow_mut() = keys.into_iter().map(Into::into).collect())
	}
}

impl app_crypto::RuntimeAppPublic for UintAuthorityId {
	const ID: KeyTypeId = key_types::DUMMY;

	type Signature = u64;

	fn all() -> Vec<Self> {
		ALL_KEYS.with(|l| l.borrow().clone())
	}

	fn generate_pair(_: Option<&str>) -> Self {
		use rand::RngCore;
		UintAuthorityId(rand::thread_rng().next_u64())
	}

	fn sign<M: AsRef<[u8]>>(&self, msg: &M) -> Option<Self::Signature> {
		let mut signature = [0u8; 8];
		msg.as_ref().iter()
			.chain(std::iter::repeat(&42u8))
			.take(8)
			.enumerate()
			.for_each(|(i, v)| { signature[i] = *v; });

		Some(u64::from_le_bytes(signature))
	}

	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool {
		let mut msg_signature = [0u8; 8];
		msg.as_ref().iter()
			.chain(std::iter::repeat(&42))
			.take(8)
			.enumerate()
			.for_each(|(i, v)| { msg_signature[i] = *v; });

		u64::from_le_bytes(msg_signature) == *signature
	}
}

impl OpaqueKeys for UintAuthorityId {
	type KeyTypeIds = std::iter::Cloned<std::slice::Iter<'static, KeyTypeId>>;

	fn key_ids() -> Self::KeyTypeIds {
		[key_types::DUMMY].iter().cloned()
	}

	fn get_raw(&self, _: KeyTypeId) -> &[u8] {
		self.as_ref()
	}

	fn get<T: Decode>(&self, _: KeyTypeId) -> Option<T> {
		self.using_encoded(|mut x| T::decode(&mut x)).ok()
	}
}

/// Digest item
pub type DigestItem = generic::DigestItem<H256>;

/// Header Digest
pub type Digest = generic::Digest<H256>;

/// Block Header
#[derive(PartialEq, Eq, Clone, Serialize, Debug, Encode, Decode)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Header {
	/// Parent hash
	pub parent_hash: H256,
	/// Block Number
	pub number: u64,
	/// Post-execution state trie root
	pub state_root: H256,
	/// Merkle root of block's extrinsics
	pub extrinsics_root: H256,
	/// Digest items
	pub digest: Digest,
}

impl traits::Header for Header {
	type Number = u64;
	type Hashing = BlakeTwo256;
	type Hash = H256;

	fn number(&self) -> &Self::Number { &self.number }
	fn set_number(&mut self, num: Self::Number) { self.number = num }

	fn extrinsics_root(&self) -> &Self::Hash { &self.extrinsics_root }
	fn set_extrinsics_root(&mut self, root: Self::Hash) { self.extrinsics_root = root }

	fn state_root(&self) -> &Self::Hash { &self.state_root }
	fn set_state_root(&mut self, root: Self::Hash) { self.state_root = root }

	fn parent_hash(&self) -> &Self::Hash { &self.parent_hash }
	fn set_parent_hash(&mut self, hash: Self::Hash) { self.parent_hash = hash }

	fn digest(&self) -> &Digest { &self.digest }
	fn digest_mut(&mut self) -> &mut Digest { &mut self.digest }

	fn new(
		number: Self::Number,
		extrinsics_root: Self::Hash,
		state_root: Self::Hash,
		parent_hash: Self::Hash,
		digest: Digest,
	) -> Self {
		Header {
			number,
			extrinsics_root,
			state_root,
			parent_hash,
			digest,
		}
	}
}

impl<'a> Deserialize<'a> for Header {
	fn deserialize<D: Deserializer<'a>>(de: D) -> Result<Self, D::Error> {
		let r = <Vec<u8>>::deserialize(de)?;
		Decode::decode(&mut &r[..])
			.map_err(|e| DeError::custom(format!("Invalid value passed into decode: {}", e.what())))
	}
}

/// An opaque extrinsic wrapper type.
#[derive(PartialEq, Eq, Clone, Debug, Encode, Decode)]
pub struct ExtrinsicWrapper<Xt>(Xt);

impl<Xt> traits::Extrinsic for ExtrinsicWrapper<Xt> {
	type Call = ();
	type SignaturePayload = ();

	fn is_signed(&self) -> Option<bool> {
		None
	}
}

impl<Xt: Encode> serde::Serialize for ExtrinsicWrapper<Xt> {
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error> where S: ::serde::Serializer {
		self.using_encoded(|bytes| seq.serialize_bytes(bytes))
	}
}

impl<Xt> From<Xt> for ExtrinsicWrapper<Xt> {
	fn from(xt: Xt) -> Self {
		ExtrinsicWrapper(xt)
	}
}

impl<Xt> Deref for ExtrinsicWrapper<Xt> {
	type Target = Xt;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

/// Testing block
#[derive(PartialEq, Eq, Clone, Serialize, Debug, Encode, Decode)]
pub struct Block<Xt> {
	/// Block header
	pub header: Header,
	/// List of extrinsics
	pub extrinsics: Vec<Xt>,
}

impl<Xt: 'static + Codec + Sized + Send + Sync + Serialize + Clone + Eq + Debug + traits::Extrinsic> traits::Block for Block<Xt> {
	type Extrinsic = Xt;
	type Header = Header;
	type Hash = <Header as traits::Header>::Hash;

	fn header(&self) -> &Self::Header {
		&self.header
	}
	fn extrinsics(&self) -> &[Self::Extrinsic] {
		&self.extrinsics[..]
	}
	fn deconstruct(self) -> (Self::Header, Vec<Self::Extrinsic>) {
		(self.header, self.extrinsics)
	}
	fn new(header: Self::Header, extrinsics: Vec<Self::Extrinsic>) -> Self {
		Block { header, extrinsics }
	}
}

impl<'a, Xt> Deserialize<'a> for Block<Xt> where Block<Xt>: Decode {
	fn deserialize<D: Deserializer<'a>>(de: D) -> Result<Self, D::Error> {
		let r = <Vec<u8>>::deserialize(de)?;
		Decode::decode(&mut &r[..])
			.map_err(|e| DeError::custom(format!("Invalid value passed into decode: {}", e.what())))
	}
}

/// Test transaction, tuple of (sender, call, signed_extra)
/// with index only used if sender is some.
///
/// If sender is some then the transaction is signed otherwise it is unsigned.
#[derive(PartialEq, Eq, Clone, Encode, Decode)]
pub struct TestXt<Call, Extra>(pub Option<(u64, Extra)>, pub Call);

impl<Call, Extra> Serialize for TestXt<Call, Extra> where TestXt<Call, Extra>: Encode {
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error> where S: Serializer {
		self.using_encoded(|bytes| seq.serialize_bytes(bytes))
	}
}

impl<Call, Extra> Debug for TestXt<Call, Extra> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "TestXt({:?}, ...)", self.0.as_ref().map(|x| &x.0))
	}
}

impl<Call: Codec + Sync + Send, Context, Extra> Checkable<Context> for TestXt<Call, Extra> {
	type Checked = Self;
	fn check(self, _: &Context) -> Result<Self::Checked, TransactionValidityError> { Ok(self) }
}
impl<Call: Codec + Sync + Send, Extra> traits::Extrinsic for TestXt<Call, Extra> {
	type Call = Call;
	type SignaturePayload = (u64, Extra);

	fn is_signed(&self) -> Option<bool> {
		Some(self.0.is_some())
	}

	fn new(c: Call, sig: Option<Self::SignaturePayload>) -> Option<Self> {
		Some(TestXt(sig, c))
	}
}

impl<Origin, Call, Extra> Applyable for TestXt<Call, Extra> where
	Call: 'static + Sized + Send + Sync + Clone + Eq + Codec + Debug + Dispatchable<Origin=Origin>,
	Extra: SignedExtension<AccountId=u64, Call=Call>,
	Origin: From<Option<u64>>,
{
	type AccountId = u64;
	type Call = Call;

	fn sender(&self) -> Option<&Self::AccountId> { self.0.as_ref().map(|x| &x.0) }

	/// Checks to see if this is a valid *transaction*. It returns information on it if so.
	fn validate<U: ValidateUnsigned<Call=Self::Call>>(
		&self,
		_info: DispatchInfo,
		_len: usize,
	) -> TransactionValidity {
		Ok(Default::default())
	}

	/// Executes all necessary logic needed prior to dispatch and deconstructs into function call,
	/// index and sender.
	fn apply(
		self,
		info: DispatchInfo,
		len: usize,
	) -> ApplyResult {
		let maybe_who = if let Some((who, extra)) = self.0 {
			Extra::pre_dispatch(extra, &who, &self.1, info, len)?;
			Some(who)
		} else {
			Extra::pre_dispatch_unsigned(&self.1, info, len)?;
			None
		};

		Ok(self.1.dispatch(maybe_who.into()).map_err(Into::into))
	}
}

impl<Call: Encode, Extra: Encode> GetDispatchInfo for TestXt<Call, Extra> {
	fn get_dispatch_info(&self) -> DispatchInfo {
		// for testing: weight == size.
		DispatchInfo {
			weight: self.encode().len() as _,
			..Default::default()
		}
	}
}
