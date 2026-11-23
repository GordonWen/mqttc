# Feature parity

[简体中文](PARITY.md) | English

The baseline is the author's early Slint build, mqttc v0.1.1 (retained locally and not
distributed with the repository). The egui and Iced builds target **full parity**, while
the GPUI build targets **core-feature parity**; differences are listed item by item below.
All UIs share `mqttc-core`, so the configuration format, protocol behavior, log limits,
auto-reply and timed-publish logic are identical.

Official Releases provide only the Iced build (packaged as `mqttc`); egui / gpui must be
built from source.

## 1. Overview

| Feature | Early Slint | egui | Iced | GPUI |
|---|---|---|---|---|
| Add / remove / switch multiple connections | ✓ | ✓ | ✓ | ✓ |
| Edit pages (connection / advanced / TLS / will / actions) | ✓ | ✓ | ✓ | ✓ |
| Connect / disconnect, status dot and text | ✓ | ✓ | ✓ | ✓ |
| MQTT 3.1.1 and 5.0 | ✓ | ✓ | ✓ | ✓ |
| QoS 0/1/2, Clean Session, Keep Alive, timeout | ✓ | ✓ | ✓ | ✓ |
| TLS: custom CA, client certificate / private key, skip verification | ✓ | ✓ | ✓ | ✓ |
| Last Will (Topic / Payload / QoS / Retain) | ✓ | ✓ | ✓ | ✓ |
| Subscription list (wildcards, QoS, click to filter) | ✓ | ✓ | ✓ | ✓ |
| Publish: Text / JSON / Base64 / Hex | ✓ | ✓ | ✓ | ✓ |
| JSON pretty-printing | ✓ | ✓ | ✓ | ✓ |
| Timed publish 1–3600s | ✓ | ✓ | ✓ | ✓ |
| Preset send and edit | ✓ | ✓ | ✓ | send ✓ / edit ✗ (read-only) |
| Auto-reply rules (topic, keyword, reply, QoS, Retain, format) | ✓ | ✓ | ✓ | enable/delete/add/QoS/format/Retain ✓; text read-only |
| Log search / type filter / format switch / auto-scroll / save / clear | ✓ | ✓ | ✓ | ✓ |
| 200 log lines retained per connection by default (adjustable up to 2000 in settings) | ✓ | ✓ | ✓ | ✓ |
| Chinese/English switch | ✓ | ✓ | ✓ | ✓ |
| 7 skins (including follow system / custom palette) | ✓ | ✓ | ✓ | ✓ |
| Config persistence (`mqttc.json`, backward compatible) | ✓ | ✓ | ✓ | ✓ |
| Window icon | ✓ | ✓ | PNG + exe ICO | ✗ (not supported by GPUI) |
| Benchmark arguments `--bench ...` | ✗ | ✓ | ✓ | ✓ |

## 2. Differences between egui / Iced and the early Slint build

Both builds are feature-complete; the differences are implementation-level:

- **Log rendering**: the early Slint build used a Slint list; egui renders all rows in a
  `ScrollArea` with `stick_to_bottom`, while Iced lays out only the newest 80 rows and
  freezes the current page when scrolled up, with "Show older" to page backwards and
  "Jump to latest" when new messages arrive. Bubble width follows content (85% cap).
  Scrollbar styles differ (egui has thin scrollbars; Iced uses `Scrollbar::new()`).
- **Fonts**: no CJK font is bundled; both use system font fallback (egui: fontique; Iced:
  cosmic-text, with the default family chosen per platform as Microsoft YaHei UI /
  PingFang SC). The early Slint build bundled glyph files, which is why it was larger.
- **Title bar**: egui uses a native title bar; Iced uses a frameless window with a
  custom-drawn title bar (brand, minimize / maximize / close, always-on-top pushpin,
  double-click to maximize, four-side resize grips), and the theme also applies to the
  title area. On Windows, maximize uses the monitor work area for high DPI.
- **UI personalization**: Iced provides a 27-slot custom palette editor and 4 global text
  sizes; egui / GPUI read and apply the same `custom_palette`, but have no editor entry
  point and keep the text scale at 1.0.
- **Dropdowns**: egui uses `ComboBox`; Iced uses `pick_list`. Interaction matches the
  reference popup menu, but popup animation/shadow is determined by the framework.
- **System skin detection**: Windows reads the registry once per second by default;
  macOS / Linux call `defaults` / `gsettings` and cache the result for 30 seconds. The
  early Slint build used Slint's built-in detection.
- **Session-page additions**: Iced's session header shows received / sent counts for the
  current connection with a reset button, and names the preset tab "Publish presets";
  egui / GPUI show no traffic counters. All three share the same log-limit setting.
- **Multi-line payloads**: both egui and Iced provide multi-line editors; Hex / Base64
  tolerance rules match the early version.

## 3. GPUI differences (intentional simplification)

GPUI 0.2.2 only provides basic elements such as div / text / list and has no widget
library. To control scope, the following trade-offs were made:

| Item | GPUI behavior | Reason / impact |
|---|---|---|
| Publish payload | single-line input (`TextInput`), not a multi-line editor | The session page can still edit and send arbitrary text; editing large JSON is inconvenient |
| Preset / rule text | read-only display; add / delete / enable / disable / cycle QoS, format, Retain, and send are available | Text editing would need an IME input for every field; read-only avoids a half-finished UI. Edit the same `mqttc.json` in egui / Iced / early Slint and return to GPUI to apply it |
| Dropdowns | click-to-cycle chips (`QoS 0 ▾` click → `QoS 1`) | No popup widget; all option sets have 2–6 values, so cycling is acceptable |
| Log list | `uniform_list` with fixed row height and single-line ellipsis, colored by type | No bubble layout; information (time / type / topic / payload) matches the reference |
| Window icon | none | GPUI 0.2.2 exposes no API for setting a window icon |
| Chinese input | `TextInput` ported from the official example, with IME marked text and password masking | On-device testing is needed to confirm IME behavior on each platform |
| Multi-line / rich text | none | Same as above |
| Auto repaint | 100ms tick while connected / timed publish is on; 1s tick when idle on system skin; no tick otherwise | Lower idle CPU; egui wakes on events, Iced ticks at 100ms while active |

**Shared parts**: connection, TLS, subscriptions, publish encoding, auto-reply, timed
publish, 200 log lines, export, filtering, skin palettes, language switching, and config
I/O all come from `mqttc-core` and behave identically to egui / Iced.

## 4. Config compatibility

All four programs read and write the same `mqttc.json` (field names, defaults and
migration logic are provided by `mqttc-core::config`):

- Connections, presets and rules saved by the early Slint build load directly in the new
  programs, and vice versa.
- Legacy single-connection configs migrate automatically; an invalid `active_id` falls
  back to the first connection; missing preset/rule fields are filled with defaults.
- GPUI preserves read-only rule/preset text as-is when saving; it is never cleared.

## 5. Benchmark mode (new capability)

The early Slint build had no command-line arguments. The three new programs support:

```text
--bench --bench-scenario s1|s2|s3|s4 --bench-quit-after <secs> --config <path>
```

It is used for unattended memory / CPU sampling and prints `FIRST_FRAME_MS`,
`BENCH_SUMMARY`, and `BENCH_QUIT`. This mode only affects startup and exit; it does not
change the normal interaction path.
