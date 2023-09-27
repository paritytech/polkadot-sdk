use polkadot_runtime_common::xcm_sender::PriceForParachainDelivery;
use xcm::latest::prelude::*;

pub fn query_response_delivery_fees<P: PriceForParachainDelivery>(querier: MultiLocation) -> u128 {
	// Message to calculate delivery fees, it's encoded size is what's important.
    // This message reports that there was no error, if an error is reported, the encoded size would be different.
    let message = Xcm(vec![
        QueryResponse {
            query_id: 0, // Dummy query id
            response: Response::ExecutionResult(None),
            max_weight: Weight::zero(),
            querier: Some(querier),
        },
        SetTopic([0u8; 32]), // Dummy topic
    ]);
    let Parachain(para_id) = querier.interior().last().unwrap() else { unreachable!("Location is parachain") };
    let delivery_fees = P::price_for_parachain_delivery((*para_id).into(), &message);
	let Fungible(delivery_fees_amount) = delivery_fees.inner()[0].fun else { unreachable!("Asset is fungible") };
    delivery_fees_amount
}
