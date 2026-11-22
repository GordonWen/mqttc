# Architecture

[简体中文](ARCHITECTURE.md) | English

This document describes the workspace layout, the responsibilities of the shared
`mqttc-core`, the threading and event model, and how the three UIs plug into the same
business logic with minimal differences.

## 1. Workspace layout

```text
Cargo.toml            workspace: core, apps/*, tools/test-broker
core/                 mqttc-core — all UI-agnostic business logic
apps/egui/            eframe 0.37 (glow/OpenGL)
apps/iced/            Iced 0.14 (tiny-skia software rendering, released build)
apps/gpui/            GPUI 0.2.2 (native GPU)
tools/test-broker/    local broker with embedded rumqttd 0.20 + retained fixtures + streaming publish
tools/bench/          PowerShell sampling / scenario orchestration / fixtures (external server config local only)
scripts/              build, packaging and release scripts
.github/workflows/    release.yml: tag-triggered Linux x64 and macOS universal builds
```

The early Slint version (v0.1.1) and `bench/ref-build` are kept only as local performance
baselines. They are not distributed with the repository and are not part of the workspace;
no build command in this repository touches them.

Release builds only `mqttc-iced`, and the artifact is renamed to `mqttc`. Windows is
packaged locally by `scripts/release-windows.ps1`; Linux and macOS are built by
`.github/workflows/release.yml` and attached to the same GitHub Release together with the
Windows build. See [RELEASING.en.md](RELEASING.en.md) for details.

## 2. Shared core `mqttc-core`

The three UIs only "paint pixels + route input". Every decision that could cause
behavioral differences lives in the core, with a single implementation:

| Module | Responsibility |
|---|---|
| `model` | `PayloadFormat`, `Skin`, `ConnStatus`, `LogRecord`, Client ID generation, numeric clamping (Keep Alive 1..65535, Timeout 1..300, QoS 0..2, timed 1..3600) |
| `config` | `ConnectionConfig` / `PublishPreset` (list order, manually reorderable) / `AutoReplyRule` / `AppConfig` (including `custom_palette`, `font_scale`) and serde defaults; reading and writing `mqttc.json`; legacy single-connection migration; `active_id` repair; preset/rule backfill; atomic writes (temporary file + rename, direct-write fallback on Windows) |
| `payload` | text / json / base64 / hex codecs, tolerant hex parsing (spaces, commas, colons, hyphens), JSON pretty-printing, log rendering |
| `filter` | MQTT `+` / `#` wildcard matching; keyword / type / subscribed-topic filtering |
| `i18n` | process-level `AtomicU8` language + `t(zh, en)`; callable from worker threads |
| `theme` | 7 skins (System resolves to dark/light, Custom resolves from `custom_palette`); a `Palette` of 27 named colors, `#rrggbb` overrides that fall back per slot; each UI converts them to its own color types |
| `platform` | system dark/light detection: Windows reads the registry (cached for 1s); macOS `defaults` / Linux `gsettings` (result cached for 30s to avoid frequent process spawning) |
| `mqtt` | `MqttHandle`: rumqttc 0.25.1, dual 3.1.1 and 5.0 clients; generation invalidates old threads; channel capacity 32; Keep Alive / Clean / Timeout / Will; rustls custom CA, client certificate, skip verification |
| `logs` | one `VecDeque<LogRecord>` per connection, default cap `MAX_LOGS = 200` (adjustable 50–2000 in settings); stores only raw bytes as `Arc<[u8]>` and re-renders when the display format changes |
| `form` | `FormState`: string/boolean drafts for every field on the edit and session pages, plus preset rows and rule rows; `from_config` / `apply_to` / row add and remove; preset pinning and display ordering |
| `runtime` | `AppRuntime`: config, forms, per-connection `ConnRuntime`, page state, preferences; action entry points and the auto-reply / timed-publish state machines |
| `export` | log export to `logs/` next to the exe (CSV / text, timestamped filenames) |
| `cli` | parsing for `--bench --bench-scenario s1..s4 --bench-quit-after N --config <path>` |

### Form and config consistency

There is only one path from config to form and back:

```text
mqttc.json ──load──▶ AppConfig ──from_config──▶ FormState ──UI binding──▶ user edits
                                                        │
                                          apply_to (clamp/backfill)
                                                        ▼
                                                   ConnectionConfig ──save──▶ mqttc.json
```

- Before switching connections or running actions (connect, subscribe, publish,
  disconnect), `apply_form()` is called so the three UIs never synchronize fields on their
  own.
- Plain text edits use `mark_dirty()` + a 400ms debounced `maybe_persist()`; actions save
  immediately.
- An empty Client ID is generated at connect time as `mqttc-<millis hex>`; an empty name
  is backfilled with `host:port`.

## 3. Threading and event model

```text
UI thread                                  worker threads (1 per connection + timed-publish thread)
┌─────────────────────────┐              ┌──────────────────────────────┐
│ AppRuntime              │              │ rumqttc Connection::iter()   │
│  ├─ EventSink.drain ────┼── queue ◀────┤  push(RuntimeEvent::Mqtt)   │
│  ├─ poll()              │              │  wake() ────────────────────┼─▶ request_repaint()
│  └─ auto-reply / timed send            └──────────────────────────────┘
└─────────────────────────┘
```

- The UI thread exclusively owns `AppRuntime` (egui/iced/gpui wrap it in
  `Rc<RefCell<AppRuntime>>`, `App` state, or `Entity<Root>`).
- Only three things cross threads: `MqttHandle`, `Arc<EventSink>`, and `Arc<AtomicU64>`
  (the timer generation).
- The MQTT event loop is a blocking std thread with a 256KB stack; the `generation`
  counter makes old threads exit on their own after reconnect or disconnect.
- Workers never read configuration: timed publish only pushes `TimedTick`, and the UI
  thread sends with the current config inside `poll()`.
- `poll() -> PollSummary { changed, status_changed, page_request }`; the UI refreshes only
  the affected area when `changed` is set, and `page_request` asks to switch to the
  session page when the handshake completes.

### Auto-reply and timed publish

- When `poll()` handles a `Recv`, it first records RECV (raw bytes), then matches enabled
  rules in order (topic wildcard + keyword + payload); the first hit is sent and recorded
  as `last_auto_reply`. Self-echo is identified by raw bytes.
- Timed publish: the first tick sends immediately, then follows the interval; disconnecting
  stops it; switching connections does not interrupt it (same as early Slint versions).

## 4. UI integration differences

| | egui | Iced | GPUI |
|---|---|---|---|
| Rendering | glow / OpenGL | tiny-skia software raster | GPU (Windows Direct3D 11 / macOS Metal / Linux Vulkan) |
| Wakeup | `ctx.request_repaint()` (repaint on event arrival) | 100ms tick (busy) / 1s (idle with System skin) / no tick otherwise | adaptive ticker: 100ms when busy / 1s when idle on system skin / no tick otherwise |
| Log list | `ScrollArea` renders all rows + `stick_to_bottom` | windowed layout of the newest 80 rows + freeze on scroll-up / show older / jump to latest | `uniform_list` fixed-row-height virtualization |
| CJK font | fontique system fallback (no bundled CJK) | cosmic-text system fallback, default family per platform (YaHei / PingFang) | system font fallback |
| Dropdown / selection | `ComboBox` | `pick_list` | custom click-to-cycle selector (no popup) |
| Input | `TextEdit` (passwords via `password(true)`) | `text_input` / `text_editor` | IME `TextInput` ported from the official example (supports password masking) |
| Page structure | global sidebar pages + subscription card in the session | 4 sidebar pages + session tabs (logs / subscriptions / auto-reply / preset replies) | global sidebar pages |
| Title bar | native title bar | frameless + custom-drawn title bar (with always-on-top pushpin) | custom-drawn `TitlebarOptions` |

GPUI trade-offs (read-only rule/preset edit pages, etc.) are documented item by item in
[PARITY.en.md](PARITY.en.md).

## 5. Size and memory strategy

- Enable only the necessary features: egui disables wgpu/accesskit/persistence; Iced
  disables wgpu/image/svg/web and enables tiny-skia; the network stack is only rumqttc +
  rustls.
- Do not bundle CJK fonts (often 10–20MB); everything uses system font fallback.
- Release profile: `opt-level=3`, thin LTO, `codegen-units=1`, `strip="symbols"`,
  `panic="abort"`. The delivery comparison uses a separate `release-size` profile
  (`opt-level="s"`, fat LTO). GPUI uses `release-unwind` because of its internal
  `catch_unwind` (unwind only; the rest is identical).
- No `+crt-static` (it is larger and conflicts with the TLS/plugin ecosystem); the docs
  note the VC runtime prerequisite.
- Runtime: 200-entry log ring buffer by default (adjustable to 2000 in settings, with a
  separate 16MB byte budget), bytes only, virtualized / windowed visible rows, debounced
  config saves, and no fixed-frame-rate animation.

## 6. Tests

```text
cargo test -p mqttc-core          # 70 tests: codecs, wildcards, filtering, config migration/round-trip, skins/custom palette, font clamping, auto-reply, form clamping, 200 cap
cargo test -p mqttc-core --test broker_e2e        # 3 tests: dual-protocol connect, subscribe, publish, disconnect
cargo test -p mqttc-core --test broker_roundtrip  # 5 tests + 1 #[ignore] TLS: connect/subscribe/publish/receive/auto-reply/timed/reconnect/200 cap
cargo test -p mqttc-iced          # 15 tests: page construction, preset collapsing, large font, delete confirmation, etc.
```

Local integration tests use `start_broker` from `tools/test-broker` (embedded rumqttd
0.20) and never depend on public services; TLS tests require certificates and are
`#[ignore]` by default. Most recent run: 70 + 3 + 5 + 15 all passed.

## 7. Portability

- `config::config_path()` = directory containing the executable / `mqttc.json`;
  `log_dir()` = the same directory / `logs/`.
- If the executable directory is not writable: the error goes to stderr and the UI stays
  usable (no crash).
- Release packages contain only the executable + a sample `mqttc.json` + a brief README;
  no registry or user-profile writes. Windows / Linux can be unzipped into any directory;
  on macOS the config lives in `mqttc.app/Contents/MacOS/`, which still means "next to the
  executable".
- Verified: unzip into a directory with spaces and Chinese characters
  (`portable test 中文`), run directly, and read/write the config in the same directory.
- Linux artifacts are built on Ubuntu 22.04 (glibc 2.35+); macOS artifacts are
  arm64 + x86_64 universal; both are unsigned.
