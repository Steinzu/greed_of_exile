# Greed of Exile

![logo](logo.png)

A lightweight, fast **Path of Exile 1** wealth tracker written in Rust.

[Wealthy Exile](https://wealthyexile.com/) and [Exilence CE](https://github.com/exilence-ce/exilence-ce) inspired me to build this — I wanted something that did the same job but was written in Rust.

---

## Known issues : 

Divination card prices are wildly innacurate sometimes, I would recommend not selecting that tab.

---

## Download

Grab the latest `greed-of-exile.exe` from the [Releases](../../releases) page. No installer needed — just run it.

---

## First-Time Setup

### 1. Account Name

Enter your account name in the **Account** field. It must be in the full format shown on the Path of Exile website:

```
Name#1234
```

You can find it by logging into [pathofexile.com](https://www.pathofexile.com) and checking your account profile.

### 2. POESESSID

Your `POESESSID` is the session cookie that authenticates requests to the PoE API.

To find it:

1. Log in to [pathofexile.com](https://www.pathofexile.com)
2. Press **F12** to open the browser developer tools
3. Go to the **Application** tab (Chrome/Edge) or **Storage** tab (Firefox)
4. Under **Cookies**, select `https://www.pathofexile.com`
5. Find the cookie named `POESESSID` and copy its value

Paste it into the **POESESSID** field in the app. It is stored locally and never sent anywhere except to the official PoE API.

### 3. League

Select your current league from the **League** dropdown. The list is fetched automatically from the PoE API when the app starts.

### 4. Save

Click **Save Config** to persist your settings. You only need to do this once, or whenever any of the above changes.

---

## Features

### Snapshot

Click **Take Snapshot** to fetch all your tracked stash tabs, look up each item's current price from [poe.ninja](https://poe.ninja), and record your total wealth as a data point on the chart.

- Only tabs that are **checked** in the tab list on the left are included in the snapshot total.
- After each snapshot, the change since the last one is displayed next to your current wealth (e.g. **+12.4div** in green, or **-3.1div** in red).

### Wealth Chart

The main panel shows a rolling **3-day wealth chart** in divines, so you can see at a glance whether you are progressing or bleeding currency.

### Auto Snapshot (Interval)

Use the **Interval** dropdown to take snapshots automatically without clicking anything:

| Option | Behaviour |
|---|---|
| Manual | Snapshots only when you click the button |
| 30 minutes | Snapshots every 30 minutes |
| 1 hour | Snapshots every hour |
| 2 hours | Snapshots every 2 hours |
| 4 hours | Snapshots every 4 hours |

When an interval is active, the snapshot button shows a countdown to the next automatic snapshot.

### Fetch Tabs

Click **Fetch Tabs** to retrieve the list of stash tabs from your account. Only supported tab types are shown (Currency, Fragment, Essence, Scarab, Fossil, Delirium, Blight, Ultimatum, and Divination Card stashes). Check the tabs you want to include in future snapshots.

### Fetch Prices

Click **Fetch Prices** to manually refresh item prices from poe.ninja. Prices are cached locally for 24 hours and automatically refreshed when you take a snapshot if the cache is stale.

### Cache Icons

Click **Cache Icons** once the first time you use the app. This downloads all item icons from poe.ninja to a local folder so they display correctly in the stash viewer. It only needs to be done once (or again if new items are added in a new league).

### Stash Viewer

Click any tab in the left panel to browse its contents. Items are displayed in a table sorted by total value (descending), with their icon, stack size, unit price, and total chaos value.

---

## Privacy

- All data is stored **locally** on your machine (under `%LOCALAPPDATA%\GreedOfExile\GreedOfExile`).
- Your POESESSID is only ever sent to `pathofexile.com` to fetch your stash data.
- Price data is fetched from `poe.ninja`. No personal data is sent there.
- Nothing is telemetry'd, logged, or uploaded anywhere.

---

## Building from Source

Requires the [Rust toolchain](https://rustup.rs/) and MSVC (Visual Studio Build Tools).

```sh
git clone https://github.com/Steinzu/greed_of_exile
cd greed_of_exile
cargo build --release
```

The binary will be at `target/release/greedofexile.exe`.

---

## Disclaimer : 

[Zed](https://zed.dev/)'s AI prediction tool has been used to help with development, comments, code reformatting and the readme.

---

## License

MIT
