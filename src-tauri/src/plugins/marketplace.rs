// ---------------- 마켓플레이스 (실제 로컬 데이터) ----------------
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Debug)]
pub(crate) struct MarketplaceInfo {
    id: String,
    repo: Option<String>,
    #[serde(rename = "lastUpdated")]
    last_updated: Option<String>,
}

pub(crate) fn read_known_marketplaces() -> Vec<MarketplaceInfo> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let path = home
        .join(".claude")
        .join("plugins")
        .join("known_marketplaces.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };
    let Some(obj) = root.as_object() else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for (id, value) in obj {
        let repo = value
            .get("source")
            .and_then(|s| s.get("repo"))
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
        let last_updated = value
            .get("lastUpdated")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        results.push(MarketplaceInfo {
            id: id.clone(),
            repo,
            last_updated,
        });
    }
    results.sort_by(|a, b| a.id.cmp(&b.id));
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_marketplaces() {
        let marketplaces = read_known_marketplaces();
        assert!(
            !marketplaces.is_empty(),
            "마켓플레이스를 하나도 찾지 못했습니다"
        );
        assert!(marketplaces.iter().any(|m| m.id == "malgnsoft-plugins"));
    }
}
