@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0.."
echo [Hawwwker] 检查环境...
where node >nul 2>nul || (echo 未找到 Node.js，请先安装 Node 20+ : https://nodejs.org & exit /b 1)
where cargo >nul 2>nul || (echo 未找到 Rust，请先安装 rustup : https://rustup.rs & exit /b 1)
node -v
cargo -V
rustup default stable-x86_64-pc-windows-msvc >nul 2>nul
echo [Hawwwker] 安装前端依赖...
call npm install || exit /b 1
echo [Hawwwker] 预编译 Rust 依赖（首次较慢，约 3-8 分钟）...
pushd src-tauri
cargo build || (popd & exit /b 1)
popd
echo.
echo 完成。开发模式：scripts\dev.bat    打包：scripts\build.bat
endlocal
