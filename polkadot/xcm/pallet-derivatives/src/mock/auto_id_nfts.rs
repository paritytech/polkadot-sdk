use super::*;

pub type PredefinedIdNftsInstance = unique_items::Instance3;
pub type NftLocalId = u64;
pub type NftFullId = (CollectionAutoId, NftLocalId);
impl unique_items::Config<PredefinedIdNftsInstance> for Test {
	type ItemId = NftFullId;
}

// Below are the operations implementations:
// * NFT creation (with a predefined ID)
// * NFT transfer from one account to another (checks if the `from` account is the NFT's current
//   owner)

impl Create<WithConfig<ConfigValue<Owner<AccountId>>, PredefinedId<NftFullId>>>
	for PredefinedIdNfts
{
	fn create(
		strategy: WithConfig<ConfigValue<Owner<AccountId>>, PredefinedId<NftFullId>>,
	) -> Result<NftFullId, DispatchError> {
		let WithConfig { config: ConfigValue(owner), extra: id_assignment } = strategy;
		let id = id_assignment.params;

		unique_items::ItemOwner::<Test, PredefinedIdNftsInstance>::try_mutate(
			id.clone(),
			|current_owner| {
				if current_owner.is_none() {
					*current_owner = Some(owner);
					Ok(())
				} else {
					Err(unique_items::Error::<Test, PredefinedIdNftsInstance>::AlreadyExists)
				}
			},
		)?;

		Ok(id)
	}
}
impl AssetDefinition for PredefinedIdNfts {
	type Id = (CollectionAutoId, NftLocalId);
}
impl Update<ChangeOwnerFrom<AccountId>> for PredefinedIdNfts {
	fn update(
		id: &Self::Id,
		strategy: ChangeOwnerFrom<AccountId>,
		new_owner: &AccountId,
	) -> DispatchResult {
		let CheckState(check_owner, _) = strategy;

		unique_items::ItemOwner::<Test, PredefinedIdNftsInstance>::try_mutate(id, |owner| {
			match owner {
				Some(current_owner) =>
					if *current_owner == check_owner {
						*owner = Some(*new_owner);
						Ok(())
					} else {
						Err(unique_items::Error::<Test, PredefinedIdNftsInstance>::NoPermission
							.into())
					},
				None =>
					Err(unique_items::Error::<Test, PredefinedIdNftsInstance>::UnknownItem.into()),
			}
		})
	}
}
