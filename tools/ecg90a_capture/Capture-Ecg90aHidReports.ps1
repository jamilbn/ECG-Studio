[CmdletBinding()]
param(
    [string]$OutFile = (Join-Path (Get-Location) 'captures\ecg90a\hid_reports.csv'),
    [string]$VidPid = '',
    [int]$Seconds = 10,
    [int]$ReadTimeoutMs = 700,
    [int]$FallbackReportLength = 64,
    [switch]$ListOnly
)

$ErrorActionPreference = 'Stop'

$source = @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public static class Ecg90aHidNative
{
    private const int DIGCF_PRESENT = 0x00000002;
    private const int DIGCF_DEVICEINTERFACE = 0x00000010;
    private const uint GENERIC_READ = 0x80000000;
    private const uint FILE_SHARE_READ = 0x00000001;
    private const uint FILE_SHARE_WRITE = 0x00000002;
    private const uint OPEN_EXISTING = 3;
    private static readonly IntPtr INVALID_HANDLE_VALUE = new IntPtr(-1);

    [StructLayout(LayoutKind.Sequential)]
    private struct SP_DEVICE_INTERFACE_DATA
    {
        public int cbSize;
        public Guid InterfaceClassGuid;
        public int Flags;
        public IntPtr Reserved;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Auto)]
    private struct SP_DEVICE_INTERFACE_DETAIL_DATA
    {
        public int cbSize;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 1024)]
        public string DevicePath;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct HIDP_CAPS
    {
        public ushort Usage;
        public ushort UsagePage;
        public ushort InputReportByteLength;
        public ushort OutputReportByteLength;
        public ushort FeatureReportByteLength;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 17)]
        public ushort[] Reserved;
        public ushort NumberLinkCollectionNodes;
        public ushort NumberInputButtonCaps;
        public ushort NumberInputValueCaps;
        public ushort NumberInputDataIndices;
        public ushort NumberOutputButtonCaps;
        public ushort NumberOutputValueCaps;
        public ushort NumberOutputDataIndices;
        public ushort NumberFeatureButtonCaps;
        public ushort NumberFeatureValueCaps;
        public ushort NumberFeatureDataIndices;
    }

    [DllImport("hid.dll")]
    private static extern void HidD_GetHidGuid(out Guid hidGuid);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_GetPreparsedData(SafeFileHandle hidDeviceObject, out IntPtr preparsedData);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern bool HidD_FreePreparsedData(IntPtr preparsedData);

    [DllImport("hid.dll", SetLastError = true)]
    private static extern int HidP_GetCaps(IntPtr preparsedData, out HIDP_CAPS capabilities);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern IntPtr SetupDiGetClassDevs(
        ref Guid classGuid,
        IntPtr enumerator,
        IntPtr hwndParent,
        int flags);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiEnumDeviceInterfaces(
        IntPtr deviceInfoSet,
        IntPtr deviceInfoData,
        ref Guid interfaceClassGuid,
        int memberIndex,
        ref SP_DEVICE_INTERFACE_DATA deviceInterfaceData);

    [DllImport("setupapi.dll", SetLastError = true, CharSet = CharSet.Auto)]
    private static extern bool SetupDiGetDeviceInterfaceDetail(
        IntPtr deviceInfoSet,
        ref SP_DEVICE_INTERFACE_DATA deviceInterfaceData,
        ref SP_DEVICE_INTERFACE_DETAIL_DATA deviceInterfaceDetailData,
        int deviceInterfaceDetailDataSize,
        out int requiredSize,
        IntPtr deviceInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    private static extern bool SetupDiDestroyDeviceInfoList(IntPtr deviceInfoSet);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
    private static extern SafeFileHandle CreateFile(
        string fileName,
        uint desiredAccess,
        uint shareMode,
        IntPtr securityAttributes,
        uint creationDisposition,
        uint flagsAndAttributes,
        IntPtr templateFile);

    public static string[] EnumeratePaths()
    {
        Guid hidGuid;
        HidD_GetHidGuid(out hidGuid);
        IntPtr infoSet = SetupDiGetClassDevs(
            ref hidGuid,
            IntPtr.Zero,
            IntPtr.Zero,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);

        if (infoSet == INVALID_HANDLE_VALUE)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }

        try
        {
            var paths = new List<string>();
            int index = 0;
            while (true)
            {
                var data = new SP_DEVICE_INTERFACE_DATA();
                data.cbSize = Marshal.SizeOf(typeof(SP_DEVICE_INTERFACE_DATA));

                if (!SetupDiEnumDeviceInterfaces(infoSet, IntPtr.Zero, ref hidGuid, index, ref data))
                {
                    int error = Marshal.GetLastWin32Error();
                    if (error == 259)
                    {
                        break;
                    }
                    throw new Win32Exception(error);
                }

                int requiredSize;
                var detail = new SP_DEVICE_INTERFACE_DETAIL_DATA();
                detail.cbSize = IntPtr.Size == 8 ? 8 : 4 + Marshal.SystemDefaultCharSize;
                int detailSize = Marshal.SizeOf(typeof(SP_DEVICE_INTERFACE_DETAIL_DATA));

                if (SetupDiGetDeviceInterfaceDetail(infoSet, ref data, ref detail, detailSize, out requiredSize, IntPtr.Zero))
                {
                    paths.Add(detail.DevicePath);
                }

                index++;
            }

            return paths.ToArray();
        }
        finally
        {
            SetupDiDestroyDeviceInfoList(infoSet);
        }
    }

    public static SafeFileHandle OpenReadHandle(string path)
    {
        return CreateFile(
            path,
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            IntPtr.Zero,
            OPEN_EXISTING,
            0,
            IntPtr.Zero);
    }

    public static ushort GetInputReportByteLength(SafeFileHandle handle)
    {
        IntPtr preparsedData;
        if (!HidD_GetPreparsedData(handle, out preparsedData))
        {
            return 0;
        }

        try
        {
            HIDP_CAPS caps;
            int result = HidP_GetCaps(preparsedData, out caps);
            if (result != 0x00110000)
            {
                return 0;
            }
            return caps.InputReportByteLength;
        }
        finally
        {
            HidD_FreePreparsedData(preparsedData);
        }
    }
}
'@

Add-Type -TypeDefinition $source

function ConvertTo-PayloadHex {
    param(
        [byte[]]$Bytes,
        [int]$Count
    )

    if ($Count -le 0) {
        return ''
    }

    return [BitConverter]::ToString($Bytes, 0, $Count).Replace('-', ' ')
}

function Get-CommandHex {
    param(
        [byte[]]$Bytes,
        [int]$Count
    )

    if ($Count -le 0) {
        return ''
    }
    if ($Count -gt 1 -and $Bytes[0] -eq 0) {
        return ('{0:X2}' -f $Bytes[1])
    }
    return ('{0:X2}' -f $Bytes[0])
}

$paths = [Ecg90aHidNative]::EnumeratePaths()
if (-not [string]::IsNullOrWhiteSpace($VidPid)) {
    $needle = $VidPid.ToLowerInvariant() -replace ':', '&pid_'
    if ($needle -notmatch '^vid_') {
        $needle = 'vid_' + $needle
    }
    $paths = $paths | Where-Object { $_.ToLowerInvariant().Contains($needle) }
}

if ($ListOnly) {
    $paths | ForEach-Object { Write-Host $_ }
    return
}

if ($paths.Count -eq 0) {
    throw "Nenhum HID encontrado para o filtro '$VidPid'."
}

$outDir = Split-Path -Parent $OutFile
if (-not [string]::IsNullOrWhiteSpace($outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$writer = [System.IO.StreamWriter]::new($OutFile, $false, [System.Text.UTF8Encoding]::new($false))
try {
    $writer.WriteLine('timestamp_utc,direction,transport,interface,endpoint,transfer_type,command,length,payload_hex,decoded_hint,notes')

    foreach ($path in $paths) {
        Write-Host "Opening HID: $path"
        $handle = [Ecg90aHidNative]::OpenReadHandle($path)
        if ($handle.IsInvalid) {
            Write-Host "Could not open HID for read."
            continue
        }

        $stream = $null
        try {
            $reportLength = [int][Ecg90aHidNative]::GetInputReportByteLength($handle)
            if ($reportLength -le 0) {
                $reportLength = $FallbackReportLength
            }

            Write-Host "Input report length: $reportLength"
            $stream = [System.IO.FileStream]::new($handle, [System.IO.FileAccess]::Read, $reportLength, $false)
            $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
            $reads = 0

            while ([DateTime]::UtcNow -lt $deadline) {
                $buffer = [byte[]]::new($reportLength)
                $task = $stream.ReadAsync($buffer, 0, $buffer.Length)
                try {
                    if (-not $task.Wait($ReadTimeoutMs)) {
                        Write-Host "Read timeout."
                        break
                    }
                } catch {
                    Write-Host "Read failed: $($_.Exception)"
                    break
                }

                $count = $task.Result
                if ($count -le 0) {
                    break
                }

                $timestamp = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ss.fffffffZ')
                $command = Get-CommandHex -Bytes $buffer -Count $count
                $payload = ConvertTo-PayloadHex -Bytes $buffer -Count $count
                $safePath = $path.Replace('"', '""')
                $writer.WriteLine('"{0}",device_to_host,hid,"{1}",,interrupt,"{2}",{3},"{4}",raw_hid_input_report,"{5}"' -f $timestamp, $safePath, $command, $count, $payload, $VidPid)
                $reads++
            }

            Write-Host "Reports captured from this HID: $reads"
        }
        finally {
            if ($stream -ne $null) {
                $stream.Dispose()
            } else {
                $handle.Dispose()
            }
        }
    }
}
finally {
    $writer.Dispose()
}

Write-Host "HID report capture saved:"
Write-Host $OutFile
