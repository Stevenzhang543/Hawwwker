; Hawwwker 便携版启动器（单文件 exe，免安装）
; 用法：makensis -DVERSION=26.02 -DFILEVER=26.2.0.0 -DSRCDIR=<release 目录> -DOUTFILE=<输出.exe> -DICON=<icon.ico> portable.nsi
;
; 启动器本身是 64 位程序（Target amd64-unicode）；SilentInstall 下不带 /SD 的 MessageBox 仍会显示。
;
; 行为：
;  1. 检查 64 位系统与 WebView2 运行时（缺少时提示下载，不会静默安装任何东西）；
;  2. 把 hawwwker.exe + WebView2Loader.dll 解到 %LOCALAPPDATA%\Hawwwker\portable\<版本>（同版本只解一次）；
;  3. 数据目录放在本 exe 旁边的 Hawwwker-data 文件夹（目录不可写时回退到系统应用数据目录）；
;  4. 启动应用并立即退出，命令行参数原样传递。

Target amd64-unicode
SetCompressor /SOLID lzma
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

Name "Hawwwker ${VERSION} Portable"
Caption "Hawwwker 货客 ${VERSION} 便携版"
OutFile "${OUTFILE}"
Icon "${ICON}"
RequestExecutionLevel user
SilentInstall silent
ShowInstDetails nevershow
AutoCloseWindow true

VIProductVersion "${FILEVER}"
VIAddVersionKey "ProductName" "Hawwwker 货客"
VIAddVersionKey "FileDescription" "Hawwwker 货客 便携版（AI 多平台购物搜索、比价与盯价）"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "Hawwwker"
VIAddVersionKey "LegalCopyright" "Hawwwker"
VIAddVersionKey "OriginalFilename" "Hawwwker-v${VERSION}-windows-x64-portable.exe"

!define WV2_GUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

Section
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP|MB_OK "Hawwwker 需要 64 位 Windows 10 / 11。"
    Abort
  ${EndIf}

  ; WebView2 运行时（Windows 11 与新版 Windows 10 已自带）
  StrCpy $0 ""
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WV2_GUID}" "pv"
  ${If} $0 == ""
  ${OrIf} $0 == "0.0.0.0"
    ReadRegStr $0 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WV2_GUID}" "pv"
  ${EndIf}
  ${If} $0 == ""
  ${OrIf} $0 == "0.0.0.0"
    MessageBox MB_ICONEXCLAMATION|MB_YESNO "没有检测到 Microsoft Edge WebView2 运行时，Hawwwker 的界面需要它。$\r$\n$\r$\n是否打开微软官方下载页？（安装后重新打开本程序即可）" IDNO wv2_skip
    ExecShell "open" "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
    wv2_skip:
    Abort
  ${EndIf}

  ; 程序文件：同版本只解压一次；正在运行时文件相同会被跳过
  StrCpy $INSTDIR "$LOCALAPPDATA\Hawwwker\portable\${VERSION}"
  SetOutPath "$INSTDIR"
  SetOverwrite ifdiff
  File "${SRCDIR}\hawwwker.exe"
  File "${SRCDIR}\WebView2Loader.dll"

  ; 数据目录：exe 旁边的 Hawwwker-data；不可写（例如放在只读位置）时交给应用使用系统目录
  StrCpy $1 "$EXEDIR\Hawwwker-data"
  ClearErrors
  CreateDirectory "$1"
  FileOpen $2 "$1\.write-test" w
  ${If} ${Errors}
    StrCpy $1 ""
  ${Else}
    FileClose $2
    Delete "$1\.write-test"
  ${EndIf}
  System::Call 'Kernel32::SetEnvironmentVariable(t "HAWWWKER_DATA_DIR", t r1) i'
  System::Call 'Kernel32::SetEnvironmentVariable(t "HAWWWKER_PORTABLE", t "1") i'

  ${GetParameters} $3
  Exec '"$INSTDIR\hawwwker.exe" $3'
SectionEnd
