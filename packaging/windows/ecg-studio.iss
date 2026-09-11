#define AppName "ECG Studio"
#define AppPublisher "ECG Studio"
#define AppExeName "ECGStudio.exe"
#define VcRedistUrl "https://aka.ms/vc14/vc_redist.x64.exe"

#ifndef AppVersion
#define AppVersion "0.1.0"
#endif

[Setup]
AppId={{3E3E4873-9EF8-4318-8175-2D79DB950321}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\ECG Studio
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
OutputDir=..\..\dist
OutputBaseFilename=ECGStudioSetup-{#AppVersion}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
SetupIconFile=..\..\dist\ECGStudio.ico
UninstallDisplayIcon={app}\{#AppExeName}
ChangesAssociations=yes
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce
Name: "taskbaricon"; Description: "Criar atalho da barra de tarefas (melhor esforco)"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce
Name: "associatefiles"; Description: "Associar arquivos ECG ao ECG Studio"; GroupDescription: "Associacoes de arquivo:"; Flags: checkedonce

[Files]
Source: "..\..\dist\ECGStudio.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon
; Windows 10/11 do not guarantee installer-driven taskbar pinning.
Name: "{userappdata}\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; Tasks: taskbaricon

[Registry]
Root: HKCR; Subkey: "ECGStudioFile"; ValueType: string; ValueData: "ECG Studio ECG File"; Flags: uninsdeletekey; Tasks: associatefiles
Root: HKCR; Subkey: "ECGStudioFile\DefaultIcon"; ValueType: string; ValueData: "{app}\{#AppExeName},0"; Tasks: associatefiles
Root: HKCR; Subkey: "ECGStudioFile\shell\open\command"; ValueType: string; ValueData: """{app}\{#AppExeName}"" ""%1"""; Tasks: associatefiles
Root: HKCR; Subkey: "Applications\{#AppExeName}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey; Tasks: associatefiles
Root: HKCR; Subkey: "Applications\{#AppExeName}\shell\open\command"; ValueType: string; ValueData: """{app}\{#AppExeName}"" ""%1"""; Tasks: associatefiles

Root: HKCR; Subkey: ".aecg"; ValueType: string; ValueData: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".aecg\OpenWithProgids"; ValueType: none; ValueName: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".hl7"; ValueType: string; ValueData: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".hl7\OpenWithProgids"; ValueType: none; ValueName: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".c8k"; ValueType: string; ValueData: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".c8k\OpenWithProgids"; ValueType: none; ValueName: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".ecg"; ValueType: string; ValueData: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".ecg\OpenWithProgids"; ValueType: none; ValueName: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".dcm"; ValueType: string; ValueData: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".dcm\OpenWithProgids"; ValueType: none; ValueName: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".dicom"; ValueType: string; ValueData: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".dicom\OpenWithProgids"; ValueType: none; ValueName: "ECGStudioFile"; Flags: uninsdeletevalue; Tasks: associatefiles

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
function IsVcRedistX64Installed(): Boolean;
var
  Installed: Cardinal;
begin
  Result :=
    RegQueryDWordValue(
      HKLM64,
      'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64',
      'Installed',
      Installed
    ) and (Installed = 1);
end;

procedure OpenVcRedistDownload();
var
  ErrorCode: Integer;
begin
  MsgBox(
    'O Microsoft Visual C++ Redistributable x64 nao foi detectado.' + #13#10 + #13#10 +
    'Vou abrir o download oficial da Microsoft.',
    mbInformation,
    MB_OK
  );

  if not ShellExec('open', '{#VcRedistUrl}', '', '', SW_SHOWNORMAL, ewNoWait, ErrorCode) then
  begin
    MsgBox(
      'Nao foi possivel abrir o download automaticamente. Abra manualmente:' + #13#10 +
      '{#VcRedistUrl}',
      mbError,
      MB_OK
    );
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and (not WizardSilent()) and (not IsVcRedistX64Installed()) then
  begin
    OpenVcRedistDownload();
  end;
end;
