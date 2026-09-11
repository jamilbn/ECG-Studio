[CmdletBinding()]
param(
    [string]$SessionPath = (Join-Path (Get-Location) 'captures\contec8000g'),
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

function Test-LikelyContecText {
    param([string]$Text)

    return $Text -match '(?i)contec|ecg|8000|8100|workstation|cp210|silicon|uart|serial|hid|wls'
}

New-Item -ItemType Directory -Path $SessionPath -Force | Out-Null

$devices = Get-CimInstance Win32_PnPEntity | Where-Object {
    ($_.PNPDeviceID -like 'USB*') -or
    ($_.PNPClass -in @('Ports', 'HIDClass', 'USB', 'USBDevice', 'Net'))
}

if ($OnlyLikelyContec) {
    $devices = $devices | Where-Object {
        Test-LikelyContecText "$($_.Name) $($_.Manufacturer) $($_.Description) $($_.PNPDeviceID)"
    }
}

$usbRows = $devices | Sort-Object PNPClass, Name | ForEach-Object {
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

$serialRows = Get-CimInstance Win32_SerialPort | Sort-Object DeviceID | ForEach-Object {
    [PSCustomObject]@{
        DeviceID = $_.DeviceID
        Name = $_.Name
        Description = $_.Description
        Manufacturer = $_.Manufacturer
        ProviderType = $_.ProviderType
        MaxBaudRate = $_.MaxBaudRate
        PNPDeviceID = $_.PNPDeviceID
    }
}

if ($OnlyLikelyContec) {
    $serialRows = $serialRows | Where-Object {
        Test-LikelyContecText "$($_.Name) $($_.Description) $($_.Manufacturer) $($_.PNPDeviceID)"
    }
}

$netRows = Get-CimInstance Win32_NetworkAdapter | Where-Object { $_.NetEnabled -eq $true } |
    Sort-Object Name |
    ForEach-Object {
        [PSCustomObject]@{
            Name = $_.Name
            Manufacturer = $_.Manufacturer
            AdapterType = $_.AdapterType
            MACAddress = $_.MACAddress
            NetConnectionID = $_.NetConnectionID
            Speed = $_.Speed
            PNPDeviceID = $_.PNPDeviceID
        }
    }

$usbRows | Export-Csv -LiteralPath (Join-Path $SessionPath 'usb_inventory.csv') -NoTypeInformation -Encoding UTF8
$serialRows | Export-Csv -LiteralPath (Join-Path $SessionPath 'serial_ports.csv') -NoTypeInformation -Encoding UTF8
$netRows | Export-Csv -LiteralPath (Join-Path $SessionPath 'net_adapters.csv') -NoTypeInformation -Encoding UTF8

Write-Host "Exported CONTEC8000G host inventory:"
Write-Host $SessionPath
