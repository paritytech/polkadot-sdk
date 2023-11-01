use sp_std::marker::PhantomData;
use frame_support::traits::{ProcessMessage, ProcessMessageError};
use frame_support::weights::WeightMeter;
use cumulus_primitives_core::{AggregateMessageOrigin, MessageOrigin};

pub struct BridgeHubMessageProcessor<XcmProcessor, SnowbridgeProcessor>(PhantomData<(XcmProcessor, SnowbridgeProcessor)>)
where
	XcmProcessor: ProcessMessage<Origin = MessageOrigin>,
	SnowbridgeProcessor: ProcessMessage<Origin = MessageOrigin>;

impl<
	XcmProcessor,
	SnowbridgeProcessor
> ProcessMessage for BridgeHubMessageProcessor<
	XcmProcessor,
	SnowbridgeProcessor
>
where
	XcmProcessor: ProcessMessage<Origin = MessageOrigin>,
	SnowbridgeProcessor: ProcessMessage<Origin = MessageOrigin>
{
	type Origin = AggregateMessageOrigin;

	fn process_message(
		message: &[u8],
		origin: Self::Origin,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		use AggregateMessageOrigin::*;
		match origin {
			Xcm(inner) => XcmProcessor::process_message(message, inner, meter, id),
			Other(inner) => SnowbridgeProcessor::process_message(message, inner, meter, id)
		}
	}
}
