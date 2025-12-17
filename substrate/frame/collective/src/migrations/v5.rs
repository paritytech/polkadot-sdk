use super::super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
    migrations::{SteppedMigration, SteppedMigrationError},
    pallet_prelude::*,
    traits::{StorePreimage},
    weights::WeightMeter,
};
use scale_info::TypeInfo;

#[frame_support::storage_alias]
pub type OldProposalOf<T: Config<I>, I: 'static> =
StorageMap<Pallet<T, I>, Identity, <T as frame_system::Config>::Hash, <T as Config<I>>::Proposal>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
pub enum Cursor {
    MigrateProposals(u32),
    ClearStorage,
}

pub struct MigrateToV5<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> SteppedMigration for MigrateToV5<T, I> {
    type Cursor = Cursor;
    type Identifier = [u8; 32];

    fn id() -> Self::Identifier {
        *b"CollectiveMigrationV5___________"
    }

    fn step(
        cursor: Option<Self::Cursor>,
        meter: &mut WeightMeter,
    ) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
        let cursor = cursor.unwrap_or(Cursor::MigrateProposals(0));

        match cursor {
            Cursor::MigrateProposals(index) => {
                let proposals = Proposals::<T, I>::get();
                let count = proposals.len() as u32;

                let mut current_index = index;

                let weight_per_item = T::DbWeight::get().reads_writes(2, 2);

                while current_index < count {
                    if meter.try_consume(weight_per_item).is_err() {
                        return Ok(Some(Cursor::MigrateProposals(current_index)));
                    }

                    let hash = proposals[current_index as usize];
                    if let Some(proposal) = OldProposalOf::<T, I>::get(hash) {
                        let _ = T::Preimages::note(proposal.encode().into());
                        OldProposalOf::<T, I>::remove(hash);
                    }

                    current_index += 1;
                }

                Ok(Some(Cursor::ClearStorage))
            },
            Cursor::ClearStorage => {
                let limit = 100u32;
                let result = OldProposalOf::<T, I>::clear(limit, None);

                let weight = T::DbWeight::get().reads_writes(1, 1).saturating_mul(result.loops as u64);
                meter.consume(weight);

                if result.maybe_cursor.is_some() {
                    Ok(Some(Cursor::ClearStorage))
                } else {
                    Ok(None)
                }
            }
        }
    }
}