# Option B Spike — Isolated Web Workspace (Tauri 2.12.6 + Wry 0.57.2)

Standalone, removable empirical feasibility spike evaluating Option B (Isolated Web Workspace) against the exact Tauri 2 stack locked in `apps/desktop/src-tauri/Cargo.lock` (`tauri 2.12.6`, `tauri-runtime-wry 2.12.4`, `wry 0.57.2`, `tao 0.37.2`).

## Modes

Build offline against cached workspace crates:
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
cargo build --offline
```

Run tests (always pass a temporary `--data-root` and clean it up afterward):
- `--mode isolation-write --data-root <dir> --port 18432 --out <json>`:
  Verifies same-origin cross-account isolation (`localStorage`, `document.cookie`, `IndexedDB`) between child WebViews A and B, compares Tauri `Window::add_child` vs raw `wry::WebViewBuilder::build_as_child` remote security boundaries, and tests `on_navigation` / `on_new_window` policies.
- `--mode isolation-verify --data-root <dir> --port 18432 --out <json>`:
  Fresh OS process verifying independent persistence after restart plus `hide()`/`show()` vs offscreen bounds switching & timer visibility behavior.
- `--mode zalo-live --data-root <dir> --out <json>`:
  Non-destructive unauthenticated `https://chat.zalo.me` QR login screen check across 1, 2, and 3 isolated WebView2 children with scoped RAM/process overhead measurements.
- `--mode deep-probe --data-root <dir> --port 18433 --out <json>`:
  Deep QR DOM inspection (`224x224` `<svg>` QR inside `.qr-container`), synchronous main-thread COM switch latency, and shared-environment Windows WebView2 `with_profile_name` isolation + RAM benchmark.

## Removal

Delete the entire `desktop-instance-manager/spikes/web-workspace/` directory. No repository files or Cargo workspace manifests reference it.
