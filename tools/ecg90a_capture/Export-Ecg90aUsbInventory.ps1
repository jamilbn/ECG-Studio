[CmdletBinding()]
param(
    [string]$OutFile = (Join-Path (Get-Location) 'captures\ecg90a\usb_inventory.csv'),
    [switch]$OnlyLikelyContec
)

$ErrorActionPreference = 'Stop'

function Get-UsbIdPart {
    param(
        [string]$DeviceId,
        [string]$Name
    )

    if ($DeviceId -match "${Name}_([0-9A-Fa-f]{4})") {
        return $Matches[1].ToUpperInvariant()
    }
    return ''
}

$devices = Get-CimInstance Win32_PnPEntity | Where-Object {
    ($_.PNPDeviceID -like 'USB*') -or
    ($_.PNPClass -in @('Ports', 'HIDClass', 'USB'))
}

if ($OnlyLikelyContec) {
    $devices = $devices | Where-Object {
        "$($_.Name) $($_.Manufacturer) $($_.Description) $($_.PNPDeviceID)" -match
            '(?i)contec|ecg|cp210|silicon|uart|serial|hid|workstation'
    }
}

$rows = $devices | Sort-Object PNPClass, Name | ForEach-Object {
    [PSCustomObject]@{
        Name = $_.Name
        Manufacturer = $_.Manufacturer
        Description = $_.Description
        PNPClass = $_.PNPClass
        Service = $_.Service
        Status = $_.Status
        VID = Get-UsbIdPart -DeviceId $_.PNPDeviceID -Name 'VID'
        PID = Get-UsbIdPart -DeviceId $_.PNPDeviceID -Name 'PID'
        PNPDeviceID = $_.PNPDeviceID
    }
}

$outDir = Split-Path -Parent $OutFile
if (-not [string]::IsNullOrWhiteSpace($outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$rows | Export-Csv -LiteralPath $OutFile -NoTypeInformation -Encoding UTF8
Write-Host "Exported USB inventory:"
Write-Host $OutFile
