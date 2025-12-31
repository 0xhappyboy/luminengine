/// Unified result type
pub type LuminEngineResult<T> = Result<T, LuminEngineError>;

/// Unified error type
#[derive(Debug, Clone)]
pub enum LuminEngineError {
    AddOrderError(String),
    AddLimitOrderError(String),
    OrderError(String),
    OrderVerifyError(String),
    IcebergOrderError(String),
    UpdateCurrentPrice(String),
    EventSendError(String),
    Error(String),
    InvalidOrder(String),
    PriceManagerError(String),
    InvalidStopPrice(String),
    InvalidLimitPrice(String),
    OrderNotModifiable(String),
}

#[derive(Debug, Clone, Copy)]
pub struct OrderFloat(f64);

impl OrderFloat {
    pub fn new(value: f64) -> Self {
        Self(value)
    }

    pub fn into_inner(self) -> f64 {
        self.0
    }
}

impl PartialEq for OrderFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for OrderFloat {}

impl std::hash::Hash for OrderFloat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.0.to_bits());
    }
}

impl PartialOrd for OrderFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for OrderFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}
