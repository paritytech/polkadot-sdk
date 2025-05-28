use super::*;

pub type AutoIdCollectionsInstance = unique_items::Instance2;
pub type CollectionAutoId = u64;
impl unique_items::Config<unique_items::Instance2> for Test {
	type ItemId = CollectionAutoId;
}

// Below are the operations implementations:
// * collection creation (with an automatically assigned ID)
// * collection destruction

impl Create<WithConfig<ConfigValue<Owner<AccountId>>, AutoId<CollectionAutoId>>>
	for AutoIdCollections
{
	fn create(
		strategy: WithConfig<ConfigValue<Owner<AccountId>>, AutoId<CollectionAutoId>>,
	) -> Result<CollectionAutoId, DispatchError> {
		let WithConfig { config: ConfigValue(owner), .. } = strategy;
		let id = unique_items::CurrentItemId::<Test, Instance2>::get().unwrap_or(0);

		unique_items::ItemOwner::<Test, unique_items::Instance2>::insert(id, owner);
		unique_items::CurrentItemId::<Test, Instance2>::set(Some(id.saturating_add(1)));

		Ok(id)
	}
}
impl AssetDefinition for AutoIdCollections {
	type Id = CollectionAutoId;
}
impl Destroy<NoParams> for AutoIdCollections {
	fn destroy(id: &Self::Id, _strategy: NoParams) -> DispatchResult {
		unique_items::ItemOwner::<Test, unique_items::Instance2>::remove(id);

		Ok(())
	}
}
