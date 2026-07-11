# Windows EXE 打包教程

> CI smoke trigger: windows client initial build.

## 1. 最简单方式：GitHub Actions

进入仓库：

```text
https://github.com/3037676975/yulong-vpn-windows
```

点击：

```text
Actions
```

选择：

```text
Build Windows Functional EXE
```

点击：

```text
Run workflow
```

等待绿色成功。

成功后可在 Releases 长期下载：

```text
YulongVPN-Windows-v1.0.4-Setup.exe
```

里面会包含 Windows 安装包。

## 2. 本地打包

需要安装：

```text
Node.js 20+
Rust stable
Visual Studio Build Tools
WebView2 Runtime
```

然后执行：

```bash
npm install
npm run tauri:build
```

输出目录：

```text
src-tauri/target/release/bundle/
```

## 3. 常见失败原因

### 3.1 npm install 失败

检查 Node.js 版本。

### 3.2 Rust 编译失败

检查 Rust 是否安装，Windows 是否安装 Visual Studio Build Tools。

### 3.3 Tauri 打包失败

检查：

```text
src-tauri/tauri.conf.json
src-tauri/Cargo.toml
src-tauri/capabilities/default.json
```

### 3.4 打包成功但没有 EXE

检查 workflow artifact 路径：

```text
src-tauri/target/release/bundle/nsis/*.exe
src-tauri/target/release/bundle/msi/*.msi
```

## 4. 真机测试流程

```text
1. 下载安装包
2. Windows 电脑安装
3. 打开玉龙VPN
4. 输入动态验证码
5. 看公告和到期时间是否显示
6. 点击自动更新配置
7. 点击一键连接
8. 检查系统代理是否变成 127.0.0.1:17890
9. 点击一键断开
10. 检查系统代理是否恢复关闭
```

## 5. mihomo 核心

构建流程固定下载官方 MetaCubeX/mihomo v1.19.28 Windows amd64 版本，验证文件大小和版本后内置到安装包，无需用户手动放置。
