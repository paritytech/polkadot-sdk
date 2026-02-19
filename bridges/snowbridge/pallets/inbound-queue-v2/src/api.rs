use crate::Config;
use crate::Nonce;
use snowbridge_core::sparse_bitmap::SparseBitmap;

pub fn is_message_relayed<T>(nonce: u64) -> bool
where
	T: Config,
{
	return Nonce::<T>::get(nonce);
}
