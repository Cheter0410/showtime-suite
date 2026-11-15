#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{D832F8E4-E533-492A-BD70-E4C6692C12B6}
AppName=Showtime Suite
AppVersion={#AppVersion}
AppPublisher=Cheter
DefaultDirName={autopf}\Showtime Suite
DefaultGroupName=Showtime Suite
DisableProgramGroupPage=yes
OutputDir=..\..\dist
OutputBaseFilename=Showtime-Suite-Setup-{#AppVersion}
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\showtime-suite.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\..\stage\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\Showtime Suite"; Filename: "{app}\showtime-suite.exe"
Name: "{autodesktop}\Showtime Suite"; Filename: "{app}\showtime-suite.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\showtime-suite.exe"; Description: "{cm:LaunchProgram,Showtime Suite}"; Flags: nowait postinstall
