use crate::{Config, MoveGlobalStorage, LOG_TARGET};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{ConstU32, TypeInfo},
	BoundedVec,
};
use log::{debug, error, warn};

#[derive(Hash, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub struct MoveAddress([u8; 32]);

#[derive(Debug, Hash, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub struct StructTagHash([u8; 32]);

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub struct GlobalResourceEntry {
	/// The serialized resource contents (Move struct instance).
	pub data: BoundedVec<u8, ConstU32<2048>>,

	/// Number of active shared borrows (`&T`).
	pub borrow_count: u32,

	/// True if there's an active mutable borrow (`&mut T`).
	pub borrow_mut: bool,
}

impl GlobalResourceEntry {
	pub fn new(data: alloc::vec::Vec<u8>) -> Option<Self> {
		BoundedVec::try_from(data).ok().map(|bv| Self {
			data: bv,
			borrow_count: 0,
			borrow_mut: false,
		})
	}
}

pub fn store<T: Config>(address: [u8; 32], tag: [u8; 32], guest_data: alloc::vec::Vec<u8>) {
	let bounded_data = BoundedVec::<u8, ConstU32<2048>>::try_from(guest_data.clone())
		.expect("failed to convert to BoundedVec");
	let entry = GlobalResourceEntry { data: bounded_data, borrow_count: 0, borrow_mut: false };
	let move_address = MoveAddress(address);
	let struct_tag_hash = StructTagHash(tag);

	MoveGlobalStorage::<T>::insert(&move_address, &struct_tag_hash, entry);
}

pub fn load<T: Config>(
	address: [u8; 32],
	tag: [u8; 32],
	remove: bool,
	is_mut: bool,
) -> alloc::vec::Vec<u8> {
	let move_address = MoveAddress(address);
	let struct_tag_hash = StructTagHash(tag);
	if let Some(entry) = MoveGlobalStorage::<T>::get(&move_address, &struct_tag_hash) {
		let value: alloc::vec::Vec<u8> = entry.data.to_vec();
		if remove {
			MoveGlobalStorage::<T>::remove(&move_address, &struct_tag_hash);
		} else {
			MoveGlobalStorage::<T>::mutate(&move_address, &struct_tag_hash, |maybe_entry| {
				if let Some(mut_entry) = maybe_entry {
					if mut_entry.borrow_mut {
						panic!("mutable borrow already exists for global at {address:x?} with type {tag:x?}");
					}
					if is_mut {
						if mut_entry.borrow_count > 0 {
							panic!("cannot create mutable borrow for global at {address:x?} with type {tag:x?} while there are active shared borrows");
						}
						mut_entry.borrow_mut = true;
					}
					mut_entry.borrow_count = mut_entry.borrow_count.saturating_add(1);
					debug!(target: LOG_TARGET, "entry: {maybe_entry:x?}");
				}
			});
		}
		return value;
	} else {
		panic!("missing global at {address:x?} {tag:x?}");
	}
}

pub fn exists<T: Config>(address: [u8; 32], tag: alloc::vec::Vec<u8>) -> bool {
	let move_address = MoveAddress(address);
	let struct_tag_hash = StructTagHash(tag.try_into().expect("expected 32 bytes"));
	MoveGlobalStorage::<T>::contains_key(&move_address, &struct_tag_hash)
}

pub fn update<T: Config>(address: [u8; 32], tag: [u8; 32], new_data: alloc::vec::Vec<u8>) {
	let move_address = MoveAddress(address);
	let struct_tag_hash = StructTagHash(tag);

	MoveGlobalStorage::<T>::mutate(&move_address, &struct_tag_hash, |maybe_entry| {
		if let Some(entry) = maybe_entry {
			if entry.borrow_mut {
				let bounded_data = BoundedVec::<u8, ConstU32<2048>>::try_from(new_data.clone())
					.expect("failed to convert to BoundedVec");
				entry.data = bounded_data;
				debug!(target: LOG_TARGET, "entry: {entry:x?}");
			}
		} else {
			panic!("missing global at {address:x?} {tag:x?}");
		}
	});
}

pub fn release<T: Config>(address: [u8; 32], tag: [u8; 32]) {
	let move_address = MoveAddress(address);
	let struct_tag_hash = StructTagHash(tag);

	MoveGlobalStorage::<T>::mutate(&move_address, &struct_tag_hash, |maybe_entry| {
		if let Some(entry) = maybe_entry {
			if entry.borrow_mut {
				// If there's a mutable borrow, we can release it
				debug!(target: LOG_TARGET, "Released mutable borrow for global at {address:x?} with type {struct_tag_hash:x?}");
				entry.borrow_mut = false;
			}
			if entry.borrow_count > 0 {
				// If there are shared borrows, we just decrement the count
				debug!(target: LOG_TARGET, "Decremented borrow count for global at {address:x?} with type {struct_tag_hash:x?}");
				entry.borrow_count = entry.borrow_count.saturating_sub(1);
			} else {
				// No active borrows, nothing to do
				error!(target: LOG_TARGET, "No active borrows to release for global at {address:x?} with type {struct_tag_hash:x?}");
			}
			debug!(target: LOG_TARGET, "entry: {entry:x?}");
		} else {
			warn!(target: LOG_TARGET, "Tried to borrow missing global at {:x?} {:x?}", address, struct_tag_hash);
		}
	});
}
