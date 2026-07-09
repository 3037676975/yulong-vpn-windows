use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder, Manager};

const API_BASE: &str = "https://api2.smilechat.cn";
const LOCAL_PROXY: &str = "127.0.0.1:7890";

static CORE_PROCESS: Lazy<Mutex<Option<Child>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Deserialize)]
struct AccessApiResponse {
    ok: Option<bool>,
    #[serde(rename = "expiresAt")]
    expires_at_camel: Option<String>,
    expires_at: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    ok: bool,
    expires_at: Option<String>,
    message: String,
}

#[derive(Debug, Deserialize)]
struct NoticeItem {
    title: Option<String>,
    content: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NoticeApiResponse {
    List { items: Vec<NoticeItem> },
    One(NoticeItem),
}

#[derive(Debug, Serialize)]
struct NoticeResponse {
    title: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ConnectResponse {
    ok: bool,
    status: String,
    nodes: Vec<String>,
    message: String,
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

fn core_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join("bin").join("mihomo.exe"))
}

fn extract_nodes_from_yaml(yaml: &str) -> Vec<String> {
    let mut nodes = Vec::new();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name:") {
            let name = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !name.is_empty() && !nodes.contains(&name) {
                nodes.push(name);
            }
        }
        if nodes.len() >= 12 {
            break;
        }
    }
    if nodes.is_empty() {
        nodes.push("自动选择".to_string());
    }
    nodes
}

fn set_windows_proxy(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let enable_value = if enabled { "1" } else { "0" };
        let status = Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                "ProxyEnable",
                "/t",
                "REG_DWORD",
                "/d",
                enable_value,
                "/f",
            ])
            .status()
            .map_err(|err| format!("设置 ProxyEnable 失败：{err}"))?;
        if !status.success() {
            return Err("设置 ProxyEnable 失败".to_string());
        }

        let status = Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                "ProxyServer",
                "/t",
                "REG_SZ",
                "/d",
                LOCAL_PROXY,
                "/f",
            ])
            .status()
            .map_err(|err| format!("设置 ProxyServer 失败：{err}"))?;
        if !status.success() {
            return Err("设置 ProxyServer 失败".to_string());
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = enabled;
        Ok(())
    }
}

async fn download_config(code: String) -> Result<(String, Vec<String>), String> {
    let url = format!("{API_BASE}/api/clash-config?code={code}");
    let text = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|err| format!("请求 Clash 配置失败：{err}"))?
        .error_for_status()
        .map_err(|err| format!("后台拒绝返回 Clash 配置：{err}"))?
        .text()
        .await
        .map_err(|err| format!("读取 Clash 配置失败：{err}"))?;

    let nodes = extract_nodes_from_yaml(&text);
    let path = config_path()?;
    fs::write(path, &text).map_err(|err| format!("保存 Clash 配置失败：{err}"))?;
    Ok((text, nodes))
}

fn start_core_if_exists() -> Result<bool, String> {
    let path = core_path()?;
    if !path.exists() {
        return Ok(false);
    }

    let cfg = config_path()?;
    let mut lock = CORE_PROCESS.lock().map_err(|_| "核心进程锁异常".to_string())?;
    if lock.is_some() {
        return Ok(true);
    }

    let child = Command::new(path)
        .arg("-f")
        .arg(cfg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("启动 mihomo 核心失败：{err}"))?;
    *lock = Some(child);
    Ok(true)
}

fn stop_core() {
    if let Ok(mut lock) = CORE_PROCESS.lock() {
        if let Some(mut child) = lock.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[tauri::command]
async fn login_access_code(code: String) -> Result<LoginResponse, String> {
    let res = reqwest::Client::new()
        .post(format!("{API_BASE}/api/access-code"))
        .json(&serde_json::json!({
            "code": code,
            "clientId": "windows",
            "pluginVersion": "windows-v1"
        }))
        .send()
        .await
        .map_err(|err| format!("连接后台失败：{err}"))?;

    let data: AccessApiResponse = res
        .json()
        .await
        .map_err(|err| format!("解析后台返回失败：{err}"))?;

    let ok = data.ok.unwrap_or(false);
    let expires_at = data.expires_at_camel.or(data.expires_at);
    let message = if ok {
        "登录成功".to_string()
    } else {
        data.message.unwrap_or_else(|| "密码错误或已过期".to_string())
    };

    Ok(LoginResponse { ok, expires_at, message })
}

#[tauri::command]
async fn fetch_notice() -> Result<NoticeResponse, String> {
    let res = reqwest::Client::new()
        .get(format!("{API_BASE}/api/notices?public=1"))
        .send()
        .await
        .map_err(|err| format!("获取公告失败：{err}"))?;

    let data: NoticeApiResponse = res
        .json()
        .await
        .map_err(|err| format!("解析公告失败：{err}"))?;

    let item = match data {
        NoticeApiResponse::List { items } => items.into_iter().next(),
        NoticeApiResponse::One(item) => Some(item),
    };

    let title = item.as_ref().and_then(|i| i.title.clone()).unwrap_or_else(|| "欢迎使用玉龙VPN".to_string());
    let content = item
        .and_then(|i| i.content.or(i.body))
        .unwrap_or_else(|| "输入动态验证码后即可使用 Windows 客户端。".to_string());

    Ok(NoticeResponse { title, content })
}

#[tauri::command]
async fn refresh_config(code: String) -> Result<ConnectResponse, String> {
    let (_, nodes) = download_config(code).await?;
    Ok(ConnectResponse {
        ok: true,
        status: "ready".to_string(),
        nodes,
        message: "配置已更新".to_string(),
    })
}

#[tauri::command]
async fn connect_proxy(code: String) -> Result<ConnectResponse, String> {
    let (_, nodes) = download_config(code).await?;
    let core_started = start_core_if_exists()?;
    set_windows_proxy(true)?;

    let message = if core_started {
        "已启动代理核心并开启系统代理".to_string()
    } else {
        "已下载配置并开启系统代理。注意：当前仓库未内置 mihomo.exe，需要后续放入 APPDATA/YulongVPN/bin/mihomo.exe 后才能真正启动代理核心。".to_string()
    };

    Ok(ConnectResponse {
        ok: true,
        status: "connected".to_string(),
        nodes,
        message,
    })
}

#[tauri::command]
async fn disconnect_proxy() -> Result<ConnectResponse, String> {
    set_windows_proxy(false)?;
    stop_core();
    Ok(ConnectResponse {
        ok: true,
        status: "disconnected".to_string(),
        nodes: vec!["自动选择".to_string()],
        message: "已关闭系统代理并停止代理核心".to_string(),
    })
}

#[tauri::command]
async fn set_system_proxy(enabled: bool) -> Result<(), String> {
    set_windows_proxy(enabled)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "显示玉龙VPN", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            let _tray = TrayIconBuilder::new()
                .tooltip("玉龙VPN Windows")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            login_access_code,
            fetch_notice,
            refresh_config,
            connect_proxy,
            disconnect_proxy,
            set_system_proxy
        ])
        .run(tauri::generate_context!())
        .expect("error while running Yulong VPN Windows");
}
