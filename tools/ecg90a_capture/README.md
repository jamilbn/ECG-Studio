# Ferramentas de captura ECG90A

Scripts pequenos para organizar capturas do CONTEC ECG90A no Windows. Eles nao
enviam comandos ao aparelho; apenas criam pastas de sessao e exportam inventario
USB do host.

Criar sessao:

```powershell
.\tools\ecg90a_capture\New-Ecg90aCaptureSession.ps1 -Transport usbpcap -Operator "lab"
```

Testar sem criar arquivos:

```powershell
.\tools\ecg90a_capture\New-Ecg90aCaptureSession.ps1 -DryRun
```

Se o Windows bloquear scripts locais, rode com bypass apenas para o processo:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\ecg90a_capture\New-Ecg90aCaptureSession.ps1 -DryRun
```

Exportar inventario USB:

```powershell
.\tools\ecg90a_capture\Export-Ecg90aUsbInventory.ps1 `
  -OutFile .\captures\ecg90a\sessions\<sessao>\usb_inventory.csv
```

Filtrar candidatos provaveis:

```powershell
.\tools\ecg90a_capture\Export-Ecg90aUsbInventory.ps1 `
  -OnlyLikelyContec `
  -OutFile .\captures\ecg90a\sessions\<sessao>\usb_inventory.csv
```

Os PCAPNG devem ser capturados com USBPcap/Wireshark ou ferramenta equivalente e
salvos na pasta `raw/` da sessao.

Captura HID read-only, quando o aparelho aparece como `HIDClass`:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\ecg90a_capture\Capture-Ecg90aHidReports.ps1 `
  -VidPid 0483:5750 `
  -Seconds 10 `
  -OutFile .\captures\ecg90a\sessions\<sessao>\exports\hid_0483_5750.csv
```
