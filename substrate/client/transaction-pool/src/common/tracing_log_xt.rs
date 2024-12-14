macro_rules! log_xt {
    (data: hash, target: $target:expr, $level:expr, $tx_collection:expr, $text_with_format:expr) => {
        for tx in $tx_collection {
            tracing::event!(
                $level,
                target = $target,
                message = $text_with_format,
                tx = format!("{:?}", tx)
            );
        }
    };
    (data: hash, target: $target:expr, $level:expr, $tx_collection:expr, $text_with_format:expr, $($arg:expr),*) => {
        for tx in $tx_collection {
            tracing::event!(
                $level,
                target = $target,
                message = $text_with_format,
                tx = format!("{:?}", tx),
                $($arg),*
            );
        }
    };
    (data: tuple, target: $target:expr, $level:expr, $tx_collection:expr, $text_with_format:expr) => {
        for tx in $tx_collection {
            tracing::event!(
                $level,
                target = $target,
                message = $text_with_format,
                tx_0 = format!("{:?}", tx.0),
                tx_1 = format!("{:?}", tx.1)
            );
        }
    };
}
macro_rules! log_xt_trace {
    (data: $datatype:ident, target: $target:expr, $($arg:tt)+) => {
        $crate::common::tracing_log_xt::log_xt!(data: $datatype, target: $target, tracing::Level::TRACE, $($arg)+);
    };
    (target: $target:expr, $tx_collection:expr, $text_with_format:expr) => {
        $crate::common::tracing_log_xt::log_xt!(data: hash, target: $target, tracing::Level::TRACE, $tx_collection, $text_with_format);
    };
    (target: $target:expr, $tx_collection:expr, $text_with_format:expr, $($arg:expr)*) => {
        $crate::common::tracing_log_xt::log_xt!(data: hash, target: $target, tracing::Level::TRACE, $tx_collection, $text_with_format, $($arg)*);
    };
}

pub(crate) use log_xt;
pub(crate) use log_xt_trace;