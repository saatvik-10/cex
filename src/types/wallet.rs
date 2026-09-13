use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct DepositInput {
    /// Amount to credit. Must be positive.
    pub amount: BigDecimal,
}

/// One balance line, returned with the amount as a string to avoid
/// float precision loss in JSON.
#[derive(Debug, Serialize)]
pub struct BalanceEntry {
    pub asset: String,
    pub amount: String,
}

#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    pub balances: Vec<BalanceEntry>,
}
