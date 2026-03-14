use crate::models::*;
use anyhow::Result;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

fn get_data_dir() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "StatOfExile", "StatOfExile") {
        let dir = proj_dirs.data_local_dir();
        if !dir.exists() {
            fs::create_dir_all(dir).unwrap_or_default();
        }
        dir.to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

pub fn get_images_dir() -> PathBuf {
    let dir = get_data_dir().join("images");
    if !dir.exists() {
        fs::create_dir_all(&dir).unwrap_or_default();
    }
    dir
}

pub fn load_config() -> AppConfig {
    let path = get_data_dir().join("config.json");
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        AppConfig::default()
    }
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    let path = get_data_dir().join("config.json");
    let data = serde_json::to_string_pretty(config)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn load_history() -> Vec<HistoryPoint> {
    let path = get_data_dir().join("history.json");
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn save_history(history: &[HistoryPoint]) -> Result<()> {
    let path = get_data_dir().join("history.json");
    let data = serde_json::to_string_pretty(history)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn load_prices() -> Option<PriceCache> {
    let path = get_data_dir().join("prices.json");
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).ok()
    } else {
        None
    }
}

pub fn save_prices(prices: &PriceCache) -> Result<()> {
    let path = get_data_dir().join("prices.json");
    let data = serde_json::to_string_pretty(prices)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn load_image_map() -> std::collections::HashMap<String, PathBuf> {
    let path = get_data_dir().join("image_map.json");
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    }
}

pub fn save_image_map(map: &std::collections::HashMap<String, PathBuf>) -> Result<()> {
    let path = get_data_dir().join("image_map.json");
    let data = serde_json::to_string_pretty(map)?;
    fs::write(&path, data)?;
    Ok(())
}
