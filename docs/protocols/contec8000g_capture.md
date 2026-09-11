# CONTEC8000G capture instructions

Scope: capture real traffic from a CONTEC8000G/ECGXXXG device so the Rust ECG
Studio app can later implement read-only access. This document is protocol and
language neutral: raw captures stay original, while normalized CSV files provide
evidence for any implementation language.

Base reference: `contec8000g_protocol_re_3_4_8.md`.

## Goals

- Confirm the real transport used by the connected 8000G: USB HID, CP210x/COM,
  WLS/network, or another USB interface.
- Preserve original capture files from Wireshark/USBPcap or serial/network
  capture tools.
- Capture the official ECG Workstation V3.4.8 detection flow.
- Capture live waveform start, a short stable waveform interval, normal stop,
  and normal close.
- Capture saved-case listing/download only if it is safe and uses test data.
- Avoid all delete commands during reverse engineering.

## Safety rules

- Prefer simulator/test signal. Do not capture identifiable patient data.
- Do not send manual commands to the device during capture.
- Use the official software only as the traffic generator.
- Keep raw captures under `captures/contec8000g/sessions/<session>/raw/`.
- Do not commit raw PCAP/PCAPNG/log files; session data is ignored by Git.
- Treat these commands as destructive and forbidden until a guarded UI exists:
  `8B`, `8C`, `8D`.

## Create a capture session

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\contec8000g_capture\New-Contec8000GCaptureSession.ps1 `
  -Transport unknown `
  -OfficialSoftware "ECG Workstation V3.4.8"
```

Then export the host inventory:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\contec8000g_capture\Export-Contec8000GHostInventory.ps1 `
  -SessionPath .\captures\contec8000g\sessions\<session>
```

The script creates:

```text
captures/contec8000g/sessions/YYYYMMDD-HHMMSS-contec8000g-TRANSPORT/
  manifest.md
  usb_inventory.csv
  serial_ports.csv
  net_adapters.csv
  raw/
  exports/
  analysis/
    frames.csv
    findings.md
  notes/
    observations.md
```

## Transport decision

Use this order:

1. Check `serial_ports.csv` for a new `COMx`, especially Silicon Labs/CP210x.
2. Check `usb_inventory.csv` for `CONTEC`, `ECG`, `workstation`, `8000`,
   `CP210`, `Silicon Labs`, `HIDClass`, or vendor-defined USB devices.
3. Check `net_adapters.csv` only if the software is configured for WLS/network.
4. If uncertain, capture USBPcap for the candidate root hub while opening the
   official software and inspect descriptors first.

Known static-analysis hints:

| Transport | What to look for |
| --- | --- |
| Serial/COM | CP210x/USB Serial device. The official path tries `230400` and `460800` bps. |
| HID | Output reports where report byte `0` may be report id and byte `1` may be command. |
| WLS/network | Socket send/recv traffic from ECG Workstation. |

## First confirmed 8000G capture

Session:

```text
captures/contec8000g/sessions/20260509-132959-contec8000g-unknown
```

Preserved raw file:

```text
raw/01_detect_open_workstation_usbpcap3_cp210x.pcapng
```

Confirmed from that capture:

| Item | Value |
| --- | --- |
| USB identity | `10C4:EA60` |
| Bridge | Silicon Labs CP210x USB-to-UART |
| Windows port | `COM6` |
| USBPcap target | `USBPcap3`, bus `3`, address `8` |
| CP210x serial speed | `230400` bps from request `0x1E`, payload `00 84 03 00` |
| Data OUT endpoint | `0x02` bulk |
| Data IN endpoint | `0x82` bulk |
| Observed host payloads | `90 00`, `85 01`, `90 05`, later `90 00` |
| First inbound stream | frame 74, starts `EE 10 A0 49...` |
| Stream record model | `EE 10` boundary, `A0` 15-byte packed records, `B0` 6-byte periodic status |

The raw file remains the base evidence. Use the derived files in `exports/` and
`analysis/frames.csv` for implementation notes, but never overwrite the raw
PCAPNG.

## USBPcap/Wireshark capture

Use this when the device appears as HID or unknown USB.

1. Open Wireshark as Administrator.
2. Select the USBPcap interface containing the 8000G candidate.
3. In the USBPcap options, select only the candidate device when possible.
4. Enable descriptor injection for already connected devices.
5. Start capture before opening ECG Workstation V3.4.8.
6. Preserve the original `.pcapng` in `raw/`.

Recommended raw files:

| File | Scenario | User action |
| --- | --- | --- |
| `01_usb_idle_before_workstation.pcapng` | Device connected, official software closed. | Capture 10 s idle. |
| `02_usb_detect_open_workstation.pcapng` | Detection/init. | Start capture, open ECG Workstation, wait for detection. |
| `03_usb_start_live_10s.pcapng` | Live waveform start. | Click/start acquisition and keep test signal stable for 10 s. |
| `04_usb_stop_live.pcapng` | Normal stop. | Stop acquisition normally. |
| `05_usb_close_workstation.pcapng` | Normal close. | Close ECG Workstation. |

If using Wireshark manually, save with `File > Save As` directly into `raw/`.
Do not filter destructively or export over the original file.

After saving the raw PCAPNG, export derived tables with:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\contec8000g_capture\Export-Contec8000GUsbPcap.ps1 `
  -SessionPath .\captures\contec8000g\sessions\<session> `
  -RawFile raw\<capture>.pcapng `
  -BusId <usb-bus-id> `
  -DeviceAddress <usb-device-address>
```

This writes `exports/serial_bulk_frames.tsv`,
`exports/host_to_device_commands.tsv`, `exports/cp210x_control_setup.tsv`,
`exports/first_40_inbound_frames.tsv`, and `analysis/usbpcap_summary.md`.

## Serial/COM capture

Use this when the device appears as a COM port.

Expected serial settings from static analysis:

| Setting | Candidate values |
| --- | --- |
| Baud | `230400`, `460800` |
| Data bits | likely `8` |
| Parity | likely none |
| Stop bits | likely `1` |
| Flow control | unknown; record whatever the official software configures |

Preferred method:

1. Use a serial monitor that can observe the official software without taking
   exclusive ownership of the port.
2. Start capture before opening ECG Workstation.
3. Export raw bytes with timestamps if the tool supports it.
4. Save the original monitor file in `raw/`.
5. Export a derived CSV into `exports/`.

Recommended raw files:

| File | Scenario |
| --- | --- |
| `01_serial_detect_open_workstation.*` | Open official software and detect device. |
| `02_serial_start_live_10s.*` | Start live waveform and capture at least 10 s. |
| `03_serial_stop_live.*` | Stop acquisition normally. |
| `04_serial_case_list_download.*` | Optional saved-case listing/download with test data only. |

## Network/WLS capture

Use only if the official software is configured for WLS/network mode.

1. Record the host IP, device IP, and port if visible.
2. Capture with Wireshark on the active network adapter.
3. Start before opening ECG Workstation.
4. Save the original `.pcapng` in `raw/`.

Recommended display filter for analysis, after preserving the raw file:

```text
ip.addr == <device-ip> || tcp.port == <suspected-port> || udp.port == <suspected-port>
```

## Interaction timeline

Fill `notes/observations.md` during each run:

| Local time | Action |
| --- | --- |
| HH:MM:SS | Device plugged in. |
| HH:MM:SS | ECG Workstation opened. |
| HH:MM:SS | Device detected / not detected. |
| HH:MM:SS | Live acquisition started. |
| HH:MM:SS | First waveform visible. |
| HH:MM:SS | Live acquisition stopped. |
| HH:MM:SS | Workstation closed. |

## Normalized frames

Derived `analysis/frames.csv` uses:

```text
timestamp_utc,direction,transport,interface,endpoint,transfer_type,command,length,payload_hex,decoded_hint,notes
```

Conventions:

- `payload_hex` uses uppercase bytes separated by one space.
- `direction` is `host_to_device`, `device_to_host`, or `unknown`.
- `transport` is `hid`, `serial`, `usb_control`, `usb_interrupt`,
  `usb_bulk`, `tcp`, `udp`, or `unknown`.
- `command` is the best-known command byte/sequence, marked as hypothesis if
  not confirmed.

## Known command markers to search

These are from static analysis of ECG Workstation V3.4.8. They are not complete
live waveform documentation yet.

| Marker | Meaning observed in 8000G analysis |
| --- | --- |
| `C2` | 10-byte preparation frame with checksum, format still incomplete. |
| `90 00` | Initialization before detection. |
| `80` | Detection command, expects `F0` response. |
| `F0` | Detection response; byte 1 identifies model. |
| `83` | Extended info/list/ACK path. |
| `F3` | Extended ACK/status after `83`. |
| `86` | Case count. |
| `87` | Saved-case download by index. |
| `8A` | Select case. |
| `8F` | End/abort transfer. |
| `91` | Close/end serial session path. |
| `93` | Counter/size query. |
| `9D` | Ping/keepalive. |
| `BA 02` | Pre-close path, expects `FA 02` in one observed path. |

Forbidden markers during app implementation until explicitly guarded:

```text
8B
8C
8D
```

## Acceptance for a useful 8000G capture

A useful first capture set has:

- host inventory files;
- raw capture files preserved;
- notes with exact click/action times;
- confirmed transport and device identity;
- detection sequence evidence;
- live start/stop sequence evidence;
- at least 10 s of waveform traffic, if live mode works;
- normalized `analysis/frames.csv` rows for the key commands.

Only after this should Rust code be added. The first Rust implementation should
be read-only and should block delete commands by design.
