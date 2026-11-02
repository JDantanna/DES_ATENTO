; Gerar com: build.ps1 (passa /DAppVersion=x.y.z)
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
AppId={{B7C1D8E2-4F3A-4C55-9A61-5D0E7A2F9C14}
AppName=Des_Atento
AppVersion={#AppVersion}
AppPublisher=Des_Atento
; Instalação por usuário: atualizar não pede administrador.
PrivilegesRequired=lowest
DefaultDirName={autopf}\Des_Atento
DefaultGroupName=Des_Atento
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=Des_Atento_Setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\des_atento.exe
WizardImageFile=..\assets\wizard.bmp
WizardSmallImageFile=..\assets\wizard_small.bmp
WizardStyle=modern
Compression=lzma2/ultra64
SolidCompression=yes
CloseApplications=yes
RestartApplications=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "Iniciar com o Windows (necessário para o Coach medir o dia todo)"; GroupDescription: "Opções:"; Flags: unchecked

[Files]
Source: "..\target\release\des_atento.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Des_Atento"; Filename: "{app}\des_atento.exe"
Name: "{autodesktop}\Des_Atento"; Filename: "{app}\des_atento.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Des_Atento"; ValueData: """{app}\des_atento.exe"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
; Instalação normal: caixa "Executar Des_Atento".
Filename: "{app}\des_atento.exe"; Description: "Abrir o Des_Atento"; Flags: nowait postinstall skipifsilent
; Atualização silenciosa feita pelo próprio app: reabre automaticamente.
Filename: "{app}\des_atento.exe"; Flags: nowait; Check: WizardSilent
