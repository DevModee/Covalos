; COVALOS Windows installer. Build through tools/build-windows-installer.ps1.
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PayloadDir
  #error PayloadDir is required
#endif
#ifndef BuildOutputDir
  #error BuildOutputDir is required
#endif
#define AppGuid "{9734E261-3523-49A9-9571-D2AA32411CF1}"

[Setup]
AppId={{9734E261-3523-49A9-9571-D2AA32411CF1}
AppName=COVALOS
AppVersion={#AppVersion}
AppVerName=COVALOS {#AppVersion}
AppPublisher=Matias Sánchez
AppComments=Native Rust IDE
VersionInfoVersion={#AppVersion}.0
VersionInfoProductName=COVALOS
VersionInfoDescription=COVALOS Windows Installer
DefaultDirName={localappdata}\Programs\Covalos
DefaultGroupName=COVALOS
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#BuildOutputDir}
OutputBaseFilename=Covalos-{#AppVersion}-Windows-x64-Setup
SetupIconFile={#PayloadDir}\covalos.ico
UninstallDisplayIcon={app}\covalos.exe
UninstallDisplayName=COVALOS
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
WizardResizable=yes
DisableWelcomePage=no
CloseApplications=yes
CloseApplicationsFilter=covalos.exe
RestartApplications=no
SetupLogging=yes
ChangesAssociations=yes
UsePreviousTasks=yes
AllowNoIcons=yes

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
spanish.DesktopIcon=Crear un acceso directo en el &escritorio
english.DesktopIcon=Create a &desktop shortcut
spanish.ContextMenu=Agregar «Abrir con COVALOS» al menú contextual de archivos y carpetas
english.ContextMenu=Add "Open with COVALOS" to the file and folder context menu
spanish.OpenWith=Abrir con COVALOS
english.OpenWith=Open with COVALOS
spanish.Launch=Iniciar COVALOS
english.Launch=Launch COVALOS

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "contextmenu"; Description: "{cm:ContextMenu}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#PayloadDir}\covalos.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\WebView2Loader.dll"; DestDir: "{app}"
Source: "{#PayloadDir}\covalos.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\THIRD_PARTY_NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\COVALOS"; Filename: "{app}\covalos.exe"; WorkingDir: "{app}"; AppUserModelID: "Covalos.IDE"
Name: "{autodesktop}\COVALOS"; Filename: "{app}\covalos.exe"; WorkingDir: "{app}"; AppUserModelID: "Covalos.IDE"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\covalos.exe"; ValueType: string; ValueName: ""; ValueData: "{app}\covalos.exe"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\*\shell\Covalos"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenWith}"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\*\shell\Covalos"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\covalos.exe"; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\*\shell\Covalos\command"; ValueType: string; ValueName: ""; ValueData: """{app}\covalos.exe"" ""%1"""; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\shell\Covalos"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenWith}"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\shell\Covalos"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\covalos.exe"; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\shell\Covalos\command"; ValueType: string; ValueName: ""; ValueData: """{app}\covalos.exe"" ""%1"""; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\Covalos"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenWith}"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\Covalos"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\covalos.exe"; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\Covalos\command"; ValueType: string; ValueName: ""; ValueData: """{app}\covalos.exe"" ""%V"""; Tasks: contextmenu

[Run]
Filename: "{app}\covalos.exe"; Description: "{cm:Launch}"; Flags: nowait postinstall skipifsilent unchecked

; Never delete profiles, projects or untracked files on uninstall.
; A stable AppId keeps upgrades and reinstalls in one Apps entry.
