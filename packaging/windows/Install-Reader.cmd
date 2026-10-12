@echo off
setlocal
set "INSTALL_DIR=%LOCALAPPDATA%\Programs\Reader"
set "START_MENU=%APPDATA%\Microsoft\Windows\Start Menu\Programs"

if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"
copy /Y "%~dp0Reader.exe" "%INSTALL_DIR%\Reader.exe" >nul

powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('%START_MENU%\Reader.lnk');$s.TargetPath='%INSTALL_DIR%\Reader.exe';$s.WorkingDirectory='%INSTALL_DIR%';$s.IconLocation='%INSTALL_DIR%\Reader.exe,0';$s.Save()"

echo Reader was installed in:
echo %INSTALL_DIR%
echo.
echo A shortcut was added to the Start menu.
pause
