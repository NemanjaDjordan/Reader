@echo off
setlocal
del /Q "%APPDATA%\Microsoft\Windows\Start Menu\Programs\Reader.lnk" 2>nul
del /Q "%LOCALAPPDATA%\Programs\Reader\Reader.exe" 2>nul
rmdir "%LOCALAPPDATA%\Programs\Reader" 2>nul
echo Reader was removed.
pause
