use crate::api;
use crate::models::*;
use crate::storage;
use chrono::{Local, Utc};
use eframe::egui;
use egui_extras::{Column, Size, StripBuilder, TableBuilder};
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Events sent from background tasks back to the UI thread
// ---------------------------------------------------------------------------

pub enum UpdateEvent {
    Status(String),
    NewHistory(HistoryPoint),
    PricesUpdated(PriceCache),
    LeaguesFetched(Vec<String>),
    TabsFetched(Vec<PoeStashTabMeta>),
    StashTabContent(u32, Vec<PoeItem>),
    ImageLoaded(String, PathBuf),
}

// ---------------------------------------------------------------------------
// Auto-snapshot interval
// ---------------------------------------------------------------------------

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum SnapshotInterval {
    Manual,
    M30,
    H1,
    H2,
    H4,
}

impl SnapshotInterval {
    fn period_secs(self) -> Option<i64> {
        match self {
            Self::Manual => None,
            Self::M30 => Some(30 * 60),
            Self::H1 => Some(60 * 60),
            Self::H2 => Some(2 * 60 * 60),
            Self::H4 => Some(4 * 60 * 60),
        }
    }

    /// Returns the Unix timestamp at which the *next* interval boundary falls.
    fn next_snapshot_time(self) -> Option<i64> {
        let period = self.period_secs()?;
        let now = Utc::now().timestamp();
        Some(((now / period) + 1) * period)
    }
}

impl std::fmt::Display for SnapshotInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Manual => "Manual",
            Self::M30 => "30 minutes",
            Self::H1 => "1 hour",
            Self::H2 => "2 hours",
            Self::H4 => "4 hours",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// Stash item enriched with pricing data – used only for table rendering
// ---------------------------------------------------------------------------

struct ItemDisplay {
    /// Raw icon URL from the PoE API (used as a cache-key fallback).
    icon_url: String,
    /// Display name / ninja name (first image-cache lookup key).
    ninja_name: String,
    /// Base-type name (second image-cache lookup key).
    ninja_base_type: String,
    /// Human-readable name shown in the table.
    name: String,
    count: u32,
    unit_price_c: f64,
    total_price_c: f64,
}

// ---------------------------------------------------------------------------
// Stash-tab types we care about for tracking/display
// ---------------------------------------------------------------------------

const ALLOWED_TAB_TYPES: &[&str] = &[
    "CurrencyStash",
    "EssenceStash",
    "FragmentStash",
    "DelveStash",
    "BlightStash",
    "DeliriumStash",
    "UltimatumStash",
    "DivinationCardStash",
];

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

pub struct GreedOfExileApp {
    config: AppConfig,
    history: Vec<HistoryPoint>,
    prices: Option<PriceCache>,
    status_msg: String,

    update_tx: Sender<UpdateEvent>,
    update_rx: Receiver<UpdateEvent>,

    interval: SnapshotInterval,

    // League / tab metadata fetched from the API
    available_leagues: Vec<String>,
    available_tabs: Vec<PoeStashTabMeta>,

    // Stash viewer state
    selected_view_tab: Option<u32>,
    view_tab_items: Vec<PoeItem>,

    // Image cache: key → on-disk path
    image_cache: HashMap<String, PathBuf>,
    /// Keys whose images are currently being downloaded.
    fetching_images: HashSet<String>,

    first_frame: bool,
    last_tabs_fetched: Option<i64>,
    last_auto_snapshot: Option<i64>,
    /// Delta in divines between the last two snapshots (positive = gain, negative = loss).
    last_snapshot_delta: Option<f64>,
    /// Set to true once a StashTabContent response arrives for the selected tab.
    view_tab_loaded: bool,
}

// ---------------------------------------------------------------------------
// Construction + background-task helpers
// ---------------------------------------------------------------------------

impl GreedOfExileApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let (update_tx, update_rx) = std::sync::mpsc::channel();

        Self {
            config: storage::load_config(),
            history: storage::load_history(),
            prices: storage::load_prices(),
            image_cache: storage::load_image_map(),
            status_msg: "Idle".to_string(),
            update_tx,
            update_rx,
            interval: SnapshotInterval::Manual,
            available_leagues: Vec::new(),
            available_tabs: Vec::new(),
            selected_view_tab: None,
            view_tab_items: Vec::new(),
            fetching_images: HashSet::new(),
            first_frame: true,
            last_tabs_fetched: None,
            last_auto_snapshot: None,
            last_snapshot_delta: None,
            view_tab_loaded: false,
        }
    }

    /// Returns `false` and sets a status message when POESESSID is not configured.
    fn has_session(&mut self) -> bool {
        if self.config.poesessid.is_empty() {
            self.status_msg = "POESESSID is missing. Configure it.".to_string();
            false
        } else {
            true
        }
    }

    fn fetch_prices(&mut self, ctx: egui::Context) {
        self.status_msg = "Fetching ninja prices...".to_string();
        let tx = self.update_tx.clone();
        let league = self.config.league.clone();
        tokio::spawn(async move {
            match api::fetch_ninja_prices(&league).await {
                Ok(new_prices) => {
                    let cache = PriceCache {
                        last_updated: Utc::now().timestamp(),
                        prices: new_prices,
                    };
                    let _ = storage::save_prices(&cache);
                    let _ = tx.send(UpdateEvent::PricesUpdated(cache));
                    let _ = tx.send(UpdateEvent::Status(
                        "Prices fetched successfully".to_string(),
                    ));
                }
                Err(_) => {
                    let _ = tx.send(UpdateEvent::Status("Failed to fetch prices".to_string()));
                }
            }
            ctx.request_repaint();
        });
    }

    fn fetch_leagues(&mut self, ctx: egui::Context) {
        self.status_msg = "Fetching leagues...".to_string();
        let tx = self.update_tx.clone();
        tokio::spawn(async move {
            match api::fetch_leagues().await {
                Ok(leagues) => {
                    let _ = tx.send(UpdateEvent::LeaguesFetched(leagues));
                }
                Err(_) => {
                    let _ = tx.send(UpdateEvent::Status("Failed to fetch leagues".to_string()));
                }
            }
            ctx.request_repaint();
        });
    }

    fn fetch_tabs_metadata(&mut self, ctx: egui::Context) {
        if !self.has_session() {
            return;
        }
        self.status_msg = "Fetching stash tabs...".to_string();
        let tx = self.update_tx.clone();
        let account = self.config.account_name.clone();
        let sessid = self.config.poesessid.clone();
        let league = self.config.league.clone();
        tokio::spawn(async move {
            match api::fetch_stash_tab(&account, &sessid, &league, 0, true).await {
                Ok(stash) => {
                    if let Some(tabs) = stash.tabs {
                        let _ = tx.send(UpdateEvent::TabsFetched(tabs));
                        let _ =
                            tx.send(UpdateEvent::Status("Tabs fetched successfully".to_string()));
                    } else {
                        let _ = tx.send(UpdateEvent::Status("No tabs in response".to_string()));
                    }
                }
                Err(e) => {
                    let _ = tx.send(UpdateEvent::Status(format!("Failed to fetch tabs: {}", e)));
                }
            }
            ctx.request_repaint();
        });
    }

    fn fetch_view_tab(&mut self, tab_index: u32, ctx: egui::Context) {
        if !self.has_session() {
            return;
        }
        self.status_msg = format!("Fetching tab {}...", tab_index);
        let tx = self.update_tx.clone();
        let account = self.config.account_name.clone();
        let sessid = self.config.poesessid.clone();
        let league = self.config.league.clone();
        tokio::spawn(async move {
            match api::fetch_stash_tab(&account, &sessid, &league, tab_index, false).await {
                Ok(stash) => {
                    let _ = tx.send(UpdateEvent::StashTabContent(tab_index, stash.items));
                    let _ = tx.send(UpdateEvent::Status("Tab content loaded".to_string()));
                }
                Err(_) => {
                    let _ = tx.send(UpdateEvent::Status(
                        "Failed to fetch tab content".to_string(),
                    ));
                }
            }
            ctx.request_repaint();
        });
    }

    fn take_snapshot(&mut self, ctx: egui::Context) {
        if !self.has_session() {
            return;
        }
        let config = self.config.clone();
        let tx = self.update_tx.clone();
        let mut prices = self.prices.clone();

        tokio::spawn(async move {
            let _ = tx.send(UpdateEvent::Status("Checking prices...".to_string()));
            ctx.request_repaint();

            let now = Utc::now().timestamp();
            let prices_stale = prices
                .as_ref()
                .map_or(true, |p| now - p.last_updated > 86400);

            if prices_stale {
                match api::fetch_ninja_prices(&config.league).await {
                    Ok(new_prices) => {
                        let cache = PriceCache {
                            last_updated: now,
                            prices: new_prices,
                        };
                        let _ = storage::save_prices(&cache);
                        let _ = tx.send(UpdateEvent::PricesUpdated(cache.clone()));
                        prices = Some(cache);
                        ctx.request_repaint();
                    }
                    Err(_) => {
                        let _ = tx.send(UpdateEvent::Status("Failed to fetch prices".to_string()));
                        ctx.request_repaint();
                        return;
                    }
                }
            }

            let _ = tx.send(UpdateEvent::Status("Fetching stash tabs...".to_string()));
            ctx.request_repaint();

            let mut total_chaos = 0.0_f64;
            let mut success = true;

            if let Some(p) = &prices {
                for &tab_idx in &config.tracked_tabs {
                    match api::fetch_stash_tab(
                        &config.account_name,
                        &config.poesessid,
                        &config.league,
                        tab_idx,
                        false,
                    )
                    .await
                    {
                        Ok(stash) => {
                            for item in &stash.items {
                                if config
                                    .disabled_resources
                                    .get(&tab_idx)
                                    .map_or(false, |s| s.contains(item.lookup_name()))
                                {
                                    continue;
                                }
                                let count = item.stack_size.unwrap_or(1) as f64;
                                let price = p
                                    .prices
                                    .get(item.lookup_name())
                                    .or_else(|| p.prices.get(item.display_name()))
                                    .copied()
                                    .unwrap_or(0.0);
                                total_chaos += price * count;
                            }
                        }
                        Err(_) => {
                            success = false;
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }

            if success {
                let divine_price = prices
                    .as_ref()
                    .and_then(|p| p.prices.get("Divine Orb").copied())
                    .unwrap_or(100.0);
                let point = HistoryPoint {
                    timestamp: Utc::now().timestamp(),
                    total_chaos_value: total_chaos,
                    total_divine_value: total_chaos / divine_price,
                };
                let _ = tx.send(UpdateEvent::NewHistory(point));
                let _ = tx.send(UpdateEvent::Status(format!(
                    "Last updated: {}",
                    Local::now().format("%H:%M:%S")
                )));
            } else {
                let _ = tx.send(UpdateEvent::Status(
                    "Error fetching stash tabs. Check settings.".to_string(),
                ));
            }
            ctx.request_repaint();
        });
    }

    // -----------------------------------------------------------------------
    // Event processing
    // -----------------------------------------------------------------------

    /// Drains the update channel and applies all queued events.
    /// Returns `true` when the image cache was modified and should be persisted.
    fn process_events(&mut self) -> bool {
        let mut image_cache_dirty = false;
        while let Ok(event) = self.update_rx.try_recv() {
            match event {
                UpdateEvent::Status(s) => self.status_msg = s,
                UpdateEvent::NewHistory(p) => {
                    self.last_snapshot_delta = self
                        .history
                        .last()
                        .map(|prev| p.total_divine_value - prev.total_divine_value);
                    self.history.push(p);
                    let _ = storage::save_history(&self.history);
                }
                UpdateEvent::PricesUpdated(p) => self.prices = Some(p),
                UpdateEvent::LeaguesFetched(leagues) => {
                    self.available_leagues = leagues;
                    self.status_msg = "Leagues fetched".to_string();
                }
                UpdateEvent::TabsFetched(tabs) => {
                    self.available_tabs = tabs
                        .into_iter()
                        .filter(|t| ALLOWED_TAB_TYPES.contains(&t.tab_type.as_str()))
                        .collect();
                    self.last_tabs_fetched = Some(Utc::now().timestamp());
                }
                UpdateEvent::StashTabContent(idx, items) => {
                    if self.selected_view_tab == Some(idx) {
                        self.view_tab_items = items;
                        self.view_tab_loaded = true;
                    }
                }
                UpdateEvent::ImageLoaded(key, path) => {
                    self.fetching_images.remove(&key);
                    self.image_cache.insert(key, path);
                    image_cache_dirty = true;
                }
            }
        }
        image_cache_dirty
    }

    // -----------------------------------------------------------------------
    // UI helpers
    // -----------------------------------------------------------------------

    fn render_top_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.add_space(4.0);

        // ── Row 1: account credentials + league + interval ──────────────────
        ui.horizontal(|ui| {
            ui.label("Account:");
            ui.add(egui::TextEdit::singleline(&mut self.config.account_name).desired_width(120.0));

            ui.label("POESESSID:");
            ui.add(
                egui::TextEdit::singleline(&mut self.config.poesessid)
                    .desired_width(120.0)
                    .password(true),
            );

            ui.label("League:");
            egui::ComboBox::from_id_salt("league_combo")
                .selected_text(&self.config.league)
                .show_ui(ui, |ui| {
                    // Clone to avoid holding an immutable borrow of self while
                    // selectable_value takes a mutable borrow.
                    let leagues = self.available_leagues.clone();
                    if leagues.is_empty() {
                        let cur = self.config.league.clone();
                        ui.selectable_value(&mut self.config.league, cur.clone(), cur);
                    } else {
                        for l in &leagues {
                            ui.selectable_value(&mut self.config.league, l.clone(), l);
                        }
                    }
                });

            ui.label("Interval:");
            egui::ComboBox::from_id_salt("interval_combo")
                .selected_text(self.interval.to_string())
                .show_ui(ui, |ui| {
                    for opt in [
                        SnapshotInterval::Manual,
                        SnapshotInterval::M30,
                        SnapshotInterval::H1,
                        SnapshotInterval::H2,
                        SnapshotInterval::H4,
                    ] {
                        ui.selectable_value(&mut self.interval, opt, opt.to_string());
                    }
                });
        });

        ui.add_space(4.0);

        // ── Row 2: action buttons + status ──────────────────────────────────
        ui.horizontal(|ui| {
            if ui.button("Save Config").clicked() {
                let _ = storage::save_config(&self.config);
                self.status_msg = "Config saved".to_string();
            }

            if ui.button("Cache Icons").clicked() {
                // Collect icons from the current tab view that are not yet cached.
                let pending: Vec<String> = self
                    .view_tab_items
                    .iter()
                    .filter(|item| {
                        !item.icon.is_empty() && !self.image_cache.contains_key(&item.icon)
                    })
                    .filter_map(|item| {
                        self.fetching_images
                            .insert(item.icon.clone())
                            .then(|| item.icon.clone())
                    })
                    .collect();

                self.status_msg = "Fetching all category icons...".to_string();
                let tx = self.update_tx.clone();
                let ctx_clone = ctx.clone();
                let league = self.config.league.clone();
                tokio::spawn(async move {
                    let icons = api::fetch_all_ninja_icons(&league).await;
                    let mut count = 0usize;
                    for (name, icon_url) in icons {
                        if let Some(path) = api::fetch_and_cache_image(&icon_url).await {
                            let _ = tx.send(UpdateEvent::ImageLoaded(name, path));
                            count += 1;
                        }
                    }
                    for icon_url in pending {
                        if let Some(path) = api::fetch_and_cache_image(&icon_url).await {
                            let _ = tx.send(UpdateEvent::ImageLoaded(icon_url, path));
                            count += 1;
                        }
                    }
                    let _ = tx.send(UpdateEvent::Status(format!("Cached {} icons.", count)));
                    ctx_clone.request_repaint();
                });
            }

            let tabs_hover = self
                .last_tabs_fetched
                .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                .map(|dt| {
                    dt.with_timezone(&Local)
                        .format("Last fetched: %Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_else(|| "Never fetched".to_string());
            if ui.button("Fetch Tabs").on_hover_text(tabs_hover).clicked() {
                self.fetch_tabs_metadata(ctx.clone());
            }

            let prices_hover = self
                .prices
                .as_ref()
                .and_then(|p| chrono::DateTime::from_timestamp(p.last_updated, 0))
                .map(|dt| {
                    dt.with_timezone(&Local)
                        .format("Last fetched: %Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_else(|| "Never fetched".to_string());
            if ui
                .button("Fetch Prices")
                .on_hover_text(prices_hover)
                .clicked()
            {
                self.fetch_prices(ctx.clone());
            }

            ui.separator();

            // Snapshot button – shows a countdown when an interval is active.
            let btn_label = self
                .interval
                .next_snapshot_time()
                .and_then(|next| {
                    let diff = next - Utc::now().timestamp();
                    if diff > 0 {
                        ctx.request_repaint_after(Duration::from_secs(1));
                        Some(format!("Take snapshot ({}m {}s)", diff / 60, diff % 60))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "Take Snapshot".to_string());
            if ui.button(&btn_label).clicked() {
                self.take_snapshot(ctx.clone());
            }

            let del_resp = ui
                .add(
                    egui::Button::new("🗑 Delete Snapshot")
                        .fill(egui::Color32::from_rgb(100, 30, 30)),
                )
                .on_hover_text(
                    "Remove the last snapshot.\nShift-click to wipe the entire history.",
                );
            if del_resp.clicked() {
                if ui.input(|i| i.modifiers.shift) {
                    self.history.clear();
                    self.last_snapshot_delta = None;
                    let _ = storage::save_history(&self.history);
                    self.status_msg = "History wiped.".to_string();
                } else if !self.history.is_empty() {
                    self.history.pop();
                    self.last_snapshot_delta = self
                        .history
                        .windows(2)
                        .last()
                        .map(|w| w[1].total_divine_value - w[0].total_divine_value);
                    let _ = storage::save_history(&self.history);
                    self.status_msg = "Last snapshot deleted.".to_string();
                }
            }

            ui.separator();
            ui.label(format!("Status: {}", self.status_msg));
        });

        ui.add_space(4.0);
    }

    fn render_wealth_chart(&self, ui: &mut egui::Ui) {
        let now = Utc::now();
        // Show a 3-day window: midnight two days ago → midnight tonight.
        let window_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp()
            - 2 * 24 * 3600;
        let window_end = window_start + 3 * 24 * 3600;

        let history: Vec<&HistoryPoint> = self
            .history
            .iter()
            .filter(|p| p.timestamp >= window_start && p.timestamp <= window_end)
            .collect();

        let current_divs = history.last().map_or(0.0, |p| p.total_divine_value);

        ui.horizontal(|ui| {
            ui.heading("Wealth");
            ui.label(
                egui::RichText::new(format!("{:.1} Divs", current_divs))
                    .heading()
                    .color(egui::Color32::from_rgb(255, 215, 0)),
            );
            if let Some(delta) = self.last_snapshot_delta {
                let (sign, color) = if delta >= 0.0 {
                    ("+", egui::Color32::from_rgb(80, 200, 80))
                } else {
                    ("", egui::Color32::from_rgb(220, 80, 80))
                };
                ui.label(
                    egui::RichText::new(format!("{}{:.1}div", sign, delta))
                        .heading()
                        .color(color)
                        .strong(),
                );
            }
        });

        let points: PlotPoints = history
            .iter()
            .map(|p| {
                let day = (p.timestamp - window_start) as f64 / (24.0 * 3600.0);
                [day, p.total_divine_value]
            })
            .collect();

        Plot::new("history_plot")
            .height(150.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .show_axes([true, true])
            .show_grid(true)
            .x_axis_formatter(|x, _range| match x.value.floor() as i64 {
                0 => "2 Days Ago".to_string(),
                1 => "Yesterday".to_string(),
                2 => "Today".to_string(),
                _ => String::new(),
            })
            .label_formatter(move |_name, value| {
                let ts = window_start + (value.x * 24.0 * 3600.0) as i64;
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| {
                        format!(
                            "{}\n{:.1} Divs",
                            dt.with_timezone(&Local).format("%H:%M"),
                            value.y
                        )
                    })
                    .unwrap_or_else(|| format!("{:.1} Divs", value.y))
            })
            .include_x(0.0)
            .include_x(2.99)
            .include_y(0.0)
            .show(ui, |plot_ui| {
                plot_ui.line(Line::new("Divine Value", points))
            });
    }

    fn render_tab_list(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Stash Tabs");

        let mut clicked_tab: Option<u32> = None;
        egui::ScrollArea::vertical()
            .id_salt("left_tabs_scroll")
            .show(ui, |ui| {
                let mut new_tracked = self.config.tracked_tabs.clone();
                for tab in &self.available_tabs {
                    ui.horizontal(|ui| {
                        let mut tracked = new_tracked.contains(&tab.i);
                        if ui.checkbox(&mut tracked, "").changed() {
                            if tracked {
                                new_tracked.push(tab.i);
                            } else {
                                new_tracked.retain(|&x| x != tab.i);
                            }
                        }
                        let selected = self.selected_view_tab == Some(tab.i);
                        if ui
                            .selectable_label(selected, format!("{} ({})", tab.n, tab.tab_type))
                            .clicked()
                        {
                            clicked_tab = Some(tab.i);
                        }
                    });
                }
                self.config.tracked_tabs = new_tracked;
            });

        if let Some(tab_i) = clicked_tab {
            self.selected_view_tab = Some(tab_i);
            self.view_tab_items.clear();
            self.view_tab_loaded = false;
            self.fetch_view_tab(tab_i, ctx.clone());
        }
    }

    fn render_tab_content(&mut self, ui: &mut egui::Ui) {
        ui.heading("Tab Content");

        let Some(tab_idx) = self.selected_view_tab else {
            ui.label("Select a tab to view its items.");
            return;
        };

        if !self.view_tab_loaded {
            ui.label(format!("Loading items for tab {}...", tab_idx));
            return;
        }

        if self.view_tab_items.is_empty() {
            ui.label("This tab is empty.");
            return;
        }

        // Build enriched display rows.
        let mut rows: Vec<ItemDisplay> = self
            .view_tab_items
            .iter()
            .map(|item| {
                let name = item.display_name().to_string();
                let lookup = item.lookup_name().to_string();
                let count = item.stack_size.unwrap_or(1);
                let unit_price_c = self
                    .prices
                    .as_ref()
                    .and_then(|p| {
                        p.prices
                            .get(&lookup)
                            .or_else(|| p.prices.get(&name))
                            .copied()
                    })
                    .unwrap_or(0.0);
                ItemDisplay {
                    icon_url: item.icon.clone(),
                    ninja_name: name.clone(),
                    ninja_base_type: lookup,
                    name,
                    count,
                    unit_price_c,
                    total_price_c: unit_price_c * count as f64,
                }
            })
            .collect();

        // Most valuable items first.
        rows.sort_by(|a, b| {
            b.total_price_c
                .partial_cmp(&a.total_price_c)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Pre-compute per-row display data to avoid borrowing `self` inside nested closures.
        let row_tracked: Vec<bool> = rows
            .iter()
            .map(|row| {
                !self
                    .config
                    .disabled_resources
                    .get(&tab_idx)
                    .map_or(false, |s| s.contains(&row.ninja_base_type))
            })
            .collect();

        let image_uris: Vec<Option<String>> = rows
            .iter()
            .map(|row| {
                self.image_cache
                    .get(&row.ninja_name)
                    .or_else(|| self.image_cache.get(&row.ninja_base_type))
                    .or_else(|| self.image_cache.get(&row.icon_url))
                    .map(|p| format!("file://{}", p.display()).replace('\\', "/"))
            })
            .collect();

        let is_fetching: Vec<bool> = rows
            .iter()
            .map(|row| self.fetching_images.contains(&row.icon_url))
            .collect();

        let mut pending_toggles: Vec<(String, bool)> = Vec::new();

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(24.0)) // Tracked
            .column(Column::exact(36.0)) // Icon
            .column(Column::auto()) // Amount
            .column(Column::initial(250.0).clip(true)) // Name
            .column(Column::auto()) // Unit price
            .column(Column::remainder()) // Total price
            .min_scrolled_height(0.0)
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("✔")
                        .on_hover_text("Toggle wealth tracking for this item");
                });
                header.col(|ui| {
                    ui.strong("Icon");
                });
                header.col(|ui| {
                    ui.strong("Amount");
                });
                header.col(|ui| {
                    ui.strong("Item");
                });
                header.col(|ui| {
                    ui.strong("Price (1)");
                });
                header.col(|ui| {
                    ui.strong("Total (c)");
                });
            })
            .body(|mut body| {
                for (i, row) in rows.iter().enumerate() {
                    let mut tracked = row_tracked[i];
                    body.row(36.0, |mut r| {
                        r.col(|ui| {
                            if ui
                                .checkbox(&mut tracked, "")
                                .on_hover_text("Include in wealth snapshot")
                                .changed()
                            {
                                pending_toggles.push((row.ninja_base_type.clone(), tracked));
                            }
                        });
                        r.col(|ui| {
                            if let Some(uri) = &image_uris[i] {
                                ui.add(
                                    egui::Image::new(uri.as_str())
                                        .fit_to_exact_size(egui::vec2(32.0, 32.0)),
                                );
                            } else if is_fetching[i] {
                                ui.spinner();
                            } else if !row.icon_url.is_empty() {
                                ui.label("-");
                            }
                        });
                        r.col(|ui| {
                            ui.label(row.count.to_string());
                        });
                        r.col(|ui| {
                            ui.label(&row.name);
                        });
                        r.col(|ui| {
                            ui.label(if row.unit_price_c > 0.0 {
                                format!("{:.1}c", row.unit_price_c)
                            } else {
                                "-".to_string()
                            });
                        });
                        r.col(|ui| {
                            ui.label(if row.total_price_c > 0.0 {
                                format!("{:.1}c", row.total_price_c)
                            } else {
                                "-".to_string()
                            });
                        });
                    });
                }
            });

        // Apply tracked/untracked toggles collected during table rendering.
        for (key, now_tracked) in pending_toggles {
            let set = self.config.disabled_resources.entry(tab_idx).or_default();
            if now_tracked {
                set.remove(&key);
            } else {
                set.insert(key);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for GreedOfExileApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── First-frame initialisation ───────────────────────────────────────
        if self.first_frame {
            self.first_frame = false;
            self.fetch_leagues(ctx.clone());
            if self.has_session() {
                self.fetch_tabs_metadata(ctx.clone());
                self.fetch_prices(ctx.clone());
            }
        }

        // ── Auto-snapshot ────────────────────────────────────────────────────
        if let Some(period) = self.interval.period_secs() {
            let now = Utc::now().timestamp();
            let boundary = (now / period) * period;
            let should_snapshot = self.last_auto_snapshot.map_or(true, |ts| ts < boundary);
            if should_snapshot {
                self.last_auto_snapshot = Some(boundary);
                self.take_snapshot(ctx.clone());
            }
        }

        // ── Drain the event channel ──────────────────────────────────────────
        if self.process_events() {
            let _ = storage::save_image_map(&self.image_cache);
        }

        // ── Top panel ───────────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            self.render_top_panel(ctx, ui);
        });

        // ── Central panel ───────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_wealth_chart(ui);
            ui.separator();
            StripBuilder::new(ui)
                .size(Size::relative(0.3))
                .size(Size::remainder())
                .horizontal(|mut strip| {
                    strip.cell(|ui| {
                        self.render_tab_list(ctx, ui);
                    });
                    strip.cell(|ui| {
                        self.render_tab_content(ui);
                    });
                });
        });
    }
}
