// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(test)]

use coretime_rococo_runtime::xcm_config::LocationToAccountId;
use xcm_runtime_apis::conversions::LocationToAccountHelper;
use parachains_common::AccountId;
use sp_core::crypto::Ss58Codec;
use xcm::latest::prelude::*;

const ALICE: [u8; 32] = [1u8; 32];

#[test]
fn location_conversion_works() {
    // the purpose of hardcoded values is to catch an unintended location conversion logic change.
    struct TestCase {
        description: &'static str,
        location: Location,
        expected_account_id_str: &'static str,
    }

    let test_cases = vec![
        // DescribeTerminus
        TestCase {
            description: "DescribeTerminus Parent",
            location: Location::new(1, Here),
            expected_account_id_str: "5Dt6dpkWPwLaH4BBCKJwjiWrFVAGyYk3tLUabvyn4v7KtESG",
        },
        TestCase {
            description: "DescribeTerminus Sibling",
            location: Location::new(1, [Parachain(1111)]),
            expected_account_id_str: "5Eg2fnssmmJnF3z1iZ1NouAuzciDaaDQH7qURAy3w15jULDk",
        },
        // DescribePalletTerminal
        TestCase {
            description: "DescribePalletTerminal Parent",
            location: Location::new(1, [PalletInstance(50)]),
            expected_account_id_str: "5CnwemvaAXkWFVwibiCvf2EjqwiqBi29S5cLLydZLEaEw6jZ",
        },
        TestCase {
            description: "DescribePalletTerminal Sibling",
            location: Location::new(1, [Parachain(1111), PalletInstance(50)]),
            expected_account_id_str: "5GFBgPjpEQPdaxEnFirUoa51u5erVx84twYxJVuBRAT2UP2g",
        },
        // DescribeAccountId32Terminal
        TestCase {
            description: "DescribeAccountId32Terminal Parent",
            location: Location::new(
                1,
                [Junction::AccountId32 { network: None, id: AccountId::from(ALICE).into() }],
            ),
            expected_account_id_str: "5DN5SGsuUG7PAqFL47J9meViwdnk9AdeSWKFkcHC45hEzVz4",
        },
        TestCase {
            description: "DescribeAccountId32Terminal Sibling",
            location: Location::new(
                1,
                [
                    Parachain(1111),
                    Junction::AccountId32 { network: None, id: AccountId::from(ALICE).into() },
                ],
            ),
            expected_account_id_str: "5DGRXLYwWGce7wvm14vX1Ms4Vf118FSWQbJkyQigY2pfm6bg",
        },
        // DescribeAccountKey20Terminal
        TestCase {
            description: "DescribeAccountKey20Terminal Parent",
            location: Location::new(1, [AccountKey20 { network: None, key: [0u8; 20] }]),
            expected_account_id_str: "5F5Ec11567pa919wJkX6VHtv2ZXS5W698YCW35EdEbrg14cg",
        },
        TestCase {
            description: "DescribeAccountKey20Terminal Sibling",
            location: Location::new(
                1,
                [Parachain(1111), AccountKey20 { network: None, key: [0u8; 20] }],
            ),
            expected_account_id_str: "5CB2FbUds2qvcJNhDiTbRZwiS3trAy6ydFGMSVutmYijpPAg",
        },
        // DescribeTreasuryVoiceTerminal
        TestCase {
            description: "DescribeTreasuryVoiceTerminal Parent",
            location: Location::new(1, [Plurality { id: BodyId::Treasury, part: BodyPart::Voice }]),
            expected_account_id_str: "5CUjnE2vgcUCuhxPwFoQ5r7p1DkhujgvMNDHaF2bLqRp4D5F",
        },
        TestCase {
            description: "DescribeTreasuryVoiceTerminal Sibling",
            location: Location::new(
                1,
                [Parachain(1111), Plurality { id: BodyId::Treasury, part: BodyPart::Voice }],
            ),
            expected_account_id_str: "5G6TDwaVgbWmhqRUKjBhRRnH4ry9L9cjRymUEmiRsLbSE4gB",
        },
        // DescribeBodyTerminal
        TestCase {
            description: "DescribeBodyTerminal Parent",
            location: Location::new(1, [Plurality { id: BodyId::Unit, part: BodyPart::Voice }]),
            expected_account_id_str: "5EBRMTBkDisEXsaN283SRbzx9Xf2PXwUxxFCJohSGo4jYe6B",
        },
        TestCase {
            description: "DescribeBodyTerminal Sibling",
            location: Location::new(
                1,
                [Parachain(1111), Plurality { id: BodyId::Unit, part: BodyPart::Voice }],
            ),
            expected_account_id_str: "5DBoExvojy8tYnHgLL97phNH975CyT45PWTZEeGoBZfAyRMH",
        },
    ];

    for tc in test_cases {
        let expected =
            AccountId::from_string(tc.expected_account_id_str).expect("Invalid AccountId string");

        let got = LocationToAccountHelper::<AccountId, LocationToAccountId>::convert_location(
            tc.location.into(),
        )
            .unwrap();

        assert_eq!(got, expected, "{}", tc.description);
    }
}
