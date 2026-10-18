; Inno Setup script for the Windows installer, compiled by deploy/package.sh
; (windows-x86_64). It takes the version with /DVersion=<version>.

#ifndef Version
  #error Pass the version with /DVersion=<version>
#endif

[Setup]
; Never change the AppId: upgrades and the uninstaller find Schwitzer by it.
AppId={{2DADCE5D-8E7F-4B18-BC78-032C5F14821F}
AppName=Schwitzer
AppVersion={#Version}
AppVerName=Schwitzer {#Version}
AppPublisher=griasdi
AppPublisherURL=https://github.com/griasdi/schwitzer
AppSupportURL=https://github.com/griasdi/schwitzer/issues
AppUpdatesURL=https://github.com/griasdi/schwitzer/releases
; Per user, without admin rights: the app updates itself (src/update.rs) and
; replaces its own schwitzer.exe, so its folder must be writable.
DefaultDirName={localappdata}\Programs\Schwitzer
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\crates\schwitzer-app\assets\logo\schwitzer.ico
UninstallDisplayIcon={app}\schwitzer.exe
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
OutputDir=..\dist

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\schwitzer.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Schwitzer"; Filename: "{app}\schwitzer.exe"
Name: "{autodesktop}\Schwitzer"; Filename: "{app}\schwitzer.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\schwitzer.exe"; Description: "{cm:LaunchProgram,Schwitzer}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; What a self-update leaves beside the program until the next start (update.rs, clean_leftovers).
Type: files; Name: "{app}\schwitzer.exe.old"
Type: files; Name: "{app}\schwitzer.exe.new"
