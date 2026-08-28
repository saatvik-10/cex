use serde::{Deserialize, Serialize};

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

    /// Canonical string form used as the `balances.asset` column value.
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
}
