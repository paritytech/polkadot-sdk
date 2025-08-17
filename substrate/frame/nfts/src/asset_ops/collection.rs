use crate::{
	types::asset_strategies::{CollectionConfig, *},
	Collection as CollectionStorage, *,
};
use core::marker::PhantomData;
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::{
		tokens::asset_ops::{
			common_strategies::*, AssetDefinition, Create, Destroy, Inspect, Update,
		},
		EnsureOrigin,
	},
	BoundedSlice,
};
use frame_system::ensure_signed;
use sp_core::Get;
use sp_runtime::DispatchError;

pub struct Collection<PalletInstance>(PhantomData<PalletInstance>);

impl<T: Config<I>, I: 'static> AssetDefinition for Collection<Pallet<T, I>> {
	type Id = T::CollectionId;
}

impl<T: Config<I>, I: 'static> Inspect<Owner<T::AccountId>> for Collection<Pallet<T, I>> {
	fn inspect(
		collection: &Self::Id,
		_owner_strategy: Owner<T::AccountId>,
	) -> Result<T::AccountId, DispatchError> {
		CollectionStorage::<T, I>::get(collection)
			.map(|a| a.owner)
			.ok_or(Error::<T, I>::UnknownCollection.into())
	}
}

impl<T: Config<I>, I: 'static> Update<Owner<T::AccountId>> for Collection<Pallet<T, I>> {
	fn update(
		collection: &Self::Id,
		_owner_strategy: Owner<T::AccountId>,
		new_owner: &T::AccountId,
	) -> DispatchResult {
		let maybe_check_owner = None;
		<Pallet<T, I>>::do_change_collection_owner(*collection, new_owner, maybe_check_owner)
	}
}

impl<T: Config<I>, I: 'static> Update<ChangeOwnerFrom<T::AccountId>> for Collection<Pallet<T, I>> {
	fn update(
		collection: &Self::Id,
		strategy: ChangeOwnerFrom<T::AccountId>,
		new_owner: &T::AccountId,
	) -> DispatchResult {
		let CheckState(from, ..) = strategy;

		<Pallet<T, I>>::do_change_collection_owner(*collection, new_owner, Some(from))
	}
}

impl<T: Config<I>, I: 'static> Update<CheckOrigin<T::RuntimeOrigin, Owner<T::AccountId>>>
	for Collection<Pallet<T, I>>
{
	fn update(
		collection: &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, Owner<T::AccountId>>,
		new_owner: &T::AccountId,
	) -> DispatchResult {
		let CheckOrigin(origin, owner_strategy) = strategy;

		let maybe_check_signer = T::ForceOrigin::try_origin(origin)
			.map(|_| None)
			.or_else(|origin| ensure_signed(origin).map(Some).map_err(DispatchError::from))?;

		if let Some(signer) = maybe_check_signer {
			<Pallet<T, I>>::do_transfer_ownership(signer, *collection, new_owner.clone())
		} else {
			Self::update(collection, owner_strategy, new_owner)
		}
	}
}

impl<T: Config<I>, I: 'static> Inspect<Admin<T::AccountId>> for Collection<Pallet<T, I>> {
	fn inspect(
		collection: &Self::Id,
		_admin_strategy: Admin<T::AccountId>,
	) -> Result<T::AccountId, DispatchError> {
		if let Some(admin) = <Pallet<T, I>>::find_account_by_role(collection, CollectionRole::Admin)
		{
			Ok(admin)
		} else if CollectionStorage::<T, I>::get(collection).is_some() {
			Err(Error::<T, I>::NoAdmin.into())
		} else {
			Err(Error::<T, I>::UnknownCollection.into())
		}
	}
}

impl<T: Config<I>, I: 'static>
	Inspect<
		CollectionConfig<
			<T::Currency as Currency<T::AccountId>>::Balance,
			<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			T::CollectionId,
		>,
	> for Collection<Pallet<T, I>>
{
	fn inspect(
		collection: &Self::Id,
		_config_strategy: CollectionConfig<
			<T::Currency as Currency<T::AccountId>>::Balance,
			<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			T::CollectionId,
		>,
	) -> Result<CollectionConfigFor<T, I>, DispatchError> {
		CollectionConfigOf::<T, I>::get(collection).ok_or(Error::<T, I>::UnknownCollection.into())
	}
}
impl<T: Config<I>, I: 'static>
	Update<
		CollectionConfig<
			<T::Currency as Currency<T::AccountId>>::Balance,
			<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			T::CollectionId,
		>,
	> for Collection<Pallet<T, I>>
{
	fn update(
		collection: &Self::Id,
		_config_strategy: CollectionConfig<
			<T::Currency as Currency<T::AccountId>>::Balance,
			<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			T::CollectionId,
		>,
		new_config: CollectionConfigFor<T, I>,
	) -> DispatchResult {
		<Pallet<T, I>>::do_force_collection_config(*collection, new_config.clone())
	}
}

impl<T: Config<I>, I: 'static>
	Inspect<CollectionDeposit<<T::Currency as Currency<T::AccountId>>::Balance>>
	for Collection<Pallet<T, I>>
{
	fn inspect(
		collection: &Self::Id,
		_deposit_strategy: CollectionDeposit<<T::Currency as Currency<T::AccountId>>::Balance>,
	) -> Result<DepositBalanceOf<T, I>, DispatchError> {
		CollectionStorage::<T, I>::get(collection)
			.map(|a| a.owner_deposit)
			.ok_or(Error::<T, I>::UnknownCollection.into())
	}
}

impl<T: Config<I>, I: 'static> Inspect<Witness<DestroyWitness>> for Collection<Pallet<T, I>> {
	fn inspect(
		collection: &Self::Id,
		_witness_strategy: Witness<DestroyWitness>,
	) -> Result<DestroyWitness, DispatchError> {
		CollectionStorage::<T, I>::get(collection)
			.map(|details| DestroyWitness {
				item_metadatas: details.item_metadatas,
				item_configs: details.item_configs,
				attributes: details.attributes,
			})
			.ok_or(Error::<T, I>::UnknownCollection.into())
	}
}

impl<T: Config<I>, I: 'static> Inspect<Bytes> for Collection<Pallet<T, I>> {
	fn inspect(collection: &Self::Id, _bytes_strategy: Bytes) -> Result<Vec<u8>, DispatchError> {
		CollectionMetadataOf::<T, I>::get(collection)
			.map(|collection_metadata| collection_metadata.data.into())
			.ok_or(Error::<T, I>::MetadataNotFound.into())
	}
}

impl<T: Config<I>, I: 'static> Update<Bytes> for Collection<Pallet<T, I>> {
	fn update(
		collection: &Self::Id,
		_bytes_strategy: Bytes,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let maybe_check_origin = None;
		let update = update
			.map(|data| <Pallet<T, I>>::construct_metadata(data.to_vec()))
			.transpose()?;

		match update {
			Some(metadata) => Pallet::<T, I>::do_set_collection_metadata(
				maybe_check_origin,
				*collection,
				metadata,
			),
			None => Pallet::<T, I>::do_clear_collection_metadata(maybe_check_origin, *collection),
		}
	}
}

impl<'a, T: Config<I>, I: 'static> Inspect<Bytes<RegularAttribute<'a>>>
	for Collection<Pallet<T, I>>
{
	fn inspect(
		collection: &Self::Id,
		bytes: Bytes<RegularAttribute>,
	) -> Result<Vec<u8>, DispatchError> {
		let item = None::<T::ItemId>;
		let Bytes(RegularAttribute(attribute)) = bytes;

		Attribute::<T, I>::get((
			collection,
			item,
			AttributeNamespace::CollectionOwner,
			<Pallet<T, I>>::construct_attribute_key(attribute.to_vec())?,
		))
		.map(|a| a.0.into())
		.ok_or(Error::<T, I>::AttributeNotFound.into())
	}
}

impl<'a, T: Config<I>, I: 'static> Update<Bytes<RegularAttribute<'a>>>
	for Collection<Pallet<T, I>>
{
	fn update(
		collection: &Self::Id,
		bytes: Bytes<RegularAttribute>,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let maybe_check_origin = None;
		let maybe_item = None;
		let namespace = AttributeNamespace::CollectionOwner;
		let Bytes(RegularAttribute(key)) = bytes;

		<Pallet<T, I>>::do_update_attribute(
			maybe_check_origin,
			*collection,
			maybe_item,
			namespace,
			key,
			update,
		)
	}
}

impl<'a, T: Config<I>, I: 'static> Inspect<Bytes<SystemAttribute<'a>>>
	for Collection<Pallet<T, I>>
{
	fn inspect(
		collection: &Self::Id,
		bytes: Bytes<SystemAttribute>,
	) -> Result<Vec<u8>, DispatchError> {
		let item: Option<T::ItemId> = None;
		let namespace = AttributeNamespace::Pallet;

		let Bytes(SystemAttribute(attribute)) = bytes;
		let attribute =
			BoundedSlice::<_, _>::try_from(attribute).map_err(|_| Error::<T, I>::IncorrectData)?;

		Attribute::<T, I>::get((collection, item, namespace, attribute))
			.map(|a| a.0.into())
			.ok_or(Error::<T, I>::AttributeNotFound.into())
	}
}

impl<'a, T: Config<I>, I: 'static> Update<Bytes<SystemAttribute<'a>>> for Collection<Pallet<T, I>> {
	fn update(
		collection: &Self::Id,
		bytes: Bytes<SystemAttribute>,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let maybe_check_origin = None;
		let maybe_item = None;
		let namespace = AttributeNamespace::Pallet;
		let Bytes(SystemAttribute(key)) = bytes;

		<Pallet<T, I>>::do_update_attribute(
			maybe_check_origin,
			*collection,
			maybe_item,
			namespace,
			key,
			update,
		)
	}
}

impl<'a, T: Config<I>, I: 'static> Inspect<Roles<'a, T::AccountId>> for Collection<Pallet<T, I>> {
	fn inspect(
		collection: &Self::Id,
		strategy: Roles<'a, T::AccountId>,
	) -> Result<CollectionRoles, DispatchError> {
		let Roles(account) = strategy;

		if let Some(roles) = CollectionRoleOf::<T, I>::get(collection, account) {
			Ok(roles)
		} else {
			CollectionStorage::<T, I>::get(collection)
				.map(|_| CollectionRoles::default())
				.ok_or(Error::<T, I>::UnknownCollection.into())
		}
	}
}

impl<T: Config<I>, I: 'static> Create<WithCollectionOwner<T::AccountId, T::CollectionId>>
	for Collection<Pallet<T, I>>
{
	fn create(
		strategy: WithCollectionOwner<T::AccountId, T::CollectionId>,
	) -> Result<T::CollectionId, DispatchError> {
		let WithConfig { config: ConfigValue(owner), .. } = strategy;

		Self::create(WithCollectionManagers::from((
			Owner::with_config_value(owner.clone()),
			Admin::with_config_value(owner),
		)))
	}
}

impl<T: Config<I>, I: 'static> Create<WithCollectionManagers<T::AccountId, T::CollectionId>>
	for Collection<Pallet<T, I>>
{
	fn create(
		strategy: WithCollectionManagers<T::AccountId, T::CollectionId>,
	) -> Result<T::CollectionId, DispatchError> {
		let WithConfig { config, .. } = strategy;

		Self::create(WithCollectionConfig::from((config.0, config.1, Default::default())))
	}
}

impl<T: Config<I>, I: 'static>
	Create<
		WithCollectionConfig<
			T::AccountId,
			<T::Currency as Currency<T::AccountId>>::Balance,
			<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			T::CollectionId,
		>,
	> for Collection<Pallet<T, I>>
{
	fn create(
		strategy: WithCollectionConfig<
			T::AccountId,
			<T::Currency as Currency<T::AccountId>>::Balance,
			<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			T::CollectionId,
		>,
	) -> Result<T::CollectionId, DispatchError> {
		let WithConfig { config, .. } = strategy;

		Self::create(WithCollectionDeposit::from((
			config.0,
			config.1,
			config.2,
			CollectionDeposit::with_config_value(T::CollectionDeposit::get()),
		)))
	}
}

impl<T: Config<I>, I: 'static>
	Create<
		WithCollectionDeposit<
			T::AccountId,
			<T::Currency as Currency<T::AccountId>>::Balance,
			<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			T::CollectionId,
		>,
	> for Collection<Pallet<T, I>>
{
	fn create(
		strategy: WithCollectionDeposit<
			T::AccountId,
			<T::Currency as Currency<T::AccountId>>::Balance,
			<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			T::CollectionId,
		>,
	) -> Result<T::CollectionId, DispatchError> {
		let WithConfig {
			config:
				(
					ConfigValue(owner),
					ConfigValue(admin),
					ConfigValue(collection_config),
					ConfigValue(collection_deposit),
				),
			..
		} = strategy;

		let collection = <Pallet<T, I>>::get_next_collection_id()?;

		<Pallet<T, I>>::do_create_collection(
			collection,
			owner.clone(),
			admin.clone(),
			collection_config,
			collection_deposit,
			Event::Created { collection, creator: owner, owner: admin },
		)?;

		<Pallet<T, I>>::set_next_collection_id(collection);

		Ok(collection)
	}
}

impl<T: Config<I>, I: 'static>
	Create<
		CheckOrigin<
			T::RuntimeOrigin,
			WithCollectionConfig<
				T::AccountId,
				<T::Currency as Currency<T::AccountId>>::Balance,
				<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
				T::CollectionId,
			>,
		>,
	> for Collection<Pallet<T, I>>
{
	fn create(
		strategy: CheckOrigin<
			T::RuntimeOrigin,
			WithCollectionConfig<
				T::AccountId,
				<T::Currency as Currency<T::AccountId>>::Balance,
				<T::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
				T::CollectionId,
			>,
		>,
	) -> Result<T::CollectionId, DispatchError> {
		let CheckOrigin(origin, WithConfig { config, .. }) = strategy;
		let (ConfigValue(owner), _, ConfigValue(collection_config)) = &config;

		let collection = <Pallet<T, I>>::get_next_collection_id()?;

		let maybe_check_signer =
			T::ForceOrigin::try_origin(origin).map(|_| None).or_else(|origin| {
				T::CreateOrigin::ensure_origin(origin, &collection)
					.map(Some)
					.map_err(DispatchError::from)
			})?;

		let collection_deposit = if let Some(signer) = maybe_check_signer {
			ensure!(signer == *owner, Error::<T, I>::NoPermission);

			// DepositRequired can be disabled by calling the with `ForceOrigin` only
			ensure!(
				!collection_config.has_disabled_setting(CollectionSetting::DepositRequired),
				Error::<T, I>::WrongSetting
			);

			T::CollectionDeposit::get()
		} else {
			Zero::zero()
		};

		Self::create(WithCollectionDeposit::from((
			config.0,
			config.1,
			config.2,
			CollectionDeposit::with_config_value(collection_deposit),
		)))
	}
}

impl<T: Config<I>, I: 'static> Destroy<WithWitness<DestroyWitness>> for Collection<Pallet<T, I>> {
	fn destroy(collection: &Self::Id, strategy: WithWitness<DestroyWitness>) -> DispatchResult {
		let CheckState(witness, ..) = strategy;

		<Pallet<T, I>>::do_destroy_collection(*collection, witness, None)?;

		Ok(())
	}
}

impl<T: Config<I>, I: 'static> Destroy<IfOwnedBy<T::AccountId, WithWitness<DestroyWitness>>>
	for Collection<Pallet<T, I>>
{
	fn destroy(
		collection: &Self::Id,
		strategy: IfOwnedBy<T::AccountId, WithWitness<DestroyWitness>>,
	) -> DispatchResult {
		let CheckState(owner, CheckState(witness, ..)) = strategy;

		<Pallet<T, I>>::do_destroy_collection(collection.clone(), witness, Some(owner))?;

		Ok(())
	}
}

impl<T: Config<I>, I: 'static> Destroy<CheckOrigin<T::RuntimeOrigin, WithWitness<DestroyWitness>>>
	for Collection<Pallet<T, I>>
{
	fn destroy(
		collection: &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, WithWitness<DestroyWitness>>,
	) -> DispatchResult {
		let CheckOrigin(origin, CheckState(witness, ..)) = strategy;

		let maybe_check_owner = T::ForceOrigin::try_origin(origin)
			.map(|_| None)
			.or_else(|origin| ensure_signed(origin).map(Some).map_err(DispatchError::from))?;

		<Pallet<T, I>>::do_destroy_collection(*collection, witness, maybe_check_owner)?;

		Ok(())
	}
}
