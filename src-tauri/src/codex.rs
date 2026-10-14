use crate::{model, WidgetState};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::mpsc,
    time::{timeout, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(15);
const INTERVAL: Duration = Duration::from_secs(60);

// Closing the job handle kills only this widget's server process tree, including
// on abrupt widget termination. It never targets the user's existing Codex app.
struct Job(windows_sys::Win32::Foundation::HANDLE);
unsafe impl Send for Job {}
impl Job {
    fn attach(child: &Child) -> Result<Self, String> {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{JobObjects::*, Threading::*},
        };
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err("无法创建后台进程管理对象".into());
            }
            let job = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
            {
                return Err("无法设置后台进程退出清理".into());
            }
            let process = OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE,
                0,
                child.id().ok_or("后台进程已退出")?,
            );
            if process.is_null() {
                return Err("无法管理 Codex 后台进程".into());
            }
            let assigned = AssignProcessToJobObject(handle, process);
            CloseHandle(process);
            if assigned == 0 {
                return Err("无法隔离 Codex 后台进程".into());
            }
            Ok(job)
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

struct Connection {
    child: Child,
    stdin: ChildStdin,
    messages: mpsc::Receiver<Value>,
    reader: tokio::task::JoinHandle<()>,
    _job: Job,
    next_id: u64,
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        self.reader.abort();
    }
}
pub fn find_cli(custom: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = custom {
        return path.is_file().then_some(path);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let exe = directory.join("codex.exe");
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join(".bin/codex.exe"))
        .filter(|p| p.is_file())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn publish(app: &AppHandle) {
    let state = app.state::<WidgetState>();
    let snapshot = state.snapshot.lock().unwrap().clone();
    let _ = app.emit("quota-updated", snapshot);
}
fn accept(app: &AppHandle, value: &Value, notification: bool) {
    let incoming = model::buckets(value);
    let state = app.state::<WidgetState>();
    let mut snapshot = state.snapshot.lock().unwrap();
    if notification {
        for mut b in incoming {
            if let Some(old) = snapshot.buckets.iter_mut().find(|old| old.id == b.id) {
                let raw = value["rateLimitsByLimitId"]
                    .get(&b.id)
                    .unwrap_or(&value["rateLimits"]);
                // Notifications can carry only one window. Omitted fields are
                // unchanged; explicit null still clears an unavailable window.
                if raw.get("primary").is_none() {
                    b.primary = old.primary.clone();
                }
                if raw.get("secondary").is_none() {
                    b.secondary = old.secondary.clone();
                }
                if raw.get("limitName").is_none() {
                    b.name = old.name.clone();
                }
                *old = b;
            } else {
                snapshot.buckets.push(b);
            }
        }
    } else {
        snapshot.buckets = incoming;
    }
    if !snapshot.buckets.iter().any(|b| b.id == snapshot.selected) {
        snapshot.selected = snapshot
            .buckets
            .iter()
            .find(|b| b.id == "codex")
            .or(snapshot.buckets.first())
            .map(|b| b.id.clone())
            .unwrap_or_default();
    }
    snapshot.updated_at = Some(now());
    snapshot.error = None;
    drop(snapshot);
    publish(app);
}
fn safe_error(value: &Value) -> String {
    let message = value["message"].as_str().unwrap_or("").to_lowercase();
    if message.contains("auth")
        || message.contains("login")
        || message.contains("401")
        || message.contains("sign in")
    {
        "登录已失效，请在 Codex 中重新登录后刷新".into()
    } else if message.contains("support")
        || message.contains("api key")
        || message.contains("chatgpt")
    {
        "当前登录方式不支持额度查询，请使用 ChatGPT 账号登录 Codex".into()
    } else if message.contains("429") || message.contains("too many") {
        "查询过于频繁，稍后将自动重试".into()
    } else {
        "额度查询失败，请检查网络和 Codex 登录状态".into()
    }
}
impl Connection {
    async fn spawn(exe: &Path, cwd: &Path) -> Result<Self, String> {
        let mut command = Command::new(exe);
        command
            .args(["app-server", "--listen", "stdio://"])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|_| "无法启动 Codex CLI，请检查所选程序".to_string())?;
        let job = Job::attach(&child)?;
        let stdin = child.stdin.take().ok_or("无法建立 Codex 输入管道")?;
        let stdout = child.stdout.take().ok_or("无法建立 Codex 输出管道")?;
        let (sender, messages) = mpsc::channel(64);
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if sender.send(value).await.is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            messages,
            reader,
            _job: job,
            next_id: 0,
        })
    }
    async fn send(&mut self, value: Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(&value).map_err(|_| "无法编码请求")?;
        bytes.push(b'\n');
        self.stdin
            .write_all(&bytes)
            .await
            .map_err(|_| "Codex 后台连接已断开".to_string())?;
        self.stdin
            .flush()
            .await
            .map_err(|_| "Codex 后台连接已断开".into())
    }
    async fn notification(&mut self, app: &AppHandle, message: &Value) -> Result<(), String> {
        if message["method"] == "account/rateLimits/updated" {
            accept(app, &message["params"], true);
        }
        if message["method"] == "account/updated" {
            let state = app.state::<WidgetState>();
            let mut s = state.snapshot.lock().unwrap();
            s.buckets.clear();
            s.updated_at = None;
            s.error = Some("账号状态已变化，正在重新读取额度".into());
            drop(s);
            let _ = state.refresh.try_send(());
            publish(app);
        }
        // This read-only client never approves tools or supplies credentials.
        if message.get("method").is_some() && message.get("id").is_some() {
            self.send(json!({"id":message["id"],"error":{"code":-32601,"message":"Read-only quota client"}})).await?;
        }
        Ok(())
    }
    async fn request(
        &mut self,
        app: &AppHandle,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"id":id,"method":method,"params":params}))
            .await?;
        loop {
            let message = self
                .messages
                .recv()
                .await
                .ok_or("Codex 后台进程已退出，将自动重连")?;
            if message["id"] == id && message.get("method").is_none() {
                if let Some(error) = message.get("error") {
                    return Err(safe_error(error));
                }
                return Ok(message["result"].clone());
            }
            self.notification(app, &message).await?;
        }
    }
    async fn initialize(&mut self, app: &AppHandle) -> Result<(), String> {
        self.request(app, "initialize", json!({"clientInfo":{"name":"codex_balance_widget","title":"Codex Balance","version":"0.1.0"},"capabilities":{"experimentalApi":false}})).await?;
        self.send(json!({"method":"initialized"})).await?;
        Ok(())
    }
    async fn read(&mut self, app: &AppHandle) -> Result<Value, String> {
        let account = self
            .request(app, "account/read", json!({"refreshToken":false}))
            .await?;
        if account["account"].is_null() {
            return Err("尚未登录，请先在 Codex 中登录 ChatGPT 账号".into());
        }
        let mode = account["account"]["type"].as_str().unwrap_or("");
        if mode == "apiKey" || mode == "apikey" {
            return Err("API Key 登录不提供套餐额度，请使用 ChatGPT 账号登录".into());
        }
        self.request(app, "account/rateLimits/read", Value::Null)
            .await
    }
}

pub async fn run(app: AppHandle, mut requests: mpsc::Receiver<()>, cwd: PathBuf) {
    let mut connection: Option<Connection> = None;
    let mut connection_path: Option<PathBuf> = None;
    let mut next = Instant::now();
    loop {
        let state = app.state::<WidgetState>();
        // Poll notifications while idle; also detect a crashed child promptly.
        if let Some(c) = connection.as_mut() {
            let mut disconnected = false;
            loop {
                match c.messages.try_recv() {
                    Ok(message) => {
                        if c.notification(&app, &message).await.is_err() {
                            disconnected = true;
                            break;
                        }
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            if disconnected {
                connection = None;
                state.snapshot.lock().unwrap().error =
                    Some("Codex 后台连接断开，将自动重连".into());
                publish(&app);
            }
        }
        if Instant::now() < next {
            tokio::select! {
                message = requests.recv() => { if message.is_none() { break; } }
                _ = tokio::time::sleep(Duration::from_millis(250)) => { continue; }
            }
        }
        if state.stopping.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        while requests.try_recv().is_ok() {}
        state.snapshot.lock().unwrap().refreshing = true;
        publish(&app);
        let custom = state.settings.lock().unwrap().value.cli_path.clone();
        let cli = find_cli(custom);
        let result = timeout(TIMEOUT, async {
            let cli = cli.ok_or("找不到 Codex CLI，请安装或点击“选择 CLI”")?;
            if connection_path.as_ref() != Some(&cli) {
                connection = None;
            }
            if connection.is_none() {
                let mut c = Connection::spawn(&cli, &cwd).await?;
                c.initialize(&app).await?;
                connection = Some(c);
                connection_path = Some(cli);
            }
            connection.as_mut().unwrap().read(&app).await
        })
        .await
        .unwrap_or_else(|_| Err("查询超时，请检查网络；稍后自动重试".into()));
        match result {
            Ok(data) => accept(&app, &data, false),
            Err(error) => {
                state.snapshot.lock().unwrap().error = Some(error);
                connection = None;
            }
        }
        state.snapshot.lock().unwrap().refreshing = false;
        publish(&app);
        next = Instant::now() + INTERVAL;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_never_forward_server_text_or_tokens() {
        assert_eq!(
            safe_error(&json!({"message":"401 secret-token"})),
            "登录已失效，请在 Codex 中重新登录后刷新"
        );
        assert!(!safe_error(&json!({"message":"network: secret-token"})).contains("secret-token"));
    }
}
