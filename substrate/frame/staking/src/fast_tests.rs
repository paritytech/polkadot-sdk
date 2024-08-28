// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tests for the module.

use super::*;
use mock::*;

use frame_support::testing_prelude::*;

#[test]
fn collect_exposures_multi_page_elect_works() {
	ExtBuilder::default().exposures_page_size(2).build_and_execute(|| {
		assert_eq!(MaxExposurePageSize::get(), 2);

		let current_era = CurrentEra::<Test>::get().unwrap();

		let exposure_one = Exposure {
			total: 1000 + 700,
			own: 1000,
			others: vec![
				IndividualExposure { who: 101, value: 500 },
				IndividualExposure { who: 102, value: 100 },
				IndividualExposure { who: 103, value: 100 },
			],
		};

		let exposure_two = Exposure {
			total: 1000 + 1000,
			own: 1000,
			others: vec![
				IndividualExposure { who: 104, value: 500 },
				IndividualExposure { who: 105, value: 500 },
			],
		};

		let exposure_three = Exposure {
			total: 1000 + 500,
			own: 1000,
			others: vec![
				IndividualExposure { who: 110, value: 250 },
				IndividualExposure { who: 111, value: 250 },
			],
		};

		let exposures_page_one = bounded_vec![(1, exposure_one), (2, exposure_two),];
		let exposures_page_two = bounded_vec![(1, exposure_three),];

		assert_eq!(
			Pallet::<Test>::store_stakers_info_paged(exposures_page_one).to_vec(),
			vec![1, 2]
		);
		assert_eq!(Pallet::<Test>::store_stakers_info_paged(exposures_page_two).to_vec(), vec![1]);

		// Stakers overview OK for validator 1.
		assert_eq!(
			ErasStakersOverview::<Test>::get(0, &1).unwrap(),
			PagedExposureMetadata { total: 2200, own: 1000, nominator_count: 5, page_count: 3 },
		);
		// Stakers overview OK for validator 2.
		assert_eq!(
			ErasStakersOverview::<Test>::get(0, &2).unwrap(),
			PagedExposureMetadata { total: 2000, own: 1000, nominator_count: 2, page_count: 1 },
		);

		// validator 1 has 3 paged exposures.
		assert!(ErasStakersPaged::<Test>::get((0, &1, 0)).is_some());
		assert!(ErasStakersPaged::<Test>::get((0, &1, 1)).is_some());
		assert!(ErasStakersPaged::<Test>::get((0, &1, 2)).is_some());
		assert!(ErasStakersPaged::<Test>::get((0, &1, 3)).is_none());
		assert_eq!(ErasStakersPaged::<Test>::iter_prefix_values((0, &1)).count(), 3);

		// validator 2 has 1 paged exposures.
		assert!(ErasStakersPaged::<Test>::get((0, &2, 0)).is_some());
		assert!(ErasStakersPaged::<Test>::get((0, &2, 1)).is_none());
		assert_eq!(ErasStakersPaged::<Test>::iter_prefix_values((0, &2)).count(), 1);

		// exposures of validator 1.
		assert_eq!(
			ErasStakersPaged::<Test>::iter_prefix_values((0, &1)).collect::<Vec<_>>(),
			vec![
				ExposurePage {
					page_total: 100,
					others: vec![IndividualExposure { who: 103, value: 100 }]
				},
				ExposurePage {
					page_total: 500,
					others: vec![
						IndividualExposure { who: 110, value: 250 },
						IndividualExposure { who: 111, value: 250 }
					]
				},
				ExposurePage {
					page_total: 600,
					others: vec![
						IndividualExposure { who: 101, value: 500 },
						IndividualExposure { who: 102, value: 100 }
					]
				},
			],
		);

		// exposures of validator 2.
		assert_eq!(
			ErasStakersPaged::<Test>::iter_prefix_values((0, &2)).collect::<Vec<_>>(),
			vec![ExposurePage {
				page_total: 1000,
				others: vec![
					IndividualExposure { who: 104, value: 500 },
					IndividualExposure { who: 105, value: 500 }
				]
			}],
		);
	})
}
