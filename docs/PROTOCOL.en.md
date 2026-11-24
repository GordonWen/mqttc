# Protocol Behaviour

[简体中文](PROTOCOL.md) | English

This page documents the MQTT-specific behaviour mqttc implements on top of rumqttc: which
connection-page fields appear for which protocol version, CONNACK capability pre-checks,
publish acknowledgement (PUBACK / PUBREC / PUBCOMP) states, manual acks, the TLS key log,
and the fault-injection tests that verify all of it.

Packet codecs, connect/reconnect and the QoS 2 PUBREL / PUBCOMP sequence come from
rumqttc 0.25.1. mqttc only adds what a debug client needs — display, validation and
logging — and does not implement broker-side storage, routing or session recovery.

## 1. Connection fields per protocol version

The editor only shows fields that mean something for the selected version:

| Field | 3.1.1 | 5.0 | Notes |
|---|---|---|---|
| Clean Session / Clean Start | ✓ (Clean Session) | ✓ (Clean Start) | Same semantics, label follows the version |
| Session Expiry | — | ✓ | CONNECT property; blank / 0 omits it |
| Receive Maximum | — | ✓ | CONNECT property; 0 omits it |
| Maximum Packet Size | — | ✓ | CONNECT property; 0 omits it |
| Topic Alias Maximum | — | ✓ | CONNECT property; 0 omits it |
| Manual ACK | ✓ | ✓ | Client behaviour, valid for both versions |
| TLS key log | ✓ | ✓ | Transport switch, independent of the version |

The four CONNECT properties are written only for **5.0 with a non-zero value**. Switching
back to 3.1.1 keeps the saved values in `mqttc.json` but does not send them. `form` clamps
on save: Receive Maximum 1–65535, Maximum Packet Size 1–268435455, and 0 always means
"omit". Older config files without these keys load as 0 / false via `serde(default)`.

## 2. CONNACK capability summary and pre-checks

After a successful handshake the runtime writes one `SYS` row:

```text
CONNACK: new session · max QoS 2 · max packet 1048576 B · receive max 20
```

Reason strings and user properties follow as extra `SYS` rows.

`BrokerCaps` (`core/src/mqtt.rs`) is filled from the 5.0 CONNACK properties. 3.1.1 has no
capability flags, so it uses permissive defaults (max QoS 2, retain / wildcard / shared
supported) and never triggers the extra checks. Fields: `session_present`, `max_qos`,
`retain_available`, `max_packet_size`, `receive_maximum`, `topic_alias_max`,
`server_keep_alive`, `wildcard_available`, `shared_available`, `session_expiry`,
`assigned_client_id`, `extra`.

Publish and subscribe run a local pre-check and cancel the operation with an `ERR` row
instead of sending a packet the broker is known to reject:

- **Publish** (`publish_check`): retain set but retain unavailable; QoS above `max_qos`;
  estimated packet size above `max_packet_size`. The estimate is
  `2 + topic bytes + payload bytes + property bytes + (QoS>0 ? 2 : 0) + 5`,
  the last five bytes covering a worst-case fixed header.
- **Subscribe** (`subscribe_check`): `+` / `#` without wildcard support; `$share/` without
  shared-subscription support; a 5.0 shared subscription with No Local set (the spec
  requires 0). `flip_sub_option` re-subscribes while connected, so it uses the same check.

The checks rely on what the broker **declares**. If the broker breaks its own limits,
rumqttc's error is the final word.

## 3. Publish acknowledgement states

Every QoS 1/2 `SEND` row carries an `AckInfo` that is updated in place — no extra rows:

| State | Meaning |
|---|---|
| `Queued` | Handed to rumqttc but not written yet (QoS 0 never enters this state) |
| `Sent` | QoS 0 written; QoS 1/2 usually move on to `AwaitingAck` quickly |
| `AwaitingAck` | QoS 1 waiting for PUBACK, QoS 2 waiting for PUBREC |
| `AwaitingComp` | QoS 2 got PUBREC; rumqttc sent PUBREL automatically |
| `Acked` | PUBACK / PUBCOMP succeeded; records the elapsed milliseconds |
| `Rejected` | Broker returned a failure reason (5.0 reason code or link error) |
| `Timeout` | No ack within 15 s, or the link dropped first; mqttc never retransmits |

Implementation notes:

- SEND rows are matched to packet ids from rumqttc's `Outgoing::Publish(pkid)` (emitted
  when the packet is actually written) through a FIFO, so concurrent publishes cannot
  attach an ack to the wrong row.
- For 5.0 only `Success` and `NoMatchingSubscribers` count as success on PUBACK / PUBREC;
  every other reason code shows as `Rejected` with the reason. 3.1.1 has no failure codes.
- QoS 2 PUBREL / PUBCOMP is driven by rumqttc; mqttc only observes the sequence.
- Timeouts are checked centrally by `ack_tick` inside `poll()`. Before a disconnect or
  reconnect, `mark_acks_lost` turns every unacknowledged row into `Timeout`.
- A timeout changes the display only; whether a QoS 1/2 packet is retransmitted is left to
  rumqttc's session state.

## 4. Manual ACK

With Manual ACK enabled (connection page or `manual_acks` in `mqttc.json`), rumqttc's
`manual_acks` holds only **inbound** QoS 1/2 PUBLISH acknowledgements:

1. The message is still logged as a `RECV` row with state `PendingAck`; the log bubble and
   the detail view show an Ack button.
2. Clicking it calls `AppRuntime::manual_ack(seq)`: PUBACK for QoS 1, PUBREC for QoS 2.
3. The state becomes `AckSent`. For QoS 2, rumqttc then sends PUBREL / PUBCOMP itself and
   the row becomes `Acked` when `Outgoing::PubComp` arrives.
4. Outgoing SEND acknowledgement tracking is unaffected.

Manual ACK is meant for inspecting how a broker behaves without an ack. While a row is
held the broker may redeliver (DUP=1); those copies are logged like any other message.

## 5. TLS key log

With TLS key log enabled, rustls' `KeyLog` appends the secrets of every connection to
`logs/tls-keys.log` under the config directory, using the standard `SSLKEYLOGFILE` labels
(`CLIENT_RANDOM`, ...), so Wireshark can decrypt the capture.

The file is equivalent to plaintext session keys: enable it only for local debugging and
never attach it to issues, logs or deliverables. The switch applies to new connections;
turning it off stops writing but does not delete an existing file.

## 6. Fault-injection tests

`tools/test-broker/src/fault.rs` is a deliberately non-compliant minimal MQTT 3.1.1 server
that lives next to rumqttd and is used only by `core/tests/fault_injection.rs`:

| `Fault` | Behaviour | Covers |
|---|---|---|
| `Happy` | Immediate PUBACK; QoS 2 walks PUBREC → PUBREL → PUBCOMP | Complete QoS 2 sequence |
| `DelayAck(d)` | PUBACK after `d` | `AwaitingAck` → `Acked`, elapsed time |
| `DropAck` | Never acks, keeps TCP open | Stuck state; `Timeout` after link loss |
| `Duplicate` | Two deliveries per PUBLISH (second with DUP=1) | Duplicates + two manual acks |
| `OutOfOrder` | Buffers two PUBACKs, sends newest first | Packet-id matching, not ordering |

Run them with:

```text
cargo test -p mqttc-core --test fault_injection
```

The real 15 s timeout is not waited out in integration tests; a unit test in
`core/src/runtime.rs` backdates `ack_at` instead. The integration tests cover the genuine
link-loss path (`disconnect` → `Timeout`).
