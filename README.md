# Grove Lite

A very small browser. A tab strip, an address bar and a page. Nothing else.

Built with Rust and [Tauri 2](https://tauri.app). 

## What it does

- Opens to a search bar and a tab strip, with a plain charcoal page behind them
- Tabs: open, close, switch. Links that open a new window open as a new tab
- One address bar: type a site (`example.com`, `localhost:3000`) or a search
- History on `Ctrl+H`, with no button. Type to filter, `Enter` to open, `Esc` to close
- No bookmarks, no menus, no settings

Searches go to DuckDuckGo. History is a small file on your computer and is never uploaded.

## Keyboard

`Ctrl` is `Cmd` on macOS.

| Keys | Action |
| --- | --- |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+L` | Focus the address bar |
| `Ctrl+H` | History |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Ctrl+1` to `8` | Jump to tab |
| `Ctrl+9` | Last tab |
| `Ctrl+R` or `F5` | Reload |
| `Alt+Left` / `Alt+Right` | Back / forward (`Cmd+[` / `Cmd+]` on macOS) |

On the history page, `Ctrl+Shift+Delete` clears all history.

## Run it

You need [Rust](https://rustup.rs) (stable).

**Linux** also needs the Tauri system packages:

```sh
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

**Windows** needs the Visual Studio C++ Build Tools. WebView2 is already included in Windows 10 and 11.

**macOS** needs the Xcode command line tools: `xcode-select --install`.

Then:

```sh
git clone https://github.com/heykodex/grove-lite.git
cd grove-lite
cargo run --release -p grove-lite
```

## Build an installer

```sh
cargo install tauri-cli --version "^2" --locked
cd src-tauri
cargo tauri build
```

The installer ends up in `target/release/bundle/`.

## Project layout

| Path | What's there |
| --- | --- |
| `src-tauri/` | The app: window, tabs, shortcuts |
| `crates/grove-core/` | Logic with no UI: address parsing, tabs, history |
| `ui/` | The tab strip, address bar and history page (plain HTML, CSS, JS) |
| `site/` | The homepage |

## Licence

MIT. See `LICENSE`.
