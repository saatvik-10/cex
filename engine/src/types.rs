use bigdecimal::{BigDecimal, Zero};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The supported assets on this exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Asset {
    Usd,
    Sol,
    Eth,
}

impl Asset {
    /// Every asset, in a deterministic order.
    pub const ALL: [Asset; 3] = [Asset::Usd, Asset::Sol, Asset::Eth];

    /// Canonical string form used as the wire/DB value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Asset::Usd => "USD",
            Asset::Sol => "SOL",
            Asset::Eth => "ETH",
        }
    }

    /// Parse from its canonical string form.
    pub fn parse(s: &str) -> Option<Asset> {
        match s.to_ascii_uppercase().as_str() {
            "USD" => Some(Asset::Usd),
            "SOL" => Some(Asset::Sol),
            "ETH" => Some(Asset::Eth),
            _ => None,
        }
    }
}

/// Format an amount for the wire. Postgres stores NUMERIC(36, 18), so we emit
/// 18 fractional digits to match; zero is emitted as "0" for readability.
pub fn fmt_amount(amount: &BigDecimal) -> String {
    if amount.is_zero() {
        "0".to_string()
    } else {
        amount.with_scale(18).to_string()
    }
}

/// A command produced by the backend and consumed by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    Deposit {
        reply_key: String,
        user_id: Uuid,
        asset: Asset,
        amount: BigDecimal,
        #[serde(default)]
        idempotency_key: Option<String>,
    },
    GetBalances {
        reply_key: String,
        user_id: Uuid,
    },
    Ping {
        reply_key: String,
    },
}

/// A result produced by the engine and published for the backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    DepositResult {
        reply_key: String,
        asset: String,
        amount: String,
    },
    BalancesResult {
        reply_key: String,
        balances: Vec<BalanceLine>,
    },
    Pong {
        reply_key: String,
    },
    /// A command the engine could not honour (bad asset, unparseable payload,
    /// ...). Always carries the command's reply_key so the backend can answer
    /// the waiting request instead of timing out.
    Error {
        reply_key: String,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceLine {
    pub asset: String,
    pub amount: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_string_roundtrip() {
        for asset in Asset::ALL {
            assert_eq!(Asset::parse(asset.as_str()), Some(asset));
        }
        assert_eq!(Asset::parse("btc"), None);
    }

    #[test]
    fn serde_upper() {
        let v = serde_json::to_string(&Asset::Sol).unwrap();
        assert_eq!(v, "\"SOL\"");
    }

    #[test]
    fn fmt_amount_scale() {
        assert_eq!(fmt_amount(&BigDecimal::from(0)), "0");
        assert_eq!(
            fmt_amount(&"100".parse::<BigDecimal>().unwrap()),
            "100.000000000000000000"
        );
    }

    #[test]
    fn command_payload_roundtrip() {
        let cmd = Command::Deposit {
            reply_key: "rk-1".into(),
            user_id: Uuid::new_v4(),
            asset: Asset::Sol,
            amount: "1.5".parse().unwrap(),
            idempotency_key: Some("dup-1".into()),
        };
        let wire = serde_json::to_string(&cmd).unwrap();
        let back: Command = serde_json::from_str(&wire).unwrap();
        match back {
            Command::Deposit {
                asset,
                idempotency_key,
                ..
            } => {
                assert_eq!(asset, Asset::Sol);
                assert_eq!(idempotency_key.as_deref(), Some("dup-1"));
            }
            _ => panic!("wrong variant"),
        }
    }
}
