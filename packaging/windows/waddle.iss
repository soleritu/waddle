; Waddle's Windows installer. Build: iscc packaging\windows\waddle.iss
; It installs for this user only, so it needs no administrator rights. Models, settings, and keys
; live in %LOCALAPPDATA%\Waddle and stay when Waddle is uninstalled.

#ifndef Version
  #define Version "0.1.0-beta"
#endif
#ifndef Binary
  #define Binary "..\..\target\release\waddle.exe"
#endif

[Setup]
AppId={{6F1C2B54-9A0E-4C55-9E7B-7A2D3C8F1E01}
AppName=Waddle
AppVersion={#Version}
AppPublisher=Waddle
AppPublisherURL=https://github.com/soleritu/waddle
DefaultDirName={localappdata}\Programs\Waddle
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\..\target\dist
OutputBaseFilename=Waddle-Setup
SetupIconFile=..\..\src\waddle.ico
UninstallDisplayIcon={app}\waddle.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Files]
Source: "{#Binary}"; DestDir: "{app}"; DestName: "waddle.exe"; Flags: ignoreversion
; Builds made with the GNU toolchain need WebView2's loader beside them. MSVC builds don't.
Source: "..\..\target\release\WebView2Loader.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\..\src\waddle.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Waddle"; Filename: "{app}\waddle.exe"; IconFilename: "{app}\waddle.ico"; Comment: "Local AI hosting"
Name: "{userdesktop}\Waddle"; Filename: "{app}\waddle.exe"; IconFilename: "{app}\waddle.ico"; Comment: "Local AI hosting"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Put Waddle on the Desktop"

[Run]
Filename: "{app}\waddle.exe"; Description: "Open Waddle"; Flags: nowait postinstall

[UninstallRun]
; Quit stops the model servers too
Filename: "{cmd}"; Parameters: "/c taskkill /IM waddle.exe /T /F"; Flags: runhidden; RunOnceId: "StopWaddle"
