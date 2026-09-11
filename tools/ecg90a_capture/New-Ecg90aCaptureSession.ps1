[CmdletBinding()]
param(
    [string]$Root = (Join-Path (Get-Location) 'captures\ecg90a\sessions'),
    [string]$Device = 'ECG90A',
    [ValidateSet('usbpcap', 'hid', 'serial', 'bulk', 'unknown')]
    [string]$Transport = 'unknown',
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
# ECG90A capture session

| Field | Value |
| --- | --- |
| Created UTC | $createdAt |
| Device | $Device |
| Serial number |  |
| Firmware |  |
| Operator | $Operator |
| Host OS | $([System.Environment]::OSVersion.VersionString) |
| Official software |  |
| Transport | $Transport |
| VID/PID |  |
| Scenario |  |
| Signal source |  |
| Patient data present | no |

## Capture files

| File | Scenario | Notes |
| --- | --- | --- |

## Notes

"@

$observations = @"
# Observacoes da sessao ECG90A

## Linha do tempo

| Hora local | Acao | Observacao |
| --- | --- | --- |
| HH:MM:SS | Conectar aparelho |  |
| HH:MM:SS | Abrir software oficial |  |
| HH:MM:SS | Iniciar aquisicao |  |
| HH:MM:SS | Waveform visivel |  |
| HH:MM:SS | Parar aquisicao |  |
| HH:MM:SS | Fechar software |  |

## Ambiente

- Fonte de sinal:
- Paciente real: nao
- Software oficial:
- Firmware:
- Cabo/hub USB:

## Observacoes livres

"@

$findings = @"
# Achados da sessao ECG90A

## Transporte

- Tipo:
- VID/PID:
- Interfaces:
- Endpoints:
- Driver:

## Sequencias confirmadas

| Sequencia | Direcao | Evidencia | Status |
| --- | --- | --- | --- |

## Waveform

- Inicio provavel:
- Tamanho de frame:
- Intervalo medio:
- Taxa estimada:
- Derivacoes:
- Escala:
- Checksum/CRC:

## Duvidas abertas

"@

$framesHeader = 'timestamp_utc,direction,transport,interface,endpoint,transfer_type,command,length,payload_hex,decoded_hint,notes'

Set-Content -LiteralPath (Join-Path $sessionPath 'manifest.md') -Value $manifest -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'usb_inventory.csv') -Value '' -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'analysis\frames.csv') -Value $framesHeader -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'analysis\findings.md') -Value $findings -Encoding UTF8
Set-Content -LiteralPath (Join-Path $sessionPath 'notes\observations.md') -Value $observations -Encoding UTF8

Write-Host "Created ECG90A capture session:"
Write-Host $sessionPath
