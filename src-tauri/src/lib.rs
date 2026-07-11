#![cfg_attr(windows, windows_subsystem = "windows")]

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
#[cfg(windows)]
use std::ffi::c_void;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, WindowEvent,
};

const API_BASE: &str = "https://api2.smilechat.cn";
const LOCAL_PROXY_HOST: &str = "127.0.0.1";
// Use app-specific ports instead of the Clash defaults. 7890/9090 are commonly
// occupied by another proxy client, which made a valid mihomo configuration
// look like a startup failure on real Windows machines.
const LOCAL_PROXY_PORT: u16 = 17_890;
const CONTROLLER_PORT: u16 = 19_090;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
#[link(name = "wininet")]
extern "system" {
    fn InternetSetOptionW(
        internet: *mut c_void,
        option: u32,
        buffer: *mut c_void,
        buffer_length: u32,
    ) -> i32;
}

#[cfg(windows)]
fn notify_windows_proxy_changed() {
    const INTERNET_OPTION_REFRESH: u32 = 37;
    const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
    unsafe {
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        );
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null_mut(),
            0,
        );
    }
}

#[cfg(not(windows))]
fn notify_windows_proxy_changed() {}

static CORE_PROCESS: Lazy<Mutex<Option<Child>>> = Lazy::new(|| Mutex::new(None));
static ACTIVE_CODE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static ACTIVE_EXPIRES: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static ACTIVE_GROUP: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static AUTO_TARGET: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static CURRENT_NODE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Deserialize)]
struct AccessApiResponse {
    ok: Option<bool>,
    #[serde(rename = "expiresAt")]
    expires_at_camel: Option<String>,
    expires_at: Option<String>,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct AuthResult {
    valid: bool,
    network_error: bool,
    expires_at: Option<String>,
    message: String,
}

#[derive(Debug, Serialize)]
struct SessionResponse {
    ok: bool,
    network_error: bool,
    expires_at: Option<String>,
    message: String,
}

#[derive(Debug, Serialize)]
struct NoticeResponse {
    title: String,
    content: String,
    url: Option<String>,
}

#[derive(Debug, Serialize)]
struct BrandingResponse {
    logo_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConnectResponse {
    ok: bool,
    status: String,
    nodes: Vec<String>,
    current_node: Option<String>,
    group: Option<String>,
    config_updated_at: Option<u64>,
    message: String,
}

#[derive(Debug, Serialize)]
struct NodeSelectionResponse {
    ok: bool,
    current_node: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct AppStateResponse {
    logged_in: bool,
    connected: bool,
    system_proxy: bool,
    expires_at: Option<String>,
    nodes: Vec<String>,
    current_node: Option<String>,
    group: Option<String>,
    config_updated_at: Option<u64>,
    core_version: Option<String>,
}

#[derive(Debug, Serialize)]
struct SelfCheckResponse {
    ok: bool,
    core_ready: bool,
    backend_ready: bool,
    core_version: Option<String>,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedSession {
    code: String,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ClashYaml {
    #[serde(default)]
    proxies: Vec<YamlProxy>,
    #[serde(rename = "proxy-groups", default)]
    proxy_groups: Vec<YamlGroup>,
}

#[derive(Debug, Deserialize)]
struct YamlProxy {
    name: String,
}

#[derive(Debug, Deserialize, Clone)]
struct YamlGroup {
    name: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    proxies: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ConfigInfo {
    nodes: Vec<String>,
    select_group: Option<String>,
    auto_target: Option<String>,
}

fn app_data_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "无法读取 APPDATA 目录".to_string())?;
    let dir = base.join("YulongVPN");
    fs::create_dir_all(&dir).map_err(|err| format!("创建数据目录失败：{err}"))?;
    Ok(dir)
}

fn config_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join("config.yaml"))
}

fn core_dir() -> Result<PathBuf, String> {
    let dir = app_data_dir()?.join("bin");
    fs::create_dir_all(&dir).map_err(|err| format!("创建核心目录失败：{err}"))?;
    Ok(dir)
}

fn core_path() -> Result<PathBuf, String> {
    Ok(core_dir()?.join("mihomo.exe"))
}

fn core_log_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join("mihomo.log"))
}

fn pid_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join("mihomo.pid"))
}

fn session_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join("session.json"))
}

fn config_updated_at() -> Option<u64> {
    let path = config_path().ok()?;
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    modified.duration_since(UNIX_EPOCH).ok().map(|v| v.as_secs())
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_command: &mut Command) {}

fn run_hidden(program: &str, args: &[&str]) -> Result<std::process::ExitStatus, String> {
    let mut command = Command::new(program);
    command.args(args);
    hide_console(&mut command);
    command
        .status()
        .map_err(|err| format!("执行 {program} 失败：{err}"))
}

fn reg_add(name: &str, value_type: &str, value: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = run_hidden(
            "reg",
            &[
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                name,
                "/t",
                value_type,
                "/d",
                value,
                "/f",
            ],
        )?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("写入 Windows 系统代理设置失败：{name}"))
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (name, value_type, value);
        Ok(())
    }
}

fn set_windows_proxy(enabled: bool) -> Result<(), String> {
    if enabled {
        reg_add(
            "ProxyServer",
            "REG_SZ",
            &format!("127.0.0.1:{LOCAL_PROXY_PORT}"),
        )?;
        reg_add("ProxyOverride", "REG_SZ", "<local>")?;
        reg_add("ProxyEnable", "REG_DWORD", "1")?;
    } else {
        reg_add("ProxyEnable", "REG_DWORD", "0")?;
    }
    notify_windows_proxy_changed();
    Ok(())
}

fn is_windows_proxy_enabled() -> bool {
    #[cfg(windows)]
    {
        let mut command = Command::new("reg");
        command.args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
        ]);
        hide_console(&mut command);
        command
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).to_ascii_lowercase())
            .map(|text| text.contains("0x1"))
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    {
        false
    }
}

fn set_active_session(code: Option<String>, expires_at: Option<String>) {
    if let Ok(mut lock) = ACTIVE_CODE.lock() {
        *lock = code;
    }
    if let Ok(mut lock) = ACTIVE_EXPIRES.lock() {
        *lock = expires_at;
    }
}

fn active_code() -> Option<String> {
    ACTIVE_CODE.lock().ok().and_then(|value| value.clone())
}

fn active_expires() -> Option<String> {
    ACTIVE_EXPIRES.lock().ok().and_then(|value| value.clone())
}

fn save_session(code: &str, expires_at: Option<String>) -> Result<(), String> {
    let data = PersistedSession {
        code: code.to_string(),
        expires_at,
    };
    let json = serde_json::to_vec(&data).map_err(|err| format!("保存登录状态失败：{err}"))?;
    fs::write(session_path()?, json).map_err(|err| format!("保存登录状态失败：{err}"))
}

fn load_session() -> Option<PersistedSession> {
    let data = fs::read(session_path().ok()?).ok()?;
    serde_json::from_slice(&data).ok()
}

fn clear_session() {
    set_active_session(None, None);
    if let Ok(path) = session_path() {
        let _ = fs::remove_file(path);
    }
}

async fn verify_access_code(code: &str) -> AuthResult {
    let response = match reqwest::Client::new()
        .post(format!("{API_BASE}/api/access-code"))
        .json(&serde_json::json!({
            "code": code,
            "clientId": "windows",
            "pluginVersion": "windows-v1.0.4"
        }))
        .timeout(Duration::from_secs(12))
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return AuthResult {
                valid: false,
                network_error: true,
                expires_at: None,
                message: format!("无法连接玉龙VPN后台：{err}"),
            }
        }
    };

    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(err) => {
            return AuthResult {
                valid: false,
                network_error: true,
                expires_at: None,
                message: format!("读取后台返回失败：{err}"),
            }
        }
    };

    let data: AccessApiResponse = match serde_json::from_str(&body) {
        Ok(data) => data,
        Err(err) => {
            return AuthResult {
                valid: false,
                network_error: true,
                expires_at: None,
                message: format!("后台返回格式异常：{err}"),
            }
        }
    };

    let expires_at = data.expires_at_camel.or(data.expires_at);
    let valid = status.is_success() && data.ok.unwrap_or(false);
    let message = if valid {
        "登录验证成功".to_string()
    } else {
        data.message
            .or(data.error)
            .unwrap_or_else(|| "动态密码错误或已过期".to_string())
    };

    AuthResult {
        valid,
        network_error: false,
        expires_at,
        message,
    }
}

fn json_string(value: &JsonValue, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(JsonValue::as_str) {
            let clean = text.trim();
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

fn normalize_config(input: &str) -> String {
    let mut output = input.to_string();
    for (key, value) in [
        ("mixed-port", LOCAL_PROXY_PORT.to_string()),
        (
            "external-controller",
            format!("127.0.0.1:{CONTROLLER_PORT}"),
        ),
        ("allow-lan", "false".to_string()),
        ("secret", "\"\"".to_string()),
    ] {
        output = upsert_top_level(&output, key, &value);
    }
    output
}

fn upsert_top_level(input: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}:");
    let mut found = false;
    let mut lines = Vec::new();

    for line in input.lines() {
        let is_top_level = !line.starts_with(' ') && !line.starts_with('\t');
        if is_top_level && line.trim_start().starts_with(&prefix) {
            if !found {
                lines.push(format!("{key}: {value}"));
                found = true;
            }
        } else {
            lines.push(line.to_string());
        }
    }

    if !found {
        lines.insert(0, format!("{key}: {value}"));
    }
    lines.join("\n") + "\n"
}

fn parse_config_info(yaml: &str) -> ConfigInfo {
    let parsed: ClashYaml = match serde_yaml::from_str(yaml) {
        Ok(parsed) => parsed,
        Err(_) => return fallback_nodes(yaml),
    };

    let actual_names: Vec<String> = parsed
        .proxies
        .iter()
        .map(|proxy| proxy.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    let actual_set: HashSet<String> = actual_names.iter().cloned().collect();

    let select_groups: Vec<YamlGroup> = parsed
        .proxy_groups
        .iter()
        .filter(|group| group.kind.eq_ignore_ascii_case("select"))
        .cloned()
        .collect();

    let selected_group = select_groups
        .iter()
        .find(|group| {
            group.name.contains("自动")
                || group.name.contains("玉龙")
                || group.name.eq_ignore_ascii_case("proxy")
        })
        .or_else(|| select_groups.first())
        .cloned();

    let automatic_groups: HashSet<String> = parsed
        .proxy_groups
        .iter()
        .filter(|group| {
            group.kind.eq_ignore_ascii_case("url-test")
                || group.kind.eq_ignore_ascii_case("fallback")
                || group.kind.eq_ignore_ascii_case("load-balance")
        })
        .map(|group| group.name.clone())
        .collect();

    let mut nodes = Vec::new();
    let mut auto_target = None;

    if let Some(group) = &selected_group {
        auto_target = group
            .proxies
            .iter()
            .find(|name| automatic_groups.contains(*name))
            .cloned();

        if auto_target.is_some() {
            nodes.push("自动选择".to_string());
        }

        for name in &group.proxies {
            if actual_set.contains(name) && !nodes.contains(name) {
                nodes.push(name.clone());
            }
        }
    }

    if nodes.is_empty() {
        nodes.extend(actual_names);
    }

    nodes.retain(|name| {
        !name.eq_ignore_ascii_case("direct")
            && !name.eq_ignore_ascii_case("reject")
            && !name.trim().is_empty()
    });
    nodes.truncate(100);

    ConfigInfo {
        nodes,
        select_group: selected_group.map(|group| group.name),
        auto_target,
    }
}

fn fallback_nodes(yaml: &str) -> ConfigInfo {
    let mut nodes = Vec::new();
    for line in yaml.lines() {
        let trimmed = line.trim();
        let candidate = if let Some(rest) = trimmed.strip_prefix("name:") {
            Some(rest.trim())
        } else if let Some(index) = trimmed.find("name:") {
            Some(trimmed[index + 5..].split(',').next().unwrap_or("").trim())
        } else {
            None
        };

        if let Some(name) = candidate {
            let clean = name.trim_matches(['\'', '"', '{', '}']).trim().to_string();
            if !clean.is_empty() && !nodes.contains(&clean) {
                nodes.push(clean);
            }
        }
        if nodes.len() >= 100 {
            break;
        }
    }
    ConfigInfo {
        nodes,
        select_group: None,
        auto_target: None,
    }
}

fn apply_config_info(info: &ConfigInfo) {
    if let Ok(mut group) = ACTIVE_GROUP.lock() {
        *group = info.select_group.clone();
    }
    if let Ok(mut target) = AUTO_TARGET.lock() {
        *target = info.auto_target.clone();
    }
}

async fn download_config(code: &str) -> Result<(String, ConfigInfo), String> {
    let mut url = reqwest::Url::parse(&format!("{API_BASE}/api/clash-config"))
        .map_err(|err| format!("配置地址无效：{err}"))?;
    url.query_pairs_mut().append_pair("code", code);

    let response = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|err| format!("请求配置失败：{err}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("读取配置失败：{err}"))?;

    if !status.is_success() {
        return Err(format!("后台拒绝返回配置（HTTP {status}）"));
    }
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || (!trimmed.contains("proxies:") && !trimmed.contains("proxy-groups:"))
    {
        return Err("后台返回的不是有效 Clash 配置".to_string());
    }

    let normalized = normalize_config(trimmed);
    let info = parse_config_info(&normalized);
    fs::write(config_path()?, &normalized).map_err(|err| format!("保存配置失败：{err}"))?;
    apply_config_info(&info);
    Ok((normalized, info))
}

fn bundled_core_path(app: &AppHandle) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| format!("无法定位程序资源目录：{err}"))?;
    let candidates = [
        resource_dir.join("resources").join("mihomo.exe"),
        resource_dir.join("mihomo.exe"),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "安装包中缺少 mihomo.exe 代理核心".to_string())
}

fn bundled_resource_path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| format!("无法定位程序资源目录：{err}"))?;
    [resource_dir.join("resources").join(name), resource_dir.join(name)]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("安装包中缺少 {name}"))
}

fn ensure_geodata(app: &AppHandle) -> Result<(), String> {
    for name in ["geoip.metadb", "geosite.dat"] {
        let source = bundled_resource_path(app, name)?;
        let target = app_data_dir()?.join(name);
        let should_copy = match (fs::metadata(&source), fs::metadata(&target)) {
            (Ok(source_meta), Ok(target_meta)) => source_meta.len() != target_meta.len(),
            (Ok(_), Err(_)) => true,
            _ => true,
        };
        if should_copy {
            fs::copy(&source, &target)
                .map_err(|err| format!("安装离线规则数据 {name} 失败：{err}"))?;
        }
    }
    Ok(())
}

fn ensure_core(app: &AppHandle) -> Result<PathBuf, String> {
    let source = bundled_core_path(app)?;
    let target = core_path()?;
    let should_copy = match (fs::metadata(&source), fs::metadata(&target)) {
        (Ok(source_meta), Ok(target_meta)) => source_meta.len() != target_meta.len(),
        (Ok(_), Err(_)) => true,
        _ => true,
    };
    if should_copy {
        fs::copy(&source, &target).map_err(|err| format!("安装代理核心失败：{err}"))?;
    }
    Ok(target)
}

fn core_version(app: &AppHandle) -> Option<String> {
    let path = ensure_core(app).ok()?;
    let mut command = Command::new(path);
    command.arg("-v");
    hide_console(&mut command);
    let output = command.output().ok()?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    text.lines().next().map(|line| line.trim().to_string())
}

fn core_is_running() -> bool {
    let mut lock = match CORE_PROCESS.lock() {
        Ok(lock) => lock,
        Err(_) => return false,
    };
    let exited = match lock.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(None) => false,
            Ok(Some(_)) | Err(_) => true,
        },
        None => return false,
    };
    if exited {
        *lock = None;
        false
    } else {
        true
    }
}

fn stop_core() {
    if let Ok(mut lock) = CORE_PROCESS.lock() {
        if let Some(mut child) = lock.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    if let Ok(path) = pid_path() {
        if let Ok(pid) = fs::read_to_string(&path) {
            let clean = pid.trim();
            if !clean.is_empty() {
                #[cfg(windows)]
                {
                    let _ = run_hidden("taskkill", &["/PID", clean, "/T", "/F"]);
                }
            }
        }
        let _ = fs::remove_file(path);
    }
}

fn start_core(app: &AppHandle) -> Result<(), String> {
    stop_core();
    let executable = ensure_core(app)?;
    ensure_geodata(app)?;
    let config = config_path()?;
    if !config.is_file() {
        return Err("尚未下载代理配置".to_string());
    }

    let log_path = core_log_path()?;
    let log = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)
        .map_err(|err| format!("创建核心日志失败：{err}"))?;
    let log_error = log
        .try_clone()
        .map_err(|err| format!("创建核心日志失败：{err}"))?;

    let mut command = Command::new(executable);
    command
        .arg("-d")
        .arg(app_data_dir()?)
        .arg("-f")
        .arg(config)
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_error));
    hide_console(&mut command);

    let child = command
        .spawn()
        .map_err(|err| format!("启动 mihomo 失败：{err}"))?;
    fs::write(pid_path()?, child.id().to_string())
        .map_err(|err| format!("保存核心进程状态失败：{err}"))?;

    let mut lock = CORE_PROCESS
        .lock()
        .map_err(|_| "核心进程状态异常".to_string())?;
    *lock = Some(child);
    Ok(())
}

async fn wait_for_port(port: u16, attempts: usize) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    for _ in 0..attempts {
        if TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

async fn wait_for_core_ready() -> bool {
    // The first start may need to initialise rule/geodata state. Keep waiting
    // while the process is alive, but stop immediately if mihomo exits.
    for _ in 0..120 {
        let proxy_ready = wait_for_port(LOCAL_PROXY_PORT, 1).await;
        let controller_ready = wait_for_port(CONTROLLER_PORT, 1).await;
        if proxy_ready && controller_ready {
            return core_is_running();
        }
        if !core_is_running() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    false
}

fn tail_core_log() -> String {
    let text = core_log_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .skip(lines.len().saturating_sub(8))
        .copied()
        .collect::<Vec<_>>()
        .join(" | ")
}

fn disconnect_internal() {
    let _ = set_windows_proxy(false);
    stop_core();
    if let Ok(mut current) = CURRENT_NODE.lock() {
        *current = None;
    }
}

async fn connect_internal(app: AppHandle) -> Result<ConnectResponse, String> {
    let code = active_code().ok_or_else(|| "请先输入动态密码登录".to_string())?;
    let auth = verify_access_code(&code).await;
    if auth.network_error {
        disconnect_internal();
        return Err(auth.message);
    }
    if !auth.valid {
        disconnect_internal();
        clear_session();
        return Err("后台密码已修改或账号已失效，请重新登录".to_string());
    }

    set_active_session(Some(code.clone()), auth.expires_at.clone());
    save_session(&code, auth.expires_at.clone())?;
    let (_, info) = download_config(&code).await?;

    start_core(&app)?;
    if !wait_for_core_ready().await {
        let detail = tail_core_log();
        disconnect_internal();
        return Err(if detail.is_empty() {
            "代理核心启动失败，请查看日志".to_string()
        } else {
            format!("代理核心启动失败：{detail}")
        });
    }

    if let Err(err) = set_windows_proxy(true) {
        stop_core();
        return Err(err);
    }
    let current_node = controller_current_node().await.or_else(|| {
        if info.auto_target.is_some() {
            Some("自动选择".to_string())
        } else {
            None
        }
    });
    if let Ok(mut current) = CURRENT_NODE.lock() {
        *current = current_node.clone();
    }

    Ok(ConnectResponse {
        ok: true,
        status: "connected".to_string(),
        nodes: info.nodes,
        current_node,
        group: info.select_group,
        config_updated_at: config_updated_at(),
        message: "代理核心已启动，Windows 系统代理已开启".to_string(),
    })
}

async fn controller_current_node() -> Option<String> {
    let group = ACTIVE_GROUP.lock().ok().and_then(|value| value.clone())?;
    let mut url = reqwest::Url::parse(&format!(
        "http://127.0.0.1:{CONTROLLER_PORT}/proxies/"
    ))
    .ok()?;
    url.path_segments_mut().ok()?.push(&group);
    let value: JsonValue = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    value
        .get("now")
        .and_then(JsonValue::as_str)
        .map(str::to_string)
}

#[tauri::command]
async fn login_access_code(code: String) -> Result<SessionResponse, String> {
    let clean = code.trim();
    if clean.len() < 4 || clean.len() > 32 {
        return Ok(SessionResponse {
            ok: false,
            network_error: false,
            expires_at: None,
            message: "请输入正确的动态密码".to_string(),
        });
    }

    let auth = verify_access_code(clean).await;
    if auth.valid {
        set_active_session(Some(clean.to_string()), auth.expires_at.clone());
        save_session(clean, auth.expires_at.clone())?;
    }
    Ok(SessionResponse {
        ok: auth.valid,
        network_error: auth.network_error,
        expires_at: auth.expires_at,
        message: auth.message,
    })
}

#[tauri::command]
async fn restore_session() -> Result<SessionResponse, String> {
    let persisted = match load_session() {
        Some(session) => session,
        None => {
            return Ok(SessionResponse {
                ok: false,
                network_error: false,
                expires_at: None,
                message: "未保存登录状态".to_string(),
            })
        }
    };

    let auth = verify_access_code(&persisted.code).await;
    if auth.valid {
        let expires_at = auth.expires_at.or(persisted.expires_at);
        set_active_session(Some(persisted.code.clone()), expires_at.clone());
        save_session(&persisted.code, expires_at.clone())?;
        return Ok(SessionResponse {
            ok: true,
            network_error: false,
            expires_at,
            message: "已恢复登录状态".to_string(),
        });
    }
    if !auth.network_error {
        clear_session();
    }
    Ok(SessionResponse {
        ok: false,
        network_error: auth.network_error,
        expires_at: None,
        message: auth.message,
    })
}

#[tauri::command]
async fn check_session() -> Result<SessionResponse, String> {
    let code = match active_code().or_else(|| load_session().map(|session| session.code)) {
        Some(code) => code,
        None => {
            return Ok(SessionResponse {
                ok: false,
                network_error: false,
                expires_at: None,
                message: "登录状态不存在".to_string(),
            })
        }
    };

    let auth = verify_access_code(&code).await;
    if auth.valid {
        set_active_session(Some(code.clone()), auth.expires_at.clone());
        save_session(&code, auth.expires_at.clone())?;
    } else {
        disconnect_internal();
        if !auth.network_error {
            clear_session();
        }
    }
    Ok(SessionResponse {
        ok: auth.valid,
        network_error: auth.network_error,
        expires_at: auth.expires_at,
        message: auth.message,
    })
}

#[tauri::command]
async fn logout() -> Result<(), String> {
    disconnect_internal();
    clear_session();
    Ok(())
}

#[tauri::command]
async fn fetch_notice() -> Result<NoticeResponse, String> {
    let response = reqwest::Client::new()
        .get(format!("{API_BASE}/api/notices?public=1"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|err| format!("获取公告失败：{err}"))?;
    let status = response.status();
    let root: JsonValue = response
        .json()
        .await
        .map_err(|err| format!("解析公告失败：{err}"))?;
    if !status.is_success() {
        return Err(format!("获取公告失败（HTTP {status}）"));
    }

    let item = root
        .get("items")
        .and_then(JsonValue::as_array)
        .and_then(|items| items.first())
        .unwrap_or(&root);
    let title = json_string(item, &["title"]).unwrap_or_else(|| "系统公告".to_string());
    let content = json_string(item, &["content", "body"])
        .unwrap_or_else(|| "请输入后台当前动态密码登录玉龙VPN。".to_string());
    let url = json_string(item, &["url"]);
    Ok(NoticeResponse { title, content, url })
}

#[tauri::command]
async fn fetch_branding() -> Result<BrandingResponse, String> {
    let root: JsonValue = match reqwest::Client::new()
        .get(format!("{API_BASE}/api/app-branding"))
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response.json().await.unwrap_or(JsonValue::Null),
        _ => JsonValue::Null,
    };
    let logo_url = json_string(&root, &["logoUrl", "logo_url", "appLogoUrl", "app_logo_url"])
        .or_else(|| {
            root.get("settings").and_then(|settings| {
                json_string(settings, &["logoUrl", "logo_url", "appLogoUrl", "app_logo_url"])
            })
        })
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
    Ok(BrandingResponse { logo_url })
}

#[tauri::command]
async fn refresh_config(app: AppHandle) -> Result<ConnectResponse, String> {
    let code = active_code().ok_or_else(|| "请先登录".to_string())?;
    let auth = verify_access_code(&code).await;
    if auth.network_error {
        disconnect_internal();
        return Err(auth.message);
    }
    if !auth.valid {
        disconnect_internal();
        clear_session();
        return Err("后台密码已修改或账号已失效，请重新登录".to_string());
    }

    let was_connected = core_is_running();
    let (_, info) = download_config(&code).await?;
    let mut current_node = CURRENT_NODE.lock().ok().and_then(|value| value.clone());

    if was_connected {
        stop_core();
        start_core(&app)?;
        if !wait_for_core_ready().await {
            disconnect_internal();
            return Err("配置更新后代理核心重启失败".to_string());
        }
        if let Err(err) = set_windows_proxy(true) {
            disconnect_internal();
            return Err(err);
        }
        current_node = controller_current_node().await.or(current_node);
    }

    Ok(ConnectResponse {
        ok: true,
        status: if was_connected { "connected" } else { "ready" }.to_string(),
        nodes: info.nodes,
        current_node,
        group: info.select_group,
        config_updated_at: config_updated_at(),
        message: if was_connected {
            "配置已更新，代理核心已自动重启".to_string()
        } else {
            "配置已更新".to_string()
        },
    })
}

#[tauri::command]
async fn connect_proxy(app: AppHandle) -> Result<ConnectResponse, String> {
    connect_internal(app).await
}

#[tauri::command]
async fn disconnect_proxy() -> Result<ConnectResponse, String> {
    disconnect_internal();
    let info = fs::read_to_string(config_path()?)
        .ok()
        .map(|text| parse_config_info(&text))
        .unwrap_or_default();
    Ok(ConnectResponse {
        ok: true,
        status: "disconnected".to_string(),
        nodes: info.nodes,
        current_node: None,
        group: info.select_group,
        config_updated_at: config_updated_at(),
        message: "已关闭 Windows 系统代理并停止代理核心".to_string(),
    })
}

#[tauri::command]
async fn set_system_proxy(enabled: bool) -> Result<(), String> {
    if enabled && (!core_is_running() || !wait_for_port(LOCAL_PROXY_PORT, 2).await) {
        return Err("代理核心未运行，不能开启系统代理".to_string());
    }
    set_windows_proxy(enabled)
}

#[tauri::command]
async fn select_node(node: String) -> Result<NodeSelectionResponse, String> {
    if !core_is_running() || !wait_for_port(CONTROLLER_PORT, 2).await {
        return Err("请先连接代理".to_string());
    }
    let group = ACTIVE_GROUP
        .lock()
        .ok()
        .and_then(|value| value.clone())
        .ok_or_else(|| "当前配置由自动策略管理，不支持手动切换".to_string())?;

    let target = if node == "自动选择" {
        AUTO_TARGET
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .ok_or_else(|| "当前配置没有自动选择策略".to_string())?
    } else {
        node.clone()
    };

    let mut url = reqwest::Url::parse(&format!(
        "http://127.0.0.1:{CONTROLLER_PORT}/proxies/"
    ))
        .map_err(|err| format!("控制地址异常：{err}"))?;
    url.path_segments_mut()
        .map_err(|_| "控制地址异常".to_string())?
        .push(&group);
    let response = reqwest::Client::new()
        .put(url)
        .timeout(Duration::from_secs(5))
        .json(&serde_json::json!({ "name": target }))
        .send()
        .await
        .map_err(|err| format!("切换节点失败：{err}"))?;
    if !response.status().is_success() {
        return Err(format!("切换节点失败（HTTP {}）", response.status()));
    }

    if let Ok(mut current) = CURRENT_NODE.lock() {
        *current = Some(node.clone());
    }
    Ok(NodeSelectionResponse {
        ok: true,
        current_node: node.clone(),
        message: format!("已切换到：{node}"),
    })
}

#[tauri::command]
async fn get_app_state(app: AppHandle) -> Result<AppStateResponse, String> {
    let info = fs::read_to_string(config_path()?)
        .ok()
        .map(|text| parse_config_info(&text))
        .unwrap_or_default();
    apply_config_info(&info);

    let connected = core_is_running() && wait_for_port(LOCAL_PROXY_PORT, 2).await;
    let current_node = if connected {
        controller_current_node()
            .await
            .or_else(|| CURRENT_NODE.lock().ok().and_then(|value| value.clone()))
    } else {
        None
    };

    Ok(AppStateResponse {
        logged_in: active_code().is_some() || load_session().is_some(),
        connected,
        system_proxy: is_windows_proxy_enabled(),
        expires_at: active_expires().or_else(|| load_session().and_then(|session| session.expires_at)),
        nodes: info.nodes,
        current_node,
        group: info.select_group,
        config_updated_at: config_updated_at(),
        core_version: core_version(&app),
    })
}

#[tauri::command]
async fn self_check(app: AppHandle) -> Result<SelfCheckResponse, String> {
    let core_version = core_version(&app);
    let core_ready = core_version.is_some();
    let backend_ready = reqwest::Client::new()
        .get(format!("{API_BASE}/api/notices?public=1"))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false);
    let ok = core_ready && backend_ready;
    Ok(SelfCheckResponse {
        ok,
        core_ready,
        backend_ready,
        core_version,
        message: if ok {
            "Windows 客户端核心与后台连接正常".to_string()
        } else {
            "客户端自检未通过，请检查代理核心或后台网络".to_string()
        },
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let application = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            disconnect_internal();
            let _ = ensure_core(app.handle());

            let show = MenuItem::with_id(app, "show", "显示玉龙VPN", true, None::<&str>)?;
            let connect = MenuItem::with_id(app, "connect", "一键连接", true, None::<&str>)?;
            let disconnect = MenuItem::with_id(app, "disconnect", "一键断开", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &connect, &disconnect, &quit])?;

            TrayIconBuilder::new()
                .tooltip("玉龙VPN Windows")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "connect" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = connect_internal(handle).await;
                        });
                    }
                    "disconnect" => disconnect_internal(),
                    "quit" => {
                        disconnect_internal();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            login_access_code,
            restore_session,
            check_session,
            logout,
            fetch_notice,
            fetch_branding,
            refresh_config,
            connect_proxy,
            disconnect_proxy,
            set_system_proxy,
            select_node,
            get_app_state,
            self_check
        ])
        .build(tauri::generate_context!())
        .expect("error while building Yulong VPN Windows");

    application.run(|_, event| {
        if matches!(event, tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }) {
            disconnect_internal();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_required_local_ports() {
        let config = "proxies:\n  - name: A\n    type: ss\nproxy-groups: []\n";
        let normalized = normalize_config(config);
        assert!(normalized.contains("mixed-port: 17890"));
        assert!(normalized.contains("external-controller: 127.0.0.1:19090"));
        assert!(normalized.contains("allow-lan: false"));
    }

    #[test]
    fn extracts_nodes_and_auto_target() {
        let config = r#"
proxies:
  - { name: 香港01, type: ss, server: example.com, port: 443, cipher: aes-128-gcm, password: x }
  - { name: 日本01, type: ss, server: example.com, port: 443, cipher: aes-128-gcm, password: x }
proxy-groups:
  - name: 自动测速
    type: url-test
    proxies: [香港01, 日本01]
    url: http://www.gstatic.com/generate_204
    interval: 300
  - name: 玉龙VPN
    type: select
    proxies: [自动测速, 香港01, 日本01]
"#;
        let info = parse_config_info(config);
        assert_eq!(info.select_group.as_deref(), Some("玉龙VPN"));
        assert_eq!(info.auto_target.as_deref(), Some("自动测速"));
        assert!(info.nodes.contains(&"自动选择".to_string()));
        assert!(info.nodes.contains(&"香港01".to_string()));
    }
}
