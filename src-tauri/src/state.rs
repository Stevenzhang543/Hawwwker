//! 全局状态（全部为 Arc，可廉价克隆后带入后台任务）。

use crate::ai::AiClient;
use crate::browser::login::LoginManager;
use crate::browser::manager::Browser;
use crate::db::Db;
use crate::fx::Fx;
use crate::search::RunRegistry;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub ai: Arc<AiClient>,
    pub browser: Arc<Browser>,
    pub logins: Arc<LoginManager>,
    pub fx: Arc<Fx>,
    pub runs: Arc<RunRegistry>,
    pub http: reqwest::Client,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> crate::error::AppResult<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let db = Db::open(&data_dir.join("hawwwker.db"))?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("Hawwwker/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| crate::error::AppError::internal(e.to_string()))?;
        let settings = crate::settings::load(&db);
        let browser = Arc::new(Browser::new(data_dir.join("browser-profile")));
        browser.configure(settings.search.background, settings.search.concurrency);
        // v26.03：浏览器每次启动后，从加密会话备份恢复丢失的登录 Cookie
        {
            let (db, dir) = (db.clone(), data_dir.clone());
            browser.set_launch_hook(Arc::new(move |current| {
                let all = crate::platforms::all(&db);
                let (ids, params) = crate::browser::vault::plan_restore(&db, &dir, &all, current);
                if !ids.is_empty() {
                    log::info!("session vault: restoring {ids:?}");
                    // 恢复的会话需要重新确认（下次预检主动验证）
                    for id in ids.iter() {
                        crate::browser::login::mark_unverified(&db, id);
                    }
                    crate::browser::vault::note_restored(&ids);
                }
                params
            }));
        }
        Ok(AppState {
            ai: Arc::new(AiClient::new(db.clone(), http.clone())),
            fx: Arc::new(Fx::new(&db)),
            logins: Arc::new(LoginManager::new()),
            runs: Arc::new(RunRegistry::default()),
            browser,
            http,
            db,
            data_dir,
        })
    }
}
