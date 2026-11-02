//! Windows 管道启动验证（在 Wine 或真实 Windows 上运行）：
//!   set HAWWWKER_FAKE_BROWSER=C:\path\fakebrowser.exe & wine_pipe.exe --ignored --nocapture
//! 假浏览器从 fd 3 读命令、向 fd 4 写响应；验证 CRT 句柄继承表、命令行引号与作业对象。

#[test]
#[ignore]
fn pipe_launch_roundtrip() {
    let exe = std::env::var("HAWWWKER_FAKE_BROWSER").expect("set HAWWWKER_FAKE_BROWSER");
    tauri::async_runtime::block_on(async move {
        let args = vec!["--user-data-dir=C:\\Users\\A B\\profile dir".to_string(), "--remote-debugging-pipe".to_string(), "about:blank".to_string()];
        let (mut proc, conn) = hawwwker_lib::browser::launch::spawn(&exe, &args, true).expect("spawn");
        let conn = conn.expect("pipe conn");
        for i in 0..3 {
            let v = conn.send("Browser.getVersion", serde_json::json!({}), std::time::Duration::from_secs(10)).await.expect("response over pipe");
            assert_eq!(v["product"], "FakeBrowser/1.0", "round {i}");
        }
        println!("PIPE_OK transport={}", conn.transport);
        assert!(!proc.exited());
        proc.kill();
        assert!(proc.exited());
        println!("KILL_OK");
    });
}
