[CmdletBinding()]
param(
    [string]$Root = (Join-Path (Get-Location) 'captures\contec8000g\sessions'),
    [string]$Device = 'CONTEC8000G',
    [ValidateSet('usbpcap', 'hid', 'serial', 'network', 'wls', 'unknown')]
    [string]$Transport = 'unknown',
    [string]$OfficialSoftware = 'ECG Workstation V3.4.8',
    [string]$Operator = '',
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

function ConvertTo-SafeName {
    param([string]$Text)

    $safe = $Text.ToLowerInvariant() -replace '[^a-z0-9]+', '-'
    $safe = $safe.Trim('-')
    if ([string]::IsNullOrWhiteSpace($safe)) {
        return 'device'
    }
    return $safe
}

$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$safeDevice = ConvertTo-SafeName $Device
$sessionName = "$timestamp-$safeDevice-$Transport"
$sessionPath = Join-Path $Root $sessionName

$paths = @(
    $sessionPath,
    (Join-Path $sessionPath 'raw'),
    (Join-Path $sessionPath 'exports'),
    (Join-Path $sessionPath 'analysis'),
    (Join-Path $sessionPath 'notes')
)

if ($DryRun) {
    Write-Host "Session path: $sessionPath"
    foreach ($path in $paths) {
        Write-Host "Would create: $path"
    }
    return
}

foreach ($path in $paths) {
    New-Item -ItemType Directory -Path $path -Force | Out-Null
}

$createdAt = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffffffZ')
$manifest = @"
# CONTEC8000G capture session

| Field | Value |
| --- | --- |
| Created UTC | $createdAt |
| Device | $Device |
| Serial number |  |
| Firmware |  |
| Operator | $Operator |
| Host OS | $([System.Environment]::OSVersion.VersionString) |
| Official software | $OfficialSoftware |
| Transport | $Transport |
| USB VID/PID |  |
| COM port |  |
| Network endpoint |  |
| Scenario |  |
| Signal source |  |
| Patient data present | no |

## Capture files

| File | Scenario | Notes |
| --- | --- | --- |

## Notes

"@

$observations = @"
# CONTEC8000G capture observations

## Timeline

| Local time | Action | Observation |
| --- | --- | --- |
| HH:MM:SS | Device plugged in |  |
| HH:MM:SS | ECG Workstation opened |  |
| HH:MM:SS | Device detected |  |
| HH:MM:SS | Live acquisition started |  |
| HH:MM:SS | First waveform visible |  |
| HH:MM:SS | Live acquisition stopped |  |
| HH:MM:SS | ECG Workstation closed |  |

## Environment

- Device label:
- Firmware:
- Signal source:
- Patient data present: no
- Official software: $OfficialSoftware
- Transport observed:
- USBPcap filter / COM port / network adapter:

## Free notes

"@

$findings = @"
# CONTEC8000G findings

## Transport

- Type:
- Device identity:
- USB VID/PID:
- COM port:
- Network endpoint:
- Driver:

## Confirmed sequences

| Sequence | Direction | Evidence | Status |
| --- | --- | --- | --- |

## Detection

- Preparation frame:
- `90 00` observed:
- `80` observed:
- `F0` response:
- Model code:
- Firmware date:
- `83`/`F3` extended ACK:

## Live waveform

- Start command:
- Stop command:
- Frame size:
- Frame interval:
- Sampling rate:
- Lead order:
- Scale:
- Checksum/CRC:

## Open questions

"@

$framesHeader = 'timestamp_utc,direction,transport,interface,endpoint,transfer_type,command,length,payload_hex,decoded_hint,notes'

Set-Content -LiteralPath (Join-Path $sessionPath 'manifest.md') -Value $manifest -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'usb_inventory.csv') -Value '' -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'serial_ports.csv') -Value '' -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'net_adapters.csv') -Value '' -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'analysis\frames.csv') -Value $framesHeader -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'analysis\findings.md') -Value $findings -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'notes\observations.md') -Value $observations -Encoding UTF8

Write-Host "Created CONTEC8000G capture session:"
Write-Host $sessionPath
