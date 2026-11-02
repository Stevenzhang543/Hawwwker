//! 启动自动化浏览器进程。
//!
//! - 管道模式：子进程 fd 3 = 读命令，fd 4 = 写响应（Chromium `--remote-debugging-pipe` 约定）。
//!   Unix：`pre_exec` 里 dup2 到 3/4；Windows：`CreateProcessW` + `lpReserved2`（C 运行库的句柄继承表，Node/libuv 同样做法）。
//! - 生命周期：Windows 把浏览器放进 "KILL_ON_JOB_CLOSE" 作业对象；Linux 设置 PDEATHSIG。
//!   本程序无论正常退出、崩溃还是被结束，浏览器都会一起退出，不会遗留占用用户目录（修复 v26.01 遗留问题）。

use super::cdp::Conn;
use std::sync::Arc;

pub enum Proc {
    Std(std::process::Child),
    #[cfg(windows)]
    Win(win::WinProc),
}

impl Proc {
    /// 进程是否已经退出。
    pub fn exited(&mut self) -> bool {
        match self {
            Proc::Std(c) => matches!(c.try_wait(), Ok(Some(_))),
            #[cfg(windows)]
            Proc::Win(p) => p.exited(),
        }
    }

    pub fn kill(&mut self) {
        match self {
            Proc::Std(c) => {
                let _ = c.kill();
                let _ = c.wait();
            }
            #[cfg(windows)]
            Proc::Win(p) => p.kill(),
        }
    }
}

/// 启动浏览器；pipe=true 时同时返回管道上的 CDP 连接。
pub fn spawn(exe: &str, args: &[String], pipe: bool) -> Result<(Proc, Option<Arc<Conn>>), String> {
    #[cfg(windows)]
    {
        win::spawn(exe, args, pipe)
    }
    #[cfg(unix)]
    {
        unix::spawn(exe, args, pipe)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (exe, args, pipe);
        Err("不支持的操作系统".into())
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::io::AsRawFd;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    pub fn spawn(exe: &str, args: &[String], pipe: bool) -> Result<(Proc, Option<Arc<Conn>>), String> {
        let mut cmd = Command::new(exe);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if !pipe {
            unsafe {
                cmd.pre_exec(|| {
                    #[cfg(target_os = "linux")]
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                    Ok(())
                });
            }
            let child = cmd.spawn().map_err(|e| format!("启动浏览器失败：{e}"))?;
            return Ok((Proc::Std(child), None));
        }
        let (child_read, parent_write) = std::io::pipe().map_err(|e| e.to_string())?;
        let (parent_read, child_write) = std::io::pipe().map_err(|e| e.to_string())?;
        let (cr, cw) = (child_read.as_raw_fd(), child_write.as_raw_fd());
        unsafe {
            cmd.pre_exec(move || {
                // 先复制到高位 fd，避免与 3/4 重叠；dup2 产生的新 fd 不带 CLOEXEC
                let a = libc::fcntl(cr, libc::F_DUPFD, 10);
                let b = libc::fcntl(cw, libc::F_DUPFD, 10);
                if a < 0 || b < 0 || libc::dup2(a, 3) < 0 || libc::dup2(b, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                #[cfg(target_os = "linux")]
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        let child = cmd.spawn().map_err(|e| format!("启动浏览器失败：{e}"))?;
        drop(child_read);
        drop(child_write);
        let conn = Conn::from_pipe(Box::new(parent_read), Box::new(parent_write));
        Ok((Proc::Std(child), Some(conn)))
    }
}

#[cfg(windows)]
pub mod win {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, GetExitCodeProcess, ResumeThread, TerminateProcess, WaitForSingleObject, CREATE_SUSPENDED,
        CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTUPINFOW,
    };

    const STILL_ACTIVE: u32 = 259;

    pub struct WinProc {
        process: HANDLE,
        job: HANDLE,
    }

    // 句柄只在本结构内使用，跨线程移动安全
    unsafe impl Send for WinProc {}

    impl WinProc {
        pub fn exited(&mut self) -> bool {
            let mut code: u32 = 0;
            unsafe { GetExitCodeProcess(self.process, &mut code) != 0 && code != STILL_ACTIVE }
        }

        pub fn kill(&mut self) {
            unsafe {
                TerminateProcess(self.process, 1);
                WaitForSingleObject(self.process, 3000);
            }
        }
    }

    impl Drop for WinProc {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.process);
                // 关闭作业句柄 → 作业内（浏览器及其子进程）全部结束
                CloseHandle(self.job);
            }
        }
    }

    /// MSVC 命令行引号规则。
    pub fn quote(arg: &str) -> String {
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
            return arg.to_string();
        }
        let mut out = String::from("\"");
        let mut backslashes = 0;
        for c in arg.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    out.push_str(&"\\".repeat(backslashes * 2 + 1));
                    out.push('"');
                    backslashes = 0;
                }
                _ => {
                    out.push_str(&"\\".repeat(backslashes));
                    out.push(c);
                    backslashes = 0;
                }
            }
        }
        out.push_str(&"\\".repeat(backslashes * 2));
        out.push('"');
        out
    }

    unsafe fn pipe(inherit_read: bool) -> Result<(HANDLE, HANDLE), String> {
        let sa = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: std::ptr::null_mut(), bInheritHandle: 1 };
        let mut r: HANDLE = std::ptr::null_mut();
        let mut w: HANDLE = std::ptr::null_mut();
        if CreatePipe(&mut r, &mut w, &sa, 0) == 0 {
            return Err(format!("创建管道失败：{}", std::io::Error::last_os_error()));
        }
        // 只让子进程那一端可继承
        let parent_end = if inherit_read { w } else { r };
        SetHandleInformation(parent_end, HANDLE_FLAG_INHERIT, 0);
        Ok((r, w))
    }

    /// C 运行库句柄继承表：int 数量 + 每个 fd 的标志字节 + 每个 fd 的句柄（紧凑排列）。
    fn crt_table(handles: &[HANDLE]) -> Vec<u8> {
        const FOPEN: u8 = 0x01;
        const FPIPE: u8 = 0x08;
        let n = handles.len();
        let mut buf = Vec::with_capacity(4 + n * (1 + std::mem::size_of::<usize>()));
        buf.extend_from_slice(&(n as i32).to_le_bytes());
        for h in handles {
            buf.push(if *h == INVALID_HANDLE_VALUE { 0 } else { FOPEN | FPIPE });
        }
        for h in handles {
            buf.extend_from_slice(&(*h as usize).to_le_bytes());
        }
        buf
    }

    pub fn spawn(exe: &str, args: &[String], pipe_mode: bool) -> Result<(Proc, Option<Arc<Conn>>), String> {
        unsafe {
            let mut cmdline: Vec<u16> = std::ffi::OsStr::new(
                &std::iter::once(quote(exe)).chain(args.iter().map(|a| quote(a))).collect::<Vec<_>>().join(" "),
            )
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
            let mut si: STARTUPINFOW = std::mem::zeroed();
            si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut table: Vec<u8> = vec![];
            let mut child_ends: Vec<HANDLE> = vec![];
            let mut parent_ends: Option<(HANDLE, HANDLE)> = None;
            if pipe_mode {
                // 管道 1：父写 → 子读（fd 3）；管道 2：子写（fd 4）→ 父读
                let (c_read, p_write) = pipe(true)?;
                let (p_read, c_write) = match pipe(false) {
                    Ok(x) => x,
                    Err(e) => {
                        CloseHandle(c_read);
                        CloseHandle(p_write);
                        return Err(e);
                    }
                };
                table = crt_table(&[INVALID_HANDLE_VALUE, INVALID_HANDLE_VALUE, INVALID_HANDLE_VALUE, c_read, c_write]);
                si.cbReserved2 = table.len() as u16;
                si.lpReserved2 = table.as_mut_ptr();
                child_ends = vec![c_read, c_write];
                parent_ends = Some((p_read, p_write));
            }
            let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
            let ok = CreateProcessW(
                std::ptr::null(),
                cmdline.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                if pipe_mode { 1 } else { 0 },
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
                std::ptr::null(),
                std::ptr::null(),
                &si,
                &mut pi,
            );
            for h in child_ends {
                CloseHandle(h);
            }
            drop(table);
            if ok == 0 {
                let err = std::io::Error::last_os_error();
                if let Some((r, w)) = parent_ends {
                    CloseHandle(r);
                    CloseHandle(w);
                }
                return Err(format!("启动浏览器失败：{err}"));
            }
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if !job.is_null() {
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                AssignProcessToJobObject(job, pi.hProcess);
            }
            ResumeThread(pi.hThread);
            CloseHandle(pi.hThread);
            let proc = Proc::Win(WinProc { process: pi.hProcess, job });
            let conn = parent_ends.map(|(r, w)| {
                let reader = std::fs::File::from_raw_handle(r as _);
                let writer = std::fs::File::from_raw_handle(w as _);
                Conn::from_pipe(Box::new(reader), Box::new(writer))
            });
            Ok((proc, conn))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn quoting() {
            assert_eq!(quote("--lang=zh-CN"), "--lang=zh-CN");
            assert_eq!(quote(r"--user-data-dir=C:\Users\A B\x"), r#""--user-data-dir=C:\Users\A B\x""#);
            assert_eq!(quote(r"C:\dir\"), r"C:\dir\");
            assert_eq!(quote(r"a b\"), r#""a b\\""#);
        }

        #[test]
        fn crt_table_layout() {
            let t = crt_table(&[INVALID_HANDLE_VALUE, INVALID_HANDLE_VALUE, INVALID_HANDLE_VALUE, 7usize as HANDLE, 9usize as HANDLE]);
            assert_eq!(t.len(), 4 + 5 + 5 * 8);
            assert_eq!(&t[..4], &5i32.to_le_bytes());
            assert_eq!(&t[4..9], &[0, 0, 0, 9, 9]);
        }
    }
}
