use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppConfig {
    pub account_name: String,
    pub poesessid: String,
    pub league: String,
    pub tracked_tabs: Vec<u32>,
    #[serde(default)]
    pub disabled_resources: HashMap<u32, HashSet<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryPoint {
    pub timestamp: i64,
    pub total_chaos_value: f64,
    pub total_divine_value: f64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PoeStashTabMeta {
    pub n: String,
    pub i: u32,
    #[serde(rename = "type", default)]
    pub tab_type: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PoeApiError {
    pub code: i32,
    pub message: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PoeStashResponse {
    pub error: Option<PoeApiError>,
    pub tabs: Option<Vec<PoeStashTabMeta>>,
    #[serde(default)]
    pub items: Vec<PoeItem>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PoeItem {
    #[serde(rename = "typeLine", default)]
    pub type_line: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "stackSize")]
    pub stack_size: Option<u32>,
    #[serde(rename = "baseType", default)]
    pub base_type: String,
    #[serde(default)]
    pub icon: String,
}

impl PoeItem {
    /// The human-readable display name: explicit name if present, otherwise the type line.
    pub fn display_name(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else {
            &self.type_line
        }
    }

    /// The name used to look up prices / icons: base type if present, otherwise the type line.
    pub fn lookup_name(&self) -> &str {
        if !self.base_type.is_empty() {
            &self.base_type
        } else {
            &self.type_line
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct PoeTradeLeaguesResponse {
    pub result: Vec<PoeTradeLeague>,
}

#[derive(Deserialize, Debug)]
pub struct PoeTradeLeague {
    pub id: String,
}

#[derive(Deserialize, Debug)]
pub struct NinjaOverviewResponse {
    #[serde(default)]
    pub lines: Vec<serde_json::Value>,
    #[serde(default)]
    pub items: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PriceCache {
    pub last_updated: i64,
    pub prices: std::collections::HashMap<String, f64>,
}
