use crate::order::Order;

/// Unified result type
pub type UnifiedResult<T> = Result<T, UnifiedError>;

/// Unified error type
#[derive(Debug, Clone)]
pub enum UnifiedError {
    AddOrderError(String),
    AddLimitOrderError(String),
    OrderError(String),
    OrderVerifyError(String),
    IcebergOrderError(String),
    UpdateCurrentPrice(String),
    EventSendError(String),
    Error(String),
    InvalidOrder(String)
}

/// push order event
pub type PushOrderEvent = fn(order: Order);

/// matcher processing process, is used to rewrite the logic of matching orders in the order book.
pub type MatcherProcessingProcess = fn();
