; Gerar com: build.ps1 (passa /DAppVersion=x.y.z)
#ifndef AppVersion
  #define AppVersion "0.3.0"
#endif

[Setup]
; Mesmo AppId do Des_Atento: instalar o Navi atualiza a instalação antiga.
AppId={{B7C1D8E2-4F3A-4C55-9A61-5D0E7A2F9C14}
AppName=Navi
AppVersion={#AppVersion}
AppPublisher=Navi
; Instalação por usuário: atualizar não pede administrador.
PrivilegesRequired=lowest
DefaultDirName={autopf}\Navi
DefaultGroupName=Navi
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=Navi_Setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\navi.exe
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
Name: "autostart"; Description: "Iniciar com o Windows (necessário para o Monitoramento medir o dia todo)"; GroupDescription: "Opções:"; Flags: unchecked

[InstallDelete]
; Restos da versão antiga (Des_Atento).
Type: files; Name: "{app}\des_atento.exe"
Type: files; Name: "{group}\Des_Atento.lnk"
Type: files; Name: "{autodesktop}\Des_Atento.lnk"

[Files]
Source: "..\target\release\navi.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Navi"; Filename: "{app}\navi.exe"
Name: "{autodesktop}\Navi"; Filename: "{app}\navi.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "Des_Atento"; Flags: deletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Navi"; ValueData: """{app}\navi.exe"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
; Instalação normal: caixa "Abrir o Navi".
Filename: "{app}\navi.exe"; Description: "Abrir o Navi"; Flags: nowait postinstall skipifsilent
; Atualização silenciosa feita pelo próprio app: reabre automaticamente.
Filename: "{app}\navi.exe"; Flags: nowait; Check: WizardSilent
