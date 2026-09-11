# CONTEC8000G capture tools

Small Windows helpers for organizing CONTEC8000G captures. They do not send
commands to the device.

Create a capture session:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\contec8000g_capture\New-Contec8000GCaptureSession.ps1 `
  -Transport unknown `
  -OfficialSoftware "ECG Workstation V3.4.8"
```

Export USB, serial, and network inventory:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\contec8000g_capture\Export-Contec8000GHostInventory.ps1 `
  -SessionPath .\captures\contec8000g\sessions\<session>
```

Raw captures should be created with Wireshark/USBPcap, a serial monitor, or a
network capture tool and saved under the session's `raw/` folder.

Export derived USBPcap tables without modifying the raw PCAPNG:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\contec8000g_capture\Export-Contec8000GUsbPcap.ps1 `
  -SessionPath .\captures\contec8000g\sessions\<session> `
  -RawFile raw\<capture>.pcapng `
  -BusId <usb-bus-id> `
  -DeviceAddress <usb-device-address>
```
