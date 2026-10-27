use codec::{Decode, DecodeWithMemTracking, Encode};
use sc_utils::notification::{NotificationSender, NotificationStream, TracingKeyStr};
use sp_runtime::scale_info::TypeInfo;

// TODO: Put this somewhere else
/// DKG Dealing
#[derive(Clone, Debug, PartialEq, Eq, TypeInfo, Encode, Decode, DecodeWithMemTracking)]
pub struct Dealing<T> {
	pub dealing: T,
}

/// The sending half of the notifications channel(s) used to send
/// notifications about DKG dealing.
pub type DkgDealingSender = NotificationSender<Dealing<Vec<u8>>>;

/// The receiving half of a notifications channel used to receive
/// notifications about DKG dealings.
pub type DkgDealingReceiver =
	NotificationStream<Dealing<Vec<u8>>, DkgDealingTracingKey>;

/// Provides tracing key for DKG dealing stream.
#[derive(Clone)]
pub struct DkgDealingTracingKey;
impl TracingKeyStr for DkgDealingTracingKey {
	const TRACING_KEY: &'static str = "mpsc_dkg_dealing_notification_stream";
}

