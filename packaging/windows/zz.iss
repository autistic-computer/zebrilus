#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef VersionInfo
  #define VersionInfo "0.0.0"
#endif
#ifndef BundleDir
  #define BundleDir "dist\zz"
#endif
#ifndef OutputDir
  #define OutputDir "dist"
#endif
#ifndef OutputBaseFilename
  #define OutputBaseFilename "zz-windows-x64-setup"
#endif

#define RepoRoot AddBackslash(SourcePath) + "..\.."

[Setup]
AppId={{4DAEF46A-646C-4AA3-9391-E31B4D32319D}
AppName=zz
AppVersion={#AppVersion}
VersionInfoVersion={#VersionInfo}
AppPublisher=demfabris
AppPublisherURL=https://github.com/autistic-computer/zz
AppSupportURL=https://github.com/autistic-computer/zz/issues
AppUpdatesURL=https://github.com/autistic-computer/zz/releases
SourceDir={#RepoRoot}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DefaultDirName={autopf}\zz
DisableProgramGroupPage=yes
UninstallDisplayName=zz
UninstallDisplayIcon={app}\zz.exe
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBaseFilename}
SetupIconFile={#SourcePath}\..\..\assets\windows\zz.ico

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#BundleDir}\*"; DestDir: "{app}"; Excludes: "*.pdb"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#RepoRoot}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RepoRoot}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\zz"; Filename: "{app}\zz.exe"
Name: "{autodesktop}\zz"; Filename: "{app}\zz.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\zz.exe"; Description: "{cm:LaunchProgram,zz}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[UninstallDelete]
Type: filesandordirs; Name: "{app}"
