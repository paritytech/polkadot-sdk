#![cfg(test)]

use crate::{mock::*, pallet::*};
use frame_support::{assert_noop, assert_ok, BoundedVec,traits::ConstU32};
use codec::Encode;
use sp_runtime::traits::{Hash, BlakeTwo256};  // Add BlakeTwo256 here


fn make_bounded_title(title: &[u8]) -> BoundedVec<u8, <Test as Config>::MaxTitleLength> {
    BoundedVec::try_from(title.to_vec()).unwrap()
}

#[test]
fn create_subnet_works() {
    new_test_ext().execute_with(|| {
        let king = 1;
        let title = make_bounded_title(b"Test Subnet");
        let performance_params = PerformanceParams {
            min_cpu_cores: 4,
            min_memory: 8192,
            min_storage: 1024,
        };
        let verification_type = VerificationType::Performance;

        assert_ok!(King::create_subnet(
            RuntimeOrigin::signed(king),
            title.clone(),
            performance_params,
            verification_type
        ));

        let mut data = Vec::new();
        data.extend_from_slice(&king.encode());
        data.extend_from_slice(&title);
        let subnet_id = BlakeTwo256::hash(&data);

        System::assert_last_event(Event::SubnetCreated { 
            king, 
            subnet_id 
        }.into());
    });
}

#[test]
fn create_duplicate_subnet_fails() {
    new_test_ext().execute_with(|| {
        let king = 1;
        let title = make_bounded_title(b"Test Subnet");
        let performance_params = PerformanceParams {
            min_cpu_cores: 4,
            min_memory: 8192,
            min_storage: 1024,
        };
        let verification_type = VerificationType::Performance;

        assert_ok!(King::create_subnet(
            RuntimeOrigin::signed(king),
            title.clone(),
            performance_params.clone(),
            verification_type.clone()
        ));

        assert_noop!(
            King::create_subnet(
                RuntimeOrigin::signed(king),
                title,
                performance_params,
                verification_type
            ),
            Error::<Test>::SubnetLimitReached
        );
    });
}

#[test]
fn verify_provider_works() {
    new_test_ext().execute_with(|| {
        let king = 1;
        let provider = 2;
        let title = make_bounded_title(b"Test Subnet");
        let performance_params = PerformanceParams {
            min_cpu_cores: 4,
            min_memory: 8192,
            min_storage: 1024,
        };
        
        assert_ok!(King::create_subnet(
            RuntimeOrigin::signed(king),
            title.clone(),
            performance_params,
            VerificationType::Performance
        ));

        let mut data = Vec::new();
        data.extend_from_slice(&king.encode());
        data.extend_from_slice(&title);
        let subnet_id = BlakeTwo256::hash(&data);

        assert_ok!(King::verify_provider(
            RuntimeOrigin::signed(king),
            subnet_id,
            provider
        ));

        System::assert_last_event(Event::ProviderVerified { 
            subnet_id,
            provider 
        }.into());
    });
}

#[test]
fn verify_provider_failures() {
    new_test_ext().execute_with(|| {
        let king = 1;
        let wrong_king = 2;
        let provider = 3;
        let title = make_bounded_title(b"Test Subnet");
        let performance_params = PerformanceParams {
            min_cpu_cores: 4,
            min_memory: 8192,
            min_storage: 1024,
        };

        assert_ok!(King::create_subnet(
            RuntimeOrigin::signed(king),
            title.clone(),
            performance_params,
            VerificationType::Performance
        ));

        let mut data = Vec::new();
        data.extend_from_slice(&king.encode());
        data.extend_from_slice(&title);
        let subnet_id = BlakeTwo256::hash(&data);

        assert_noop!(
            King::verify_provider(
                RuntimeOrigin::signed(wrong_king),
                subnet_id,
                provider
            ),
            Error::<Test>::SubnetNotFound
        );

        assert_ok!(King::verify_provider(
            RuntimeOrigin::signed(king),
            subnet_id,
            provider
        ));

        assert_noop!(
            King::verify_provider(
                RuntimeOrigin::signed(king),
                subnet_id,
                provider
            ),
            Error::<Test>::ProviderAlreadyVerified
        );
    });
}