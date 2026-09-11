param(
    [Parameter(Mandatory = $true)]
    [string]$SessionPath,

    [Parameter(Mandatory = $true)]
    [string]$RawFile,

    [Parameter(Mandatory = $true)]
    [int]$BusId,

    [Parameter(Mandatory = $true)]
    [int]$DeviceAddress,

    [string]$TSharkPath = "C:\Program Files\Wireshark\tshark.exe",

    [int]$FirstInboundCount = 40
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (!(Test-Path -LiteralPath $SessionPath)) {
    throw "SessionPath not found: $SessionPath"
}

if (!(Test-Path -LiteralPath $TSharkPath)) {
    throw "tshark not found: $TSharkPath"
}

$session = (Resolve-Path -LiteralPath $SessionPath).Path
$rawPath = if ([System.IO.Path]::IsPathRooted($RawFile)) {
    $RawFile
} else {
    Join-Path $session $RawFile
}

if (!(Test-Path -LiteralPath $rawPath)) {
    throw "RawFile not found: $rawPath"
}

$exports = Join-Path $session "exports"
$analysis = Join-Path $session "analysis"
New-Item -ItemType Directory -Force -Path $exports | Out-Null
New-Item -ItemType Directory -Force -Path $analysis | Out-Null

$baseFilter = "usb.bus_id == $BusId && usb.device_address == $DeviceAddress"
$bulkFilter = "$baseFilter && usb.capdata"

$bulkOut = Join-Path $exports "serial_bulk_frames.tsv"
$bulkRows = @("frame.number`tframe.time_utc`tframe.time_relative`tusb.src`tusb.dst`tusb.endpoint_address`tusb.transfer_type`tusb.data_len`tusb.capdata")
$bulkRows += & $TSharkPath -r $rawPath -Y $bulkFilter -T fields `
    -e frame.number `
    -e frame.time_utc `
    -e frame.time_relative `
    -e usb.src `
    -e usb.dst `
    -e usb.endpoint_address `
    -e usb.transfer_type `
    -e usb.data_len `
    -e usb.capdata
$bulkRows | Set-Content -LiteralPath $bulkOut -Encoding ascii

$commandsOut = Join-Path $exports "host_to_device_commands.tsv"
$commandRows = @("frame.number`tframe.time_utc`tframe.time_relative`tusb.src`tusb.dst`tusb.endpoint_address`tusb.transfer_type`tusb.data_len`tusb.capdata")
$commandRows += & $TSharkPath -r $rawPath -Y "$bulkFilter && usb.src == ""host""" -T fields `
    -e frame.number `
    -e frame.time_utc `
    -e frame.time_relative `
    -e usb.src `
    -e usb.dst `
    -e usb.endpoint_address `
    -e usb.transfer_type `
    -e usb.data_len `
    -e usb.capdata
$commandRows | Set-Content -LiteralPath $commandsOut -Encoding ascii

$controlOut = Join-Path $exports "cp210x_control_setup.tsv"
$vendorControlFilter = "$baseFilter && usb.transfer_type == 2 && usb.src == ""host"" && (usb.bmRequestType == 0x41 || usb.bmRequestType == 0xc1 || usb.bmRequestType == 0xc0)"
$controlRows = @("frame.number`tframe.time_relative`tusb.src`tusb.dst`tendpoint`tbmRequestType`tbRequest`twValue`twIndex`twLength`tdata_fragment")
$controlRows += & $TSharkPath -r $rawPath -Y $vendorControlFilter -T fields `
    -e frame.number `
    -e frame.time_relative `
    -e usb.src `
    -e usb.dst `
    -e usb.endpoint_address `
    -e usb.bmRequestType `
    -e usb.setup.bRequest `
    -e usb.setup.wValue `
    -e usb.setup.wIndex `
    -e usb.setup.wLength `
    -e usb.data_fragment
$controlRows | Set-Content -LiteralPath $controlOut -Encoding ascii

$rows = Import-Csv -Delimiter "`t" -LiteralPath $bulkOut
$inbound = @($rows | Where-Object { $_.'usb.dst' -eq "host" -and [int]$_.'usb.data_len' -gt 0 })
$outbound = @($rows | Where-Object { $_.'usb.src' -eq "host" -and [int]$_.'usb.data_len' -gt 0 })

$firstInboundOut = Join-Path $exports "first_40_inbound_frames.tsv"
$inbound | Select-Object -First $FirstInboundCount |
    Export-Csv -Delimiter "`t" -NoTypeInformation -LiteralPath $firstInboundOut -Encoding ascii

$summaryOut = Join-Path $analysis "usbpcap_summary.md"
$inBytes = ($inbound | Measure-Object -Property 'usb.data_len' -Sum).Sum
$outBytes = ($outbound | Measure-Object -Property 'usb.data_len' -Sum).Sum
$firstIn = if ($inbound.Count -gt 0) { $inbound[0].'frame.time_relative' } else { "" }
$lastIn = if ($inbound.Count -gt 0) { $inbound[-1].'frame.time_relative' } else { "" }
$firstOut = if ($outbound.Count -gt 0) { $outbound[0].'frame.time_relative' } else { "" }
$lastOut = if ($outbound.Count -gt 0) { $outbound[-1].'frame.time_relative' } else { "" }

@(
    "# USBPcap derived summary",
    "",
    "| Field | Value |",
    "| --- | --- |",
    ("| Raw file | ``{0}`` |" -f $rawPath),
    ("| USB bus | ``{0}`` |" -f $BusId),
    ("| Device address | ``{0}`` |" -f $DeviceAddress),
    ("| Bulk rows | ``{0}`` |" -f $rows.Count),
    ("| Inbound non-empty rows | ``{0}`` |" -f $inbound.Count),
    ("| Inbound bytes | ``{0}`` |" -f $inBytes),
    ("| Outbound non-empty rows | ``{0}`` |" -f $outbound.Count),
    ("| Outbound bytes | ``{0}`` |" -f $outBytes),
    ("| First inbound relative time | ``{0}`` |" -f $firstIn),
    ("| Last inbound relative time | ``{0}`` |" -f $lastIn),
    ("| First outbound relative time | ``{0}`` |" -f $firstOut),
    ("| Last outbound relative time | ``{0}`` |" -f $lastOut),
    "",
    "Generated from the raw capture without modifying the original PCAPNG."
) | Set-Content -LiteralPath $summaryOut -Encoding ascii

Write-Host "Wrote $bulkOut"
Write-Host "Wrote $commandsOut"
Write-Host "Wrote $controlOut"
Write-Host "Wrote $firstInboundOut"
Write-Host "Wrote $summaryOut"
