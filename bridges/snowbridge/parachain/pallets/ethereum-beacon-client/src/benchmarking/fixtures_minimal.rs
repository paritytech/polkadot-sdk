// Generated, do not edit!
// See README.md for instructions to generate
use crate::{CheckpointUpdate, ExecutionHeaderUpdate, Update};
use hex_literal::hex;
use primitives::{
	updates::AncestryProof, BeaconHeader, ExecutionPayloadHeader, NextSyncCommitteeUpdate,
	SyncAggregate, SyncCommittee,
};
use sp_core::U256;
use sp_std::{boxed::Box, vec};

pub fn make_checkpoint() -> Box<CheckpointUpdate> {
	Box::new(CheckpointUpdate {
        header: BeaconHeader {
            slot: 152,
            proposer_index: 7,
            parent_root: hex!("7ecd45ee8bbacb20c6818eb46740b0f8d2fbd5af964e0dd844d3aa49ca75ce26").into(),
            state_root: hex!("0859c78c54a2ecf1af0aecc05f3c8ba9274d1df500bfc4e6a7194c0235f55def").into(),
            body_root: hex!("561bec3e5b200ee1bf6b60b5858ca2d7766ac9327b46355c070afafae8fca2ba").into(),
        },
        current_sync_committee: SyncCommittee {
            pubkeys: [
                hex!("a99a76ed7796f7be22d5b7e85deeb7c5677e88e511e0b337618f8c4eb61349b4bf2d153f649f7b53359fe8b94a38e44c").into(),
                hex!("ab0bdda0f85f842f431beaccf1250bf1fd7ba51b4100fd64364b6401fda85bb0069b3e715b58819684e7fc0b10a72a34").into(),
                hex!("a8d4c7c27795a725961317ef5953a7032ed6d83739db8b0e8a72353d1b8b4439427f7efa2c89caa03cc9f28f8cbab8ac").into(),
                hex!("81283b7a20e1ca460ebd9bbd77005d557370cabb1f9a44f530c4c4c66230f675f8df8b4c2818851aa7d77a80ca5a4a5e").into(),
                hex!("9977f1c8b731a8d5558146bfb86caea26434f3c5878b589bf280a42c9159e700e9df0e4086296c20b011d2e78c27d373").into(),
                hex!("88c141df77cd9d8d7a71a75c826c41a9c9f03c6ee1b180f3e7852f6a280099ded351b58d66e653af8e42816a4d8f532e").into(),
                hex!("a3a32b0f8b4ddb83f1a0a853d81dd725dfe577d4f4c3db8ece52ce2b026eca84815c1a7e8e92a4de3d755733bf7e4a9b").into(),
                hex!("b89bebc699769726a318c8e9971bd3171297c61aea4a6578a7a4f94b547dcba5bac16a89108b6b6a1fe3695d1a874a0b").into(),
                hex!("a99a76ed7796f7be22d5b7e85deeb7c5677e88e511e0b337618f8c4eb61349b4bf2d153f649f7b53359fe8b94a38e44c").into(),
                hex!("ab0bdda0f85f842f431beaccf1250bf1fd7ba51b4100fd64364b6401fda85bb0069b3e715b58819684e7fc0b10a72a34").into(),
                hex!("a8d4c7c27795a725961317ef5953a7032ed6d83739db8b0e8a72353d1b8b4439427f7efa2c89caa03cc9f28f8cbab8ac").into(),
                hex!("81283b7a20e1ca460ebd9bbd77005d557370cabb1f9a44f530c4c4c66230f675f8df8b4c2818851aa7d77a80ca5a4a5e").into(),
                hex!("9977f1c8b731a8d5558146bfb86caea26434f3c5878b589bf280a42c9159e700e9df0e4086296c20b011d2e78c27d373").into(),
                hex!("88c141df77cd9d8d7a71a75c826c41a9c9f03c6ee1b180f3e7852f6a280099ded351b58d66e653af8e42816a4d8f532e").into(),
                hex!("a3a32b0f8b4ddb83f1a0a853d81dd725dfe577d4f4c3db8ece52ce2b026eca84815c1a7e8e92a4de3d755733bf7e4a9b").into(),
                hex!("b89bebc699769726a318c8e9971bd3171297c61aea4a6578a7a4f94b547dcba5bac16a89108b6b6a1fe3695d1a874a0b").into(),
                hex!("a99a76ed7796f7be22d5b7e85deeb7c5677e88e511e0b337618f8c4eb61349b4bf2d153f649f7b53359fe8b94a38e44c").into(),
                hex!("ab0bdda0f85f842f431beaccf1250bf1fd7ba51b4100fd64364b6401fda85bb0069b3e715b58819684e7fc0b10a72a34").into(),
                hex!("a8d4c7c27795a725961317ef5953a7032ed6d83739db8b0e8a72353d1b8b4439427f7efa2c89caa03cc9f28f8cbab8ac").into(),
                hex!("81283b7a20e1ca460ebd9bbd77005d557370cabb1f9a44f530c4c4c66230f675f8df8b4c2818851aa7d77a80ca5a4a5e").into(),
                hex!("9977f1c8b731a8d5558146bfb86caea26434f3c5878b589bf280a42c9159e700e9df0e4086296c20b011d2e78c27d373").into(),
                hex!("88c141df77cd9d8d7a71a75c826c41a9c9f03c6ee1b180f3e7852f6a280099ded351b58d66e653af8e42816a4d8f532e").into(),
                hex!("a3a32b0f8b4ddb83f1a0a853d81dd725dfe577d4f4c3db8ece52ce2b026eca84815c1a7e8e92a4de3d755733bf7e4a9b").into(),
                hex!("b89bebc699769726a318c8e9971bd3171297c61aea4a6578a7a4f94b547dcba5bac16a89108b6b6a1fe3695d1a874a0b").into(),
                hex!("a99a76ed7796f7be22d5b7e85deeb7c5677e88e511e0b337618f8c4eb61349b4bf2d153f649f7b53359fe8b94a38e44c").into(),
                hex!("ab0bdda0f85f842f431beaccf1250bf1fd7ba51b4100fd64364b6401fda85bb0069b3e715b58819684e7fc0b10a72a34").into(),
                hex!("a8d4c7c27795a725961317ef5953a7032ed6d83739db8b0e8a72353d1b8b4439427f7efa2c89caa03cc9f28f8cbab8ac").into(),
                hex!("81283b7a20e1ca460ebd9bbd77005d557370cabb1f9a44f530c4c4c66230f675f8df8b4c2818851aa7d77a80ca5a4a5e").into(),
                hex!("9977f1c8b731a8d5558146bfb86caea26434f3c5878b589bf280a42c9159e700e9df0e4086296c20b011d2e78c27d373").into(),
                hex!("88c141df77cd9d8d7a71a75c826c41a9c9f03c6ee1b180f3e7852f6a280099ded351b58d66e653af8e42816a4d8f532e").into(),
                hex!("a3a32b0f8b4ddb83f1a0a853d81dd725dfe577d4f4c3db8ece52ce2b026eca84815c1a7e8e92a4de3d755733bf7e4a9b").into(),
                hex!("b89bebc699769726a318c8e9971bd3171297c61aea4a6578a7a4f94b547dcba5bac16a89108b6b6a1fe3695d1a874a0b").into(),
            ],
            aggregate_pubkey: hex!("8fe11476a05750c52618deb79918e2e674f56dfbf12dbce55ae4386d108e8a1e83c6326f5957e2ef19137582ce270dc6").into(),
        },
        current_sync_committee_branch: vec![
                hex!("c6dd41df7b4978b7ee7479dcbfdbfa9fb56d1464afe9806bfd6f527e9d1c69a4").into(),
                hex!("b3824f305318071010d4f4d9505f9fc102cdd57f1c03cf4540e471b157f98e3f").into(),
                hex!("66677299731d34be9d4dea7c0e89d8eae81b8dffa3c6218152484a063b1064a1").into(),
                hex!("ef53c2b02cdb7f44fb37ea75f4c48c5086604a26be9635f88b1138f961f87773").into(),
                hex!("1a3f86a88fb2ec99cb1d7b0f35227c0f646490773bac523c30fc7eccf87c0f84").into(),
        ],
        validators_root: hex!("270d43e74ce340de4bca2b1936beca0f4f5408d9e78aec4850920baf659d5b69").into(),
        block_roots_root: hex!("8ad7487f0e79a6cf67ce49916f74e5b272be9e38ea5f8a6a588eaa89ea7f858e").into(),
        block_roots_branch: vec![
            hex!("90683050cb3f3b04b2a89c206365747bef4c42154feb5d3f4198dfd48b2067ae").into(),
            hex!("34f521b45630273930475b8a4350fe2c71c804878ebcd89064eb1dc24a85e468").into(),
            hex!("81e166eaeaff5df2d3a5df306c78af075106cbf63c2f1b50ab67eec2dbb0f5ce").into(),
            hex!("857526b3bf20de3ec6e57565dd593304f83e47541cc18d7c2d5b4c9f2724ba14").into(),
            hex!("1b4bedd5ecc523599da000387f5e33ffe5324b8e234c43ee702de8ccc88d713f").into(),
        ],
    })
}

pub fn make_sync_committee_update() -> Box<Update> {
	Box::new(Update {
        attested_header: BeaconHeader {
            slot: 144,
            proposer_index: 2,
            parent_root: hex!("74e7da3093f79414b5ab593d1c12ba63767db7f4af62672fa1be641331c338e0").into(),
            state_root: hex!("92a08dc74f6af7bfcd428aa1677d58f36fefc159c0134b312c186a69fcd00496").into(),
            body_root: hex!("51506a8230d40d065a29ad01197c983b9b5d4b9abd26d3c48773941f4e6f482d").into(),
        },
        sync_aggregate: SyncAggregate{
            sync_committee_bits: hex!("ffffffff"),
            sync_committee_signature: hex!("92cbff711040bcc4a81616edbadbe712ea3ac4939c3d3121a4561b4b3950e2e5b09809606b79fdad2ce20fdef413120a04f02dc65149ca9b6ecab2a94ac4087062df4ab03e161ae56fa6fee4ba40af29c3b4327e21e846f98144cab30684d197").into(),
        },
        signature_slot: 145,
        next_sync_committee_update: Some(NextSyncCommitteeUpdate {
            next_sync_committee: SyncCommittee {
                pubkeys: [
                    hex!("a8d4c7c27795a725961317ef5953a7032ed6d83739db8b0e8a72353d1b8b4439427f7efa2c89caa03cc9f28f8cbab8ac").into(),
                    hex!("81283b7a20e1ca460ebd9bbd77005d557370cabb1f9a44f530c4c4c66230f675f8df8b4c2818851aa7d77a80ca5a4a5e").into(),
                    hex!("b89bebc699769726a318c8e9971bd3171297c61aea4a6578a7a4f94b547dcba5bac16a89108b6b6a1fe3695d1a874a0b").into(),
                    hex!("88c141df77cd9d8d7a71a75c826c41a9c9f03c6ee1b180f3e7852f6a280099ded351b58d66e653af8e42816a4d8f532e").into(),
                    hex!("9977f1c8b731a8d5558146bfb86caea26434f3c5878b589bf280a42c9159e700e9df0e4086296c20b011d2e78c27d373").into(),
                    hex!("ab0bdda0f85f842f431beaccf1250bf1fd7ba51b4100fd64364b6401fda85bb0069b3e715b58819684e7fc0b10a72a34").into(),
                    hex!("a3a32b0f8b4ddb83f1a0a853d81dd725dfe577d4f4c3db8ece52ce2b026eca84815c1a7e8e92a4de3d755733bf7e4a9b").into(),
                    hex!("a99a76ed7796f7be22d5b7e85deeb7c5677e88e511e0b337618f8c4eb61349b4bf2d153f649f7b53359fe8b94a38e44c").into(),
                    hex!("a8d4c7c27795a725961317ef5953a7032ed6d83739db8b0e8a72353d1b8b4439427f7efa2c89caa03cc9f28f8cbab8ac").into(),
                    hex!("81283b7a20e1ca460ebd9bbd77005d557370cabb1f9a44f530c4c4c66230f675f8df8b4c2818851aa7d77a80ca5a4a5e").into(),
                    hex!("b89bebc699769726a318c8e9971bd3171297c61aea4a6578a7a4f94b547dcba5bac16a89108b6b6a1fe3695d1a874a0b").into(),
                    hex!("88c141df77cd9d8d7a71a75c826c41a9c9f03c6ee1b180f3e7852f6a280099ded351b58d66e653af8e42816a4d8f532e").into(),
                    hex!("9977f1c8b731a8d5558146bfb86caea26434f3c5878b589bf280a42c9159e700e9df0e4086296c20b011d2e78c27d373").into(),
                    hex!("ab0bdda0f85f842f431beaccf1250bf1fd7ba51b4100fd64364b6401fda85bb0069b3e715b58819684e7fc0b10a72a34").into(),
                    hex!("a3a32b0f8b4ddb83f1a0a853d81dd725dfe577d4f4c3db8ece52ce2b026eca84815c1a7e8e92a4de3d755733bf7e4a9b").into(),
                    hex!("a99a76ed7796f7be22d5b7e85deeb7c5677e88e511e0b337618f8c4eb61349b4bf2d153f649f7b53359fe8b94a38e44c").into(),
                    hex!("a8d4c7c27795a725961317ef5953a7032ed6d83739db8b0e8a72353d1b8b4439427f7efa2c89caa03cc9f28f8cbab8ac").into(),
                    hex!("81283b7a20e1ca460ebd9bbd77005d557370cabb1f9a44f530c4c4c66230f675f8df8b4c2818851aa7d77a80ca5a4a5e").into(),
                    hex!("b89bebc699769726a318c8e9971bd3171297c61aea4a6578a7a4f94b547dcba5bac16a89108b6b6a1fe3695d1a874a0b").into(),
                    hex!("88c141df77cd9d8d7a71a75c826c41a9c9f03c6ee1b180f3e7852f6a280099ded351b58d66e653af8e42816a4d8f532e").into(),
                    hex!("9977f1c8b731a8d5558146bfb86caea26434f3c5878b589bf280a42c9159e700e9df0e4086296c20b011d2e78c27d373").into(),
                    hex!("ab0bdda0f85f842f431beaccf1250bf1fd7ba51b4100fd64364b6401fda85bb0069b3e715b58819684e7fc0b10a72a34").into(),
                    hex!("a3a32b0f8b4ddb83f1a0a853d81dd725dfe577d4f4c3db8ece52ce2b026eca84815c1a7e8e92a4de3d755733bf7e4a9b").into(),
                    hex!("a99a76ed7796f7be22d5b7e85deeb7c5677e88e511e0b337618f8c4eb61349b4bf2d153f649f7b53359fe8b94a38e44c").into(),
                    hex!("a8d4c7c27795a725961317ef5953a7032ed6d83739db8b0e8a72353d1b8b4439427f7efa2c89caa03cc9f28f8cbab8ac").into(),
                    hex!("81283b7a20e1ca460ebd9bbd77005d557370cabb1f9a44f530c4c4c66230f675f8df8b4c2818851aa7d77a80ca5a4a5e").into(),
                    hex!("b89bebc699769726a318c8e9971bd3171297c61aea4a6578a7a4f94b547dcba5bac16a89108b6b6a1fe3695d1a874a0b").into(),
                    hex!("88c141df77cd9d8d7a71a75c826c41a9c9f03c6ee1b180f3e7852f6a280099ded351b58d66e653af8e42816a4d8f532e").into(),
                    hex!("9977f1c8b731a8d5558146bfb86caea26434f3c5878b589bf280a42c9159e700e9df0e4086296c20b011d2e78c27d373").into(),
                    hex!("ab0bdda0f85f842f431beaccf1250bf1fd7ba51b4100fd64364b6401fda85bb0069b3e715b58819684e7fc0b10a72a34").into(),
                    hex!("a3a32b0f8b4ddb83f1a0a853d81dd725dfe577d4f4c3db8ece52ce2b026eca84815c1a7e8e92a4de3d755733bf7e4a9b").into(),
                    hex!("a99a76ed7796f7be22d5b7e85deeb7c5677e88e511e0b337618f8c4eb61349b4bf2d153f649f7b53359fe8b94a38e44c").into(),
                ],
                aggregate_pubkey: hex!("8fe11476a05750c52618deb79918e2e674f56dfbf12dbce55ae4386d108e8a1e83c6326f5957e2ef19137582ce270dc6").into(),
            },
            next_sync_committee_branch: vec![
                hex!("c243f5286cbf6c3c5f58ef6e1734a737bf96cfc0adbf2ee88d4a996f8dfc7097").into(),
                hex!("6ae16cb8ea57014c75a9469485bc024bb1e64676656cb069753bf091e58de88f").into(),
                hex!("1afa3c77cd75040c1bbbc5d1b7c07e65eeee44fbc659b64ccd55b4ba579038ae").into(),
                hex!("89b14216f60859ae366de066eeff4c7b69f8b5738c5b70cdfab4b3a976f9f12f").into(),
                hex!("b7d8fa1e64e6d6339a7c68a81e20daab65804777b93a04236fa503b575c2f1a9").into(),
            ],
        }),
        finalized_header: BeaconHeader{
            slot: 128,
            proposer_index: 3,
            parent_root: hex!("3774d6a479fc6f71ccaff3f8a1881c424b0319e96a7da15f87e3b750ba50dae5").into(),
            state_root: hex!("85937007c564046bcc147d53b1d2d78941e2787ec059bee191420369a7f80447").into(),
            body_root: hex!("c8f31c33ffaeef2d534363b7ef9ec391f90c17bc5bbe439cf0dcf0de8eeb4d7d").into(),
        },
        finality_branch: vec![
            hex!("1000000000000000000000000000000000000000000000000000000000000000").into(),
            hex!("10c726fac935bf9657cc7476d3cfa7bedec5983dcfb59e8a7df6d0a619e108d7").into(),
            hex!("c2374959a44ad09eb825ada636c430d530e656d7f06dccbcf2ca8b6dc834b188").into(),
            hex!("1afa3c77cd75040c1bbbc5d1b7c07e65eeee44fbc659b64ccd55b4ba579038ae").into(),
            hex!("89b14216f60859ae366de066eeff4c7b69f8b5738c5b70cdfab4b3a976f9f12f").into(),
            hex!("b7d8fa1e64e6d6339a7c68a81e20daab65804777b93a04236fa503b575c2f1a9").into(),
        ],
        block_roots_root: hex!("b10da1bbb1c0dcb01637b9e3dcb710d1013dc42a002bd837d5b4effa0ac995c9").into(),
        block_roots_branch: vec![
            hex!("9c62693fd336e4b3ca61d3cbe9caeb85f1f1b59ef50aeddfce53dc7321d6058b").into(),
            hex!("364947a6574f4acc97888e1fddb5ec718859d54cd7e3db98cda8db4dbea82ded").into(),
            hex!("10a61d26b0c1b5b89cf5b82b965f772a1b6ae852fbd13a986c5c63bf12b99244").into(),
            hex!("506ed8ca9908af4b5acb4cfdec6dd968acf38346c16385abca360c9976411c3e").into(),
            hex!("ada571922caf3dde48e19f1482df8d17dd6bb05e525c016c8cf6796a5d7f4a00").into(),
        ],
    })
}

pub fn make_finalized_header_update() -> Box<Update> {
	Box::new(Update {
        attested_header: BeaconHeader {
            slot: 184,
            proposer_index: 5,
            parent_root: hex!("13548d0b72ef2b352ba73a53274ad02e9310d8417d4782bf9cdad877da549595").into(),
            state_root: hex!("cbd5e841afef5103a613bb9a89199d39c40751af1af7cabadfa8e1fd49c4be09").into(),
            body_root: hex!("c9b5074ac3043ae2ad6da067b0787d61b6f1611b560ef80860ce62564534a53a").into(),
        },
        sync_aggregate: SyncAggregate{
            sync_committee_bits: hex!("ffffffff"),
            sync_committee_signature: hex!("80628825caef48ef13b4ffda96fcee74743609fe3537b7cd0ca8bc25c8a540b96dfbe23bdd0a60a8db70b2c48f090c1d03a25923207a193724d98e140db90ab692cd00ec2b4d3c0c4cbff13b4ab343e0c896e15cba08f9d82c392b1c982a8115").into(),
        },
        signature_slot: 185,
        next_sync_committee_update: None,
        finalized_header: BeaconHeader {
            slot: 168,
            proposer_index: 1,
            parent_root: hex!("bb8245e89f5d7d9191c559425b8522487d98b8f2c9b814a158656eaf06caaa06").into(),
            state_root: hex!("98b4d1141cfc324ce7095417d81b28995587e9a50ebb4872254187155e6b160c").into(),
            body_root: hex!("c1a089291dbc744be622356dcbdd65fe053dcb908056511e5148f73a3d5c8a7e").into(),
        },
        finality_branch: vec![
            hex!("1500000000000000000000000000000000000000000000000000000000000000").into(),
            hex!("10c726fac935bf9657cc7476d3cfa7bedec5983dcfb59e8a7df6d0a619e108d7").into(),
            hex!("c2374959a44ad09eb825ada636c430d530e656d7f06dccbcf2ca8b6dc834b188").into(),
            hex!("97a3662f859b89f96d7c6ce03496c353df5f6c225455f3f2c5edde329f5a94d1").into(),
            hex!("4c4720ad9a38628c6774dbd1180d026aceb0be3cb0085d531b1e09faf014328a").into(),
            hex!("c3d59497774f8f1c13fda9f9c6b4f773efabbac8a24d914dc7cf02874d5f5658").into(),
        ],
        block_roots_root: hex!("dff54b382531f4af2cb6e5b27ea905cc8e19b48f3ae8e02955f859e6bfd37e42").into(),
        block_roots_branch: vec![
            hex!("8f957e090dec42d80c118d24c0e841681e82d3b330707729cb939d538c208fb7").into(),
            hex!("4d33691095103fbf0df53ae0ea14378d721356b54592019050fc532bfef42d0c").into(),
            hex!("bc9b31cd5d18358bff3038fab52101cfd5c56c75539449d988c5a789200fb264").into(),
            hex!("7d57e424243eeb39169edccf9dab962ba8d80a9575179799bbd509c95316d8df").into(),
            hex!("c07eeb9da14bcedb7dd225a68b92f578ef0b86187724879e5171d5de8a00be3a").into(),
        ]
    })
}

pub fn make_execution_header_update() -> Box<ExecutionHeaderUpdate> {
	Box::new(ExecutionHeaderUpdate {
        header: BeaconHeader {
            slot: 166,
            proposer_index: 7,
            parent_root: hex!("da28a205118aaf4aa69a3fb4eb7a565541b7172cc77771ec886b54b6f5bc10f3").into(),
            state_root: hex!("179a6249c3e86ebcc9c71a0fc604a825a5fb5dd1602177681c54dc7e09af4265").into(),
            body_root: hex!("e1290e50d64f043594bcac66824b04eb3047ae4174188e40106235183f47074c").into(),
        },
        ancestry_proof: Some(AncestryProof {
            header_branch: vec![
                hex!("bb8245e89f5d7d9191c559425b8522487d98b8f2c9b814a158656eaf06caaa06").into(),
                hex!("ab498fef414d709fcac589217246527b82c25706fa800f21d543c83a0ccc59a2").into(),
                hex!("de054acbf636cf9f1692137f78179381b5b0328b49fbc4d324eb8e897b41f52c").into(),
                hex!("c472fde1df644788c92208467ff5aad686ce55bab1570917e8de1922296666fd").into(),
                hex!("11a71c67676c72696c452d875318501cd50968106f72fb611bfe5952285516f8").into(),
                hex!("6d2bd2b6cd84ddadc89df27f3f7f9141bb88c742a30123c77eefebef9d5f6667").into(),
            ],
            finalized_block_root: hex!("be7d9cc4483ed0065fc7c32e2a783ca3782d8dbd7bfe899fd7c0bcee82f11629").into(),
        }),
        execution_header: ExecutionPayloadHeader {
            parent_hash: hex!("96b27b6e0919c19a70c4a2f7136fd59d2e63a3ba0453a86775add3f2dd681cea").into(),
            fee_recipient: hex!("0000000000000000000000000000000000000000").into(),
            state_root: hex!("b847ee60946ebdb5bd92c22385da44b8a9aea4c6779f1a1402cc06e22b76fb4a").into(),
            receipts_root: hex!("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421").into(),
            logs_bloom: hex!("00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").into(),
            prev_randao: hex!("e6c6665aa502e12dd8f5937da2eb7f7fe7a78bc9a900c9321412b8ddd4d72325").into(),
            block_number: 166,
            gas_limit: 68022694,
            gas_used: 0,
            timestamp: 1704700423,
            extra_data: hex!("d983010d05846765746888676f312e32312e318664617277696e").into(),
            base_fee_per_gas: U256::from(7_u64),
            block_hash: hex!("1871ded7b2b8b4b5b358c904104704811b15aeefc24e49daa2a1a68176d6553a").into(),
            transactions_root: hex!("7ffe241ea60187fdb0187bfa22de35d1f9bed7ab061d9401fd47e34a54fbede1").into(),
            withdrawals_root: hex!("28ba1834a3a7b657460ce79fa3a1d909ab8828fd557659d4d0554a9bdbc0ec30").into(),
        },
        execution_branch: vec![
            hex!("276d006ecfe51451787321ef00417b194e90b35d4106bd7d51372f39918a4531").into(),
            hex!("336488033fe5f3ef4ccc12af07b9370b92e553e35ecb4a337a1b1c0e4afe1e0e").into(),
            hex!("db56114e00fdd4c1f85c892bf35ac9a89289aaecb1ebd0a96cde606a748b5d71").into(),
            hex!("6b8ff91d1be713c644ef9b3e5b77323897930c342fd12615c0d2142bee5cd1d7").into(),
        ],
    })
}
