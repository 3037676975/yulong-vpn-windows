# 玉龙VPN Windows 客户端

> 玉龙VPN Windows EXE 端。  
> 目标：参考 Android 端逻辑，做一个 Windows 电脑端客户端：动态验证码登录、公告、到期时间、一键连接、一键断开、节点列表、系统代理、开机自启、托盘图标、自动更新配置。

---

## 1. 项目目标

Android 端已经完成后，Windows 端可以复用同一套后台接口。

Windows 端第一版建议先做 **系统代理模式**，因为它比 TUN 全局模式更稳定、更容易打包，不需要一开始就处理虚拟网卡和管理员权限。

第一版流程：

```text
打开玉龙VPN.exe
  ↓
输入动态验证码
  ↓
POST /api/access-code 校验
  ↓
校验成功后进入主界面
  ↓
显示到期时间、公告、节点列表
  ↓
GET /api/clash-config?code=验证码 下载配置
  ↓
启动本地 mihomo / Clash core
  ↓
设置 Windows 系统代理 127.0.0.1:7890
  ↓
一键连接成功
```

---

## 2. 当前功能规划

| 功能 | 当前状态 | 说明 |
|---|---|---|
| 登录动态验证码 | ✅ 已完成 | 登录页只输入密码，成功后进入主界面 |
| 显示到期时间 | ✅ 已完成 | 读取后台 `expiresAt` |
| 显示后台公告 | ✅ 已完成 | 请求 `/api/notices?public=1` |
| 一键连接 | ✅ 已完成 | 下载配置、启动核心、开启系统代理 |
| 一键断开 | ✅ 已完成 | 关闭系统代理、停止核心进程 |
| 节点列表 | ✅ 已完成 | 从 Clash YAML 解析节点并支持切换 |
| 自动选择节点 | ✅ 已完成 | 对接 mihomo 策略组控制接口 |
| 系统代理开关 | ✅ 已完成 | Windows 注册表 + WinINet 即时刷新 |
| 开机自启 | ✅ 已完成 | 使用 Tauri autostart 插件 |
| 托盘图标 | ✅ 已完成 | 支持显示、连接、断开和退出 |
| 自动更新配置 | ✅ 已完成 | 登录后重新拉取 clash-config |
| 软件内状态 | ✅ UI 已写 | 未登录、已登录、连接中、已连接、已断开 |

---

## 3. 技术选型

```text
桌面框架：Tauri 2
前端：React + Vite + TypeScript
后端能力：Rust 命令
代理核心：mihomo / Clash Meta Windows core
打包：GitHub Actions windows-latest
输出：Windows .exe / .msi 安装包
```

为什么选 Tauri：

- 比 Electron 更轻。
- 可以打包成标准 Windows 软件。
- 可以使用 Rust 控制系统代理、进程、托盘、开机自启。
- 适合做品牌化客户端。

---

## 4. 后台接口

Windows 端复用 Android 端后台接口。

### 4.1 登录动态验证码

```http
POST https://api2.smilechat.cn/api/access-code
Content-Type: application/json
```

请求示例：

```json
{
  "code": "888777",
  "clientId": "windows",
  "pluginVersion": "windows-v1.0.1"
}
```

成功返回示例：

```json
{
  "ok": true,
  "expiresAt": "2026-07-30T00:55:00+00:00"
}
```

### 4.2 下载 Clash 配置

```http
GET https://api2.smilechat.cn/api/clash-config?code=验证码
```

### 4.3 读取公告

```http
GET https://api2.smilechat.cn/api/notices?public=1
```

---

## 5. 目录结构

```text
.
├── README.md
├── package.json
├── index.html
├── vite.config.ts
├── tsconfig.json
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   └── style.css
├── src-tauri/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   └── src/
│       ├── main.rs
│       └── lib.rs
├── docs/
│   ├── PRD.md
│   ├── HANDOFF.md
│   └── BUILD_WINDOWS.md
└── .github/workflows/windows-exe.yml
```

---

## 6. 小白怎么运行

### 6.1 安装基础工具

需要安装：

```text
Node.js
Rust
Visual Studio Build Tools
WebView2 Runtime
```

零基础不建议一开始本地编译，建议先用 GitHub Actions 自动打包。

### 6.2 本地开发命令

```bash
npm install
npm run dev
```

### 6.3 本地打包 EXE

```bash
npm run tauri build
```

打包后产物一般在：

```text
src-tauri/target/release/bundle/
```

---

## 7. GitHub Actions 自动打包

进入 GitHub 仓库：

```text
Actions
```

选择：

```text
Build Windows Functional EXE
```

点：

```text
Run workflow
```

成功后在 Releases 长期下载 `YulongVPN-Windows-v1.0.1-Setup.exe`。

---

## 8. 第一版和未来版本区别

### v1：系统代理模式

优点：

- 最容易做。
- 最容易打包。
- 不需要虚拟网卡。
- 适合先给用户测试。

缺点：

- 主要影响走系统代理的软件。
- 少数软件可能不走系统代理。

### v2：TUN 全局模式

优点：

- 更接近手机 VPN 效果。
- 可以接管更多软件流量。

缺点：

- 需要管理员权限。
- 需要 Wintun / 虚拟网卡。
- 打包、杀毒误报、兼容性更复杂。

建议路线：

```text
先完成 v1 系统代理版
  ↓
稳定后再做 v2 TUN 全局版
```

---

## 9. 安全注意事项

不要提交：

```text
真实节点密码
后台管理员密码
Supabase Service Role Key
收费系统密钥
Windows 代码签名证书
```

EXE 第一版可以开源，但真实节点和商业配置必须由后台接口动态下发。

---

## 10. 和 Android 端的关系

Android 端已经证明这套后台流程可行：

```text
动态验证码
公告
到期时间
Clash 配置
品牌 UI
```

Windows 端就是把同一套业务逻辑迁移到电脑端：

```text
Android VpnService
换成
Windows 系统代理 / mihomo core
```

---

## 11. 当前交接说明

这个仓库当前已经完成 Windows v1.0.1 系统代理功能版。后续重点：

```text
1. Windows 10 / 11 真机安装与连接回归
2. 代码签名，减少未知发布者提示
3. 根据实际需求评估 v2 TUN 全局模式
4. 增加版本更新提示
```

---

## 12. 项目口号

```text
玉龙VPN Windows：输入密码，一键连接。
```
