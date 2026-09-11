param(
    [Parameter(Mandatory = $true)]
    [string] $ExePath,

    [Parameter(Mandatory = $true)]
    [string] $IconPath
)

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class NativeResourceUpdater
{
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern IntPtr BeginUpdateResource(string pFileName, bool bDeleteExistingResources);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool UpdateResource(
        IntPtr hUpdate,
        IntPtr lpType,
        IntPtr lpName,
        ushort wLanguage,
        byte[] lpData,
        uint cbData);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool EndUpdateResource(IntPtr hUpdate, bool fDiscard);
}
'@

function Read-UInt16LE([byte[]] $Bytes, [int] $Offset) {
    return [BitConverter]::ToUInt16($Bytes, $Offset)
}

function Read-UInt32LE([byte[]] $Bytes, [int] $Offset) {
    return [BitConverter]::ToUInt32($Bytes, $Offset)
}

function Write-UInt16LE([System.Collections.Generic.List[byte]] $Bytes, [int] $Value) {
    $Bytes.AddRange([BitConverter]::GetBytes([UInt16] $Value))
}

function Write-UInt32LE([System.Collections.Generic.List[byte]] $Bytes, [uint32] $Value) {
    $Bytes.AddRange([BitConverter]::GetBytes($Value))
}

$resolvedExe = (Resolve-Path -LiteralPath $ExePath).Path
$resolvedIcon = (Resolve-Path -LiteralPath $IconPath).Path
$iconBytes = [System.IO.File]::ReadAllBytes($resolvedIcon)

if ((Read-UInt16LE $iconBytes 0) -ne 0 -or (Read-UInt16LE $iconBytes 2) -ne 1) {
    throw "Arquivo ICO invalido: $resolvedIcon"
}

$iconCount = Read-UInt16LE $iconBytes 4
if ($iconCount -lt 1) {
    throw "Arquivo ICO sem imagens: $resolvedIcon"
}

$has256Icon = $false
for ($index = 0; $index -lt $iconCount; $index++) {
    $entryOffset = 6 + ($index * 16)
    $width = $iconBytes[$entryOffset]
    $height = $iconBytes[$entryOffset + 1]
    if ($width -eq 0 -and $height -eq 0) {
        $has256Icon = $true
        break
    }
}

if (-not $has256Icon) {
    throw "O ICO precisa conter uma imagem 256x256 para evitar reducao do icone: $resolvedIcon"
}

$update = [NativeResourceUpdater]::BeginUpdateResource($resolvedExe, $false)
if ($update -eq [IntPtr]::Zero) {
    throw "BeginUpdateResource falhou: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
}

$discard = $true
try {
    $group = [System.Collections.Generic.List[byte]]::new()
    Write-UInt16LE $group 0
    Write-UInt16LE $group 1
    Write-UInt16LE $group $iconCount

    for ($index = 0; $index -lt $iconCount; $index++) {
        $entryOffset = 6 + ($index * 16)
        $width = $iconBytes[$entryOffset]
        $height = $iconBytes[$entryOffset + 1]
        $colorCount = $iconBytes[$entryOffset + 2]
        $reserved = $iconBytes[$entryOffset + 3]
        $planes = Read-UInt16LE $iconBytes ($entryOffset + 4)
        $bitCount = Read-UInt16LE $iconBytes ($entryOffset + 6)
        $bytesInRes = Read-UInt32LE $iconBytes ($entryOffset + 8)
        $imageOffset = Read-UInt32LE $iconBytes ($entryOffset + 12)
        $resourceId = $index + 1

        $image = New-Object byte[] $bytesInRes
        [Array]::Copy($iconBytes, $imageOffset, $image, 0, $bytesInRes)

        if (-not [NativeResourceUpdater]::UpdateResource(
            $update,
            [IntPtr] 3,
            [IntPtr] $resourceId,
            0,
            $image,
            $image.Length
        )) {
            throw "UpdateResource RT_ICON falhou: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        }

        $group.Add($width)
        $group.Add($height)
        $group.Add($colorCount)
        $group.Add($reserved)
        Write-UInt16LE $group $planes
        Write-UInt16LE $group $bitCount
        Write-UInt32LE $group $bytesInRes
        Write-UInt16LE $group $resourceId
    }

    $groupBytes = $group.ToArray()
    if (-not [NativeResourceUpdater]::UpdateResource(
        $update,
        [IntPtr] 14,
        [IntPtr] 1,
        0,
        $groupBytes,
        $groupBytes.Length
    )) {
        throw "UpdateResource RT_GROUP_ICON falhou: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }

    $discard = $false
}
finally {
    if (-not [NativeResourceUpdater]::EndUpdateResource($update, $discard)) {
        throw "EndUpdateResource falhou: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
}
