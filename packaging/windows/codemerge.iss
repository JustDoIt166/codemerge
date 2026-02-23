#define AppName "CodeMerge"
#define AppPublisher "CodeMerge Maintainers"
#define AppURL "https://github.com/hellotime/codemerge"
#define AppExeName "codemerge.exe"

[Setup]
AppId={{5B244A31-9F38-4E75-8AFA-6A85B7E661DD}
AppName={#AppName}
AppVersion={#GetEnv("APP_VERSION")}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
OutputDir={#GetEnv("OUTPUT_DIR")}
OutputBaseFilename={#GetEnv("INSTALLER_BASENAME")}
Compression=lzma
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64
WizardStyle=modern
PrivilegesRequired=admin
SetupIconFile={#GetEnv("ICON_PATH")}
UninstallDisplayIcon={app}\codemerge.ico

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#GetEnv("BINARY_PATH")}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GetEnv("ICON_PATH")}"; DestDir: "{app}"; DestName: "codemerge.ico"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\codemerge.ico"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\codemerge.ico"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
