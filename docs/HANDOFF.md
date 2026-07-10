# 玉龙VPN Windows 项目交接文档

## 1. 当前状态

本仓库已经完成 Tauri + React + Rust 的 Windows v1.0.2 功能版。

已完成：

```text
README
React UI
Tauri 配置
Rust 后端命令
Windows 系统代理命令
开机自启插件权限
托盘图标与快捷连接菜单
GitHub Actions 自动打包
PRD
构建文档
```

## 2. 核心文件

```text
src/App.tsx
= 前端主界面，包含登录页和控制台

src/style.css
= 浅绿色毛玻璃 UI 样式

src-tauri/src/lib.rs
= Rust 后端命令，负责请求后台、保存配置、系统代理、启动核心

src-tauri/tauri.conf.json
= Tauri 应用配置

.github/workflows/windows-exe.yml
= GitHub Actions 自动打包 Windows 安装包
```

## 3. 当前代理核心说明

构建时会从官方 MetaCubeX/mihomo v1.19.28 Release 下载并验证核心，再内置进安装包。

原因：

```text
1. mihomo core 可能比较大
2. 要注意上游协议
3. 不应把商业节点或敏感配置写进仓库
```

安装后程序会自动将内置核心复制到：

```text
%APPDATA%/YulongVPN/bin/mihomo.exe
```

点击“一键连接”时会用：

```text
mihomo.exe -f %APPDATA%/YulongVPN/config.yaml
```

启动代理核心。

## 4. 下一步建议

### 第一步：先跑 GitHub Actions

确认 Windows 安装包能打出来。

### 第二步：下载 EXE 测试 UI

先确认登录页、主界面、公告、到期时间是否符合预期。

### 第三步：测试连接

测试：

```text
动态验证码
配置下载
系统代理开启
浏览器是否走代理
一键断开是否恢复系统代理
```

## 5. 风险点

### 5.1 Windows 系统代理影响范围

系统代理一般影响浏览器、部分软件，但不是所有软件都一定走系统代理。

### 5.2 TUN 全局模式复杂

如果要做全局模式，需要额外处理：

```text
管理员权限
Wintun
虚拟网卡
杀毒误报
驱动兼容性
```

### 5.3 代码签名

正式发布 EXE 时建议做代码签名，否则 Windows 可能提示未知发布者。

## 6. 不要提交的内容

```text
真实节点
节点密码
后台密钥
数据库密钥
代码签名证书
收费系统密钥
```

## 7. 交接给开发者的一句话

这个项目第一版不是要一口气做完整 VPN 驱动，而是先把 Windows 客户端业务闭环跑通：

```text
验证码登录 + 下载配置 + 启动 core + 系统代理 + 一键连接
```
