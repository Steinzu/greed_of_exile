use crate::models::*;
use anyhow::{Context, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const USER_AGENT: &str = "GreedOfExile/1.0.0 (contact@example.com)";

const ITEM_TYPES: &[&str] = &[
    "Currency",
    "Fragment",
    "DivinationCard",
    "Artifact",
    "Oil",
    "Incubator",
    "SkillGem",
    "DeliriumOrb",
    "Invitation",
    "Scarab",
    "Astrolabe",
    "Fossil",
    "Resonator",
    "Essence",
    "Vial",
];

fn is_exchange_type(item_type: &str) -> bool {
    !matches!(item_type, "SkillGem")
}

fn ninja_api_url(league: &str, item_type: &str) -> String {
    if is_exchange_type(item_type) {
        format!(
            "https://poe.ninja/poe1/api/economy/exchange/current/overview?league={}&type={}",
            league, item_type
        )
    } else {
        format!(
            "https://poe.ninja/poe1/api/economy/stash/current/item/overview?league={}&type={}",
            league, item_type
        )
    }
}

pub async fn fetch_leagues() -> Result<Vec<String>> {
    let client = Client::builder().user_agent(USER_AGENT).build()?;
    let resp = client
        .get("https://www.pathofexile.com/api/trade/data/leagues")
        .send()
        .await
        .context("Failed to fetch leagues")?;
    let data: PoeTradeLeaguesResponse = resp.json().await.context("Failed to parse leagues")?;
    let mut leagues: Vec<String> = data.result.into_iter().map(|l| l.id).collect();
    leagues.sort();
    leagues.dedup();
    Ok(leagues)
}

pub async fn fetch_ninja_prices(league: &str) -> Result<HashMap<String, f64>> {
    let client = Client::builder().user_agent(USER_AGENT).build()?;
    let mut prices = HashMap::new();

    for &item_type in ITEM_TYPES {
        let url = ninja_api_url(league, item_type);
        let Ok(resp) = client.get(&url).send().await else {
            continue;
        };
        let Ok(data) = resp.json::<NinjaOverviewResponse>().await else {
            continue;
        };

        if is_exchange_type(item_type) {
            // Exchange endpoints have a separate items array for metadata (id -> name mapping)
            // and a lines array with the actual prices keyed by id.
            let id_to_name: HashMap<String, String> = data
                .items
                .iter()
                .filter_map(|item| {
                    let id = item.get("id")?.as_str()?.to_string();
                    let name = item.get("name")?.as_str()?.to_string();
                    Some((id, name))
                })
                .collect();

            for line in &data.lines {
                if let (Some(id), Some(price)) = (
                    line.get("id").and_then(|v| v.as_str()),
                    line.get("primaryValue").and_then(|v| v.as_f64()),
                ) {
                    if let Some(name) = id_to_name.get(id) {
                        prices.insert(name.clone(), price);
                    }
                }
            }
        } else {
            for line in &data.lines {
                if let (Some(name), Some(price)) = (
                    line.get("name").and_then(|v| v.as_str()),
                    line.get("chaosValue").and_then(|v| v.as_f64()),
                ) {
                    prices.insert(name.to_string(), price);
                }
            }
        }
    }

    // Chaos Orb is always 1:1; provide a fallback for Divine Orb if missing.
    prices.insert("Chaos Orb".to_string(), 1.0);
    prices.entry("Divine Orb".to_string()).or_insert(100.0);

    Ok(prices)
}

pub async fn fetch_all_ninja_icons(league: &str) -> HashMap<String, String> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .unwrap_or_default();
    let mut icons = HashMap::new();

    for &item_type in ITEM_TYPES {
        let url = ninja_api_url(league, item_type);
        let Ok(resp) = client.get(&url).send().await else {
            continue;
        };
        let Ok(data) = resp.json::<NinjaOverviewResponse>().await else {
            continue;
        };

        if is_exchange_type(item_type) {
            for item in &data.items {
                if let (Some(name), Some(icon)) = (
                    item.get("name").and_then(|v| v.as_str()),
                    item.get("image").and_then(|v| v.as_str()),
                ) {
                    icons.insert(name.to_string(), icon.to_string());
                }
            }
        } else {
            for line in &data.lines {
                if let (Some(name), Some(icon)) = (
                    line.get("name").and_then(|v| v.as_str()),
                    line.get("icon").and_then(|v| v.as_str()),
                ) {
                    icons.insert(name.to_string(), icon.to_string());
                }
            }
        }
    }

    icons
}

pub async fn fetch_stash_tab(
    account: &str,
    sessid: &str,
    league: &str,
    tab_index: u32,
    fetch_tabs_metadata: bool,
) -> Result<PoeStashResponse> {
    let client = Client::builder().user_agent(USER_AGENT).build()?;
    let tab_index_str = tab_index.to_string();

    let url = url::Url::parse_with_params(
        "https://www.pathofexile.com/character-window/get-stash-items",
        &[
            ("accountName", account),
            ("realm", "pc"),
            ("league", league),
            ("tabs", if fetch_tabs_metadata { "1" } else { "0" }),
            ("tabIndex", &tab_index_str),
        ],
    )
    .context("Failed to build URL")?;

    let resp = client
        .get(url)
        .header("Cookie", format!("POESESSID={}", sessid))
        .send()
        .await
        .context("Failed to fetch stash tab")?;

    let text = resp.text().await?;
    let data: PoeStashResponse = serde_json::from_str(&text).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse stash response. Error: {}, Body: {}",
            e,
            text
        )
    })?;

    if let Some(err) = data.error {
        anyhow::bail!("API Error ({}): {}", err.code, err.message);
    }

    Ok(data)
}

/// Downloads `icon_url` into the local image cache and returns the on-disk path.
/// Returns `None` if the URL is empty, the download fails, or the write fails.
/// If the file is already cached it is returned immediately without a network request.
pub async fn fetch_and_cache_image(icon_url: &str) -> Option<std::path::PathBuf> {
    if icon_url.is_empty() {
        return None;
    }

    let full_url = if icon_url.starts_with("//") {
        format!("https:{}", icon_url)
    } else if icon_url.starts_with("/gen/image") {
        format!("https://web.poecdn.com{}", icon_url)
    } else if icon_url.starts_with('/') {
        format!("https://www.pathofexile.com{}", icon_url)
    } else {
        icon_url.to_string()
    };

    let mut hasher = DefaultHasher::new();
    full_url.hash(&mut hasher);
    let file_path = crate::storage::get_images_dir().join(format!("{:x}.png", hasher.finish()));

    if file_path.exists() {
        return Some(file_path);
    }

    let client = Client::builder().user_agent(USER_AGENT).build().ok()?;
    let resp = client.get(&full_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    std::fs::write(&file_path, bytes).ok()?;
    Some(file_path)
}
