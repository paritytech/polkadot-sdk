use super::*;

pub type PredefinedIdCollectionsInstance = unique_items::Instance1;
impl unique_items::Config<PredefinedIdCollectionsInstance> for Test {
	type ItemId = AssetId;
}

// Below are the operations implementations:
// * collection creation (with a predefined ID)
// * collection destruction

impl Create<WithConfig<ConfigValue<Owner<AccountId>>, PredefinedId<AssetId>>>
	for PredefinedIdCollections
{
	fn create(
		strategy: WithConfig<ConfigValue<Owner<AccountId>>, PredefinedId<AssetId>>,
	) -> Result<AssetId, DispatchError> {
		let WithConfig { config: ConfigValue(owner), extra: id_assignment } = strategy;
		let id = id_assignment.params;

		unique_items::ItemOwner::<Test, PredefinedIdCollectionsInstance>::try_mutate(
			id.clone(),
			|current_owner| {
				if current_owner.is_none() {
					*current_owner = Some(owner);
					Ok(())
				} else {
					Err(unique_items::Error::<Test, PredefinedIdCollectionsInstance>::AlreadyExists)
				}
			},
		)?;

		Ok(id)
	}
}
impl AssetDefinition for PredefinedIdCollections {
	type Id = AssetId;
}
impl Destroy<NoParams> for PredefinedIdCollections {
	fn destroy(id: &Self::Id, _strategy: NoParams) -> DispatchResult {
		unique_items::ItemOwner::<Test, PredefinedIdCollectionsInstance>::remove(id);

		Ok(())
	}
}
