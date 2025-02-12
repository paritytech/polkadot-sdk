#![cfg(test)]

use crate::{mock::*, pallet::Config, Error, Event, ResourceConfig, Pallet as Subnet};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::{BlakeTwo256, Hash};
use codec::Encode;

fn default_resource_config() -> ResourceConfig {
    ResourceConfig {
        min_computational_capacity: 4,
        min_memory: 8192,
        min_storage: 1024,
        min_bandwidth: 100,
    }
}

#[test]
fn create_subnet_works() {
    new_test_ext().execute_with(|| {
        let king_account = 1;
        
        // First create a king
        assert_ok!(King::create_subnet(
            RuntimeOrigin::signed(king_account),
            vec![].try_into().unwrap(),
            pallet_king::PerformanceParams {
                min_cpu_cores: 4,
                min_memory: 8192,
                min_storage: 1024,
            },
            pallet_king::VerificationType::Performance
        ));

        let resource_config = default_resource_config();
        
        // Added type annotation for Subnet
        assert_ok!(Subnet::<Test>::create_subnet(
            RuntimeOrigin::signed(king_account),
            resource_config.clone()
        ));

        let subnet_id = BlakeTwo256::hash(&king_account.encode());
        
        // Using direct storage access
        assert!(crate::Subnets::<Test>::contains_key(subnet_id));
        assert!(crate::SubnetMetricsStorage::<Test>::contains_key(subnet_id));
    });
}

