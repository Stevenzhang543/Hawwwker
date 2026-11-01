@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0.."
call npm run tauri build || exit /b 1
echo.
echo 免安装程序：%cd%\src-tauri\target\release\hawwwker.exe
echo 安装包目录：%cd%\src-tauri\target\release\bundle\
explorer "%cd%\src-tauri\target\release\bundle"
endlocal
