param(
    [Parameter(Mandatory = $true)]
    [string] $SourcePath,

    [Parameter(Mandatory = $true)]
    [string] $IconPath,

    [int] $Width = 256,

    [int] $Height = 256
)

$ErrorActionPreference = 'Stop'

function Read-UInt32BE([byte[]] $Bytes, [int] $Offset) {
    return ([uint32] $Bytes[$Offset] -shl 24) -bor
        ([uint32] $Bytes[$Offset + 1] -shl 16) -bor
        ([uint32] $Bytes[$Offset + 2] -shl 8) -bor
        [uint32] $Bytes[$Offset + 3]
}

function Add-UInt16LE([System.Collections.Generic.List[byte]] $Bytes, [int] $Value) {
    $Bytes.AddRange([BitConverter]::GetBytes([uint16] $Value))
}

function Add-UInt32LE([System.Collections.Generic.List[byte]] $Bytes, [uint32] $Value) {
    $Bytes.AddRange([BitConverter]::GetBytes($Value))
}

$resolvedSource = (Resolve-Path -LiteralPath $SourcePath).Path
$pngBytes = [System.IO.File]::ReadAllBytes($resolvedSource)

$pngSignature = [byte[]] (137, 80, 78, 71, 13, 10, 26, 10)
if ($pngBytes.Length -lt 33) {
    throw "PNG muito pequeno: $resolvedSource"
}

for ($index = 0; $index -lt $pngSignature.Length; $index++) {
    if ($pngBytes[$index] -ne $pngSignature[$index]) {
        throw "Arquivo PNG invalido: $resolvedSource"
    }
}

$chunkType = [System.Text.Encoding]::ASCII.GetString($pngBytes, 12, 4)
if ($chunkType -ne 'IHDR') {
    throw "PNG sem chunk IHDR inicial: $resolvedSource"
}

$actualWidth = Read-UInt32BE $pngBytes 16
$actualHeight = Read-UInt32BE $pngBytes 20
if ($actualWidth -ne $Width -or $actualHeight -ne $Height) {
    throw "O icone fonte precisa ser ${Width}x${Height}; atual: ${actualWidth}x${actualHeight}."
}

$widthByte = if ($Width -eq 256) { 0 } else { $Width }
$heightByte = if ($Height -eq 256) { 0 } else { $Height }
if ($widthByte -gt 255 -or $heightByte -gt 255) {
    throw "ICO suporta dimensoes ate 256x256."
}

$icoBytes = [System.Collections.Generic.List[byte]]::new()
Add-UInt16LE $icoBytes 0
Add-UInt16LE $icoBytes 1
Add-UInt16LE $icoBytes 1
$icoBytes.Add([byte] $widthByte)
$icoBytes.Add([byte] $heightByte)
$icoBytes.Add(0)
$icoBytes.Add(0)
Add-UInt16LE $icoBytes 1
Add-UInt16LE $icoBytes 32
Add-UInt32LE $icoBytes ([uint32] $pngBytes.Length)
Add-UInt32LE $icoBytes 22
$icoBytes.AddRange($pngBytes)

$destination = $IconPath
if (-not [System.IO.Path]::IsPathRooted($destination)) {
    $destination = Join-Path (Get-Location) $destination
}

$destinationDir = [System.IO.Path]::GetDirectoryName($destination)
if ($destinationDir -and -not [System.IO.Directory]::Exists($destinationDir)) {
    [System.IO.Directory]::CreateDirectory($destinationDir) | Out-Null
}

[System.IO.File]::WriteAllBytes($destination, $icoBytes.ToArray())
Write-Host "Icone ICO gerado em $destination (${Width}x${Height})."
