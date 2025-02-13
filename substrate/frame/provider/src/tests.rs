#![cfg(test)]

use crate::{mock::*, Error, Event, ResourceInfo, Pallet as Provider};  // Remove unused ProviderStatus
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::{BlakeTwo256, Hash};
use sp_core::H256;
use codec::Encode;
use pallet_king::{self, PerformanceParams, VerificationType};
use pallet_subnet::{self, ResourceConfig};

fn default_resource_info() -> ResourceInfo {
   ResourceInfo {
       computational_capacity: 4,
       memory: 8192,
       storage: 1024,
       bandwidth: 100,
   }
}

fn setup_subnet() -> (u64, H256) {
   let king_account = 1;
   
   // First create a king
   assert_ok!(King::create_subnet(
       RuntimeOrigin::signed(king_account),
       vec![].try_into().unwrap(),
       PerformanceParams {
           min_cpu_cores: 4,  
           min_memory: 8192,
           min_storage: 1024,
       },
       VerificationType::Performance
   ));

   // Create subnet
   let resource_config = ResourceConfig {
       min_computational_capacity: 4,
       min_memory: 8192,
       min_storage: 1024,
       min_bandwidth: 100,
   };

   assert_ok!(Subnet::create_subnet(
       RuntimeOrigin::signed(king_account),
       resource_config.clone()
   ));

   let subnet_id = BlakeTwo256::hash(&(king_account.encode(), resource_config.encode()).encode());
   
   (king_account, subnet_id)
}

#[test]
fn register_provider_works() {
   new_test_ext().execute_with(|| {
       let provider_account = 2;
       let (_, subnet_id) = setup_subnet();
       
       let resources = default_resource_info();
       
       assert_ok!(Provider::<Test>::register_provider(
           RuntimeOrigin::signed(provider_account),
           subnet_id,
           resources
       ));
       
       assert!(crate::Providers::<Test>::contains_key(provider_account));
       
       System::assert_last_event(Event::ProviderRegistered { 
           provider: provider_account,
           subnet_id 
       }.into());
   });
}

#[test]
fn register_provider_fails_invalid_subnet() {
   new_test_ext().execute_with(|| {
       let provider_account = 2;
       let invalid_subnet_id = BlakeTwo256::hash(&[1u8]);
       
       assert_noop!(
           Provider::<Test>::register_provider(
               RuntimeOrigin::signed(provider_account),
               invalid_subnet_id,
               default_resource_info()
           ),
           Error::<Test>::InvalidSubnet
       );
   });
}

#[test]
fn duplicate_registration_fails() {
   new_test_ext().execute_with(|| {
       let provider_account = 2;
       let (_, subnet_id) = setup_subnet();
       let resources = default_resource_info();
       
       // First registration should succeed
       assert_ok!(Provider::<Test>::register_provider(
           RuntimeOrigin::signed(provider_account),
           subnet_id,
           resources.clone()
       ));
       
       // Second registration should fail
       assert_noop!(
           Provider::<Test>::register_provider(
               RuntimeOrigin::signed(provider_account),
               subnet_id,
               resources
           ),
           Error::<Test>::AlreadyRegistered
       );
   });
}