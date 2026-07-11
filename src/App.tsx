import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
import yulongLogo from '../src-tauri/icons/icon.ico';

type AppStatus = 'checking' | 'locked' | 'ready' | 'connecting' | 'connected' | 'disconnected' | 'error';

type SessionResponse = {
  ok: boolean;
  network_error: boolean;
  expires_at?: string | null;
  message: string;
};

type NoticeResponse = {
  title: string;
  content: string;
  url?: string | null;
};

type BrandingResponse = {
  logo_url?: string | null;
};

type ConnectResponse = {
  ok: boolean;
  status: string;
  nodes: string[];
  current_node?: string | null;
  group?: string | null;
  config_updated_at?: number | null;
  message: string;
};

type NodeSelectionResponse = {
  ok: boolean;
  current_node: string;
  message: string;
};

type AppStateResponse = {
  logged_in: boolean;
  connected: boolean;
  system_proxy: boolean;
  expires_at?: string | null;
  nodes: string[];
  current_node?: string | null;
  group?: string | null;
  config_updated_at?: number | null;
  core_version?: string | null;
};

type SelfCheckResponse = {
  ok: boolean;
  core_ready: boolean;
  backend_ready: boolean;
  core_version?: string | null;
  message: string;
};

const defaultNotice: NoticeResponse = {
  title: '系统公告',
  content: '正在同步玉龙VPN后台公告，请稍候…',
};

function BrandLogo({ compact = false }: { url?: string | null; compact?: boolean }) {
  return (
    <div className={compact ? 'mini-logo' : 'brand-logo'}>
      <img src={yulongLogo} alt="玉龙VPN Logo" />
    </div>
  );
}

function formatExpiry(value?: string | null) {
  if (!value) return '后台未返回到期时间';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('zh-CN', { hour12: false });
}

function formatUpdatedAt(value?: number | null) {
  if (!value) return '尚未更新';
  return new Date(value * 1000).toLocaleString('zh-CN', { hour12: false });
}

export default function App() {
  const [code, setCode] = useState('');
  const [loggedIn, setLoggedIn] = useState(false);
  const [status, setStatus] = useState<AppStatus>('checking');
  const [message, setMessage] = useState('正在检查客户端与后台状态…');
  const [expiresAt, setExpiresAt] = useState<string | null>(null);
  const [notice, setNotice] = useState<NoticeResponse>(defaultNotice);
  const [logoUrl, setLogoUrl] = useState<string | null>(null);
  const [nodes, setNodes] = useState<string[]>([]);
  const [currentNode, setCurrentNode] = useState<string | null>(null);
  const [group, setGroup] = useState<string | null>(null);
  const [systemProxy, setSystemProxy] = useState(false);
  const [autoStart, setAutoStart] = useState(false);
  const [configUpdatedAt, setConfigUpdatedAt] = useState<number | null>(null);
  const [coreVersion, setCoreVersion] = useState<string | null>(null);
  const [selfCheckOk, setSelfCheckOk] = useState(false);
  const [busyNode, setBusyNode] = useState<string | null>(null);
  const nodeCardRef = useRef<HTMLDivElement>(null);
  const [activeView, setActiveView] = useState<'console' | 'nodes'>('console');

  const statusText = useMemo(() => {
    switch (status) {
      case 'checking': return '正在自检';
      case 'locked': return '未登录';
      case 'ready': return '已登录，未连接';
      case 'connecting': return '连接中';
      case 'connected': return '已连接';
      case 'disconnected': return '已断开';
      case 'error': return '连接异常';
      default: return '未知状态';
    }
  }, [status]);

  function applyConnectionResponse(res: ConnectResponse) {
    setNodes(res.nodes || []);
    setCurrentNode(res.current_node || null);
    setGroup(res.group || null);
    setConfigUpdatedAt(res.config_updated_at || null);
    setSystemProxy(res.status === 'connected');
    setStatus(res.status === 'connected' ? 'connected' : 'ready');
    setMessage(res.message);
  }

  function forceLogout(reason: string) {
    setLoggedIn(false);
    setStatus('locked');
    setSystemProxy(false);
    setCurrentNode(null);
    setNodes([]);
    setExpiresAt(null);
    setMessage(reason);
  }

  async function refreshState() {
    try {
      const state = await invoke<AppStateResponse>('get_app_state');
      setSystemProxy(state.system_proxy);
      setNodes(state.nodes || []);
      setCurrentNode(state.current_node || null);
      setGroup(state.group || null);
      setConfigUpdatedAt(state.config_updated_at || null);
      setCoreVersion(state.core_version || null);
      if (state.expires_at) setExpiresAt(state.expires_at);
      if (state.connected) {
        setStatus('connected');
      } else {
        setStatus((old) => old === 'connected' ? 'error' : old);
      }
    } catch {
      // 状态轮询失败不打断当前界面。
    }
  }

  async function refreshConfig(silent = false) {
    if (!silent) setMessage('正在从后台更新配置…');
    try {
      const res = await invoke<ConnectResponse>('refresh_config');
      applyConnectionResponse(res);
      if (silent) setMessage('配置已自动同步');
    } catch (err) {
      const text = String(err);
      if (text.includes('重新登录') || text.includes('已失效')) {
        forceLogout(text);
      } else {
        setStatus('error');
        setMessage(`更新配置失败：${text}`);
      }
    }
  }

  useEffect(() => {
    let cancelled = false;

    async function bootstrap() {
      const [noticeResult, brandingResult, autoStartResult, checkResult] = await Promise.allSettled([
        invoke<NoticeResponse>('fetch_notice'),
        invoke<BrandingResponse>('fetch_branding'),
        isEnabled(),
        invoke<SelfCheckResponse>('self_check'),
      ]);

      if (cancelled) return;
      if (noticeResult.status === 'fulfilled') setNotice(noticeResult.value);
      if (brandingResult.status === 'fulfilled') setLogoUrl(brandingResult.value.logo_url || null);
      if (autoStartResult.status === 'fulfilled') setAutoStart(autoStartResult.value);
      if (checkResult.status === 'fulfilled') {
        setSelfCheckOk(checkResult.value.ok);
        setCoreVersion(checkResult.value.core_version || null);
        if (!checkResult.value.ok) setMessage(checkResult.value.message);
      }

      try {
        const session = await invoke<SessionResponse>('restore_session');
        if (cancelled) return;
        if (session.ok) {
          setLoggedIn(true);
          setExpiresAt(session.expires_at || null);
          setStatus('ready');
          setMessage('登录状态已恢复，正在同步配置…');
          await refreshConfig(true);
        } else {
          setStatus('locked');
          setMessage(session.network_error ? '后台网络异常，暂时无法恢复登录' : '请输入动态密码登录');
        }
      } catch (err) {
        if (!cancelled) {
          setStatus('locked');
          setMessage(`启动检查失败：${String(err)}`);
        }
      }
    }

    bootstrap();
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (!loggedIn) return;

    const authTimer = window.setInterval(async () => {
      try {
        const session = await invoke<SessionResponse>('check_session');
        if (session.ok) {
          setExpiresAt(session.expires_at || null);
          return;
        }
        if (session.network_error) {
          setStatus('error');
          setSystemProxy(false);
          setMessage('网络异常，已暂停代理，恢复网络后会继续验证');
        } else {
          forceLogout('后台密码已修改或账号已失效，请重新登录');
        }
      } catch (err) {
        setStatus('error');
        setMessage(`登录状态检查失败：${String(err)}`);
      }
    }, 30_000);

    const stateTimer = window.setInterval(refreshState, 5_000);
    return () => {
      window.clearInterval(authTimer);
      window.clearInterval(stateTimer);
    };
  }, [loggedIn]);

  async function login() {
    const clean = code.trim();
    if (clean.length < 4) {
      setMessage('请输入正确的动态密码');
      return;
    }

    setStatus('checking');
    setMessage('正在验证动态密码…');
    try {
      const res = await invoke<SessionResponse>('login_access_code', { code: clean });
      if (!res.ok) {
        setStatus('locked');
        setMessage(res.message || '动态密码错误或已过期');
        return;
      }

      setCode('');
      setLoggedIn(true);
      setExpiresAt(res.expires_at || null);
      setStatus('ready');
      setMessage('登录成功，正在同步配置…');
      await refreshConfig(true);
    } catch (err) {
      setStatus('locked');
      setMessage(`登录失败：${String(err)}`);
    }
  }

  async function connect() {
    setStatus('connecting');
    setMessage('正在验证登录、启动 mihomo 并检查本地代理端口…');
    try {
      const res = await invoke<ConnectResponse>('connect_proxy');
      applyConnectionResponse(res);
      setSystemProxy(true);
    } catch (err) {
      const text = String(err);
      if (text.includes('重新登录') || text.includes('已失效')) {
        forceLogout(text);
      } else {
        setSystemProxy(false);
        setStatus('error');
        setMessage(`连接失败：${text}`);
      }
    }
  }

  async function disconnect() {
    if (status === 'ready' || status === 'disconnected') {
      setMessage('当前已经是断开状态');
      return;
    }
    setMessage('正在关闭系统代理和代理核心…');
    try {
      const res = await invoke<ConnectResponse>('disconnect_proxy');
      applyConnectionResponse(res);
      setSystemProxy(false);
      setStatus('disconnected');
    } catch (err) {
      setStatus('error');
      setMessage(`断开失败：${String(err)}`);
    }
  }

  async function toggleSystemProxy() {
    try {
      const next = !systemProxy;
      await invoke('set_system_proxy', { enabled: next });
      setSystemProxy(next);
      setMessage(next ? 'Windows 系统代理已开启' : 'Windows 系统代理已关闭');
    } catch (err) {
      setMessage(`系统代理切换失败：${String(err)}`);
    }
  }

  async function toggleAutoStart() {
    try {
      if (autoStart) {
        await disable();
        setAutoStart(false);
        setMessage('开机自启已关闭');
      } else {
        await enable();
        setAutoStart(true);
        setMessage('开机自启已开启');
      }
    } catch (err) {
      setMessage(`开机自启设置失败：${String(err)}`);
    }
  }

  async function chooseNode(node: string) {
    if (status !== 'connected') {
      setMessage('请先连接后再切换节点');
      return;
    }
    setBusyNode(node);
    setMessage(`正在切换到：${node}…`);
    try {
      const res = await invoke<NodeSelectionResponse>('select_node', { node });
      setCurrentNode(res.current_node);
      setMessage(res.message);
    } catch (err) {
      setMessage(`节点切换失败：${String(err)}`);
    } finally {
      setBusyNode(null);
    }
  }

  async function logout() {
    try {
      await invoke('logout');
    } finally {
      forceLogout('已退出登录，请输入动态密码');
    }
  }

  if (!loggedIn) {
    return (
      <main className="login-shell">
        <section className="login-card glass">
          <BrandLogo url={logoUrl} />
          <h1>玉龙VPN Windows</h1>
          <p className="muted">输入后台动态密码后进入电脑端控制台</p>

          <div className="login-notice">
            <strong>{notice.title}</strong>
            <p>{notice.content}</p>
          </div>

          <input
            className="code-input"
            value={code}
            onChange={(event) => setCode(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && login()}
            placeholder="请输入动态密码"
            type="password"
            inputMode="numeric"
            disabled={status === 'checking'}
            autoFocus
          />

          <button className="primary login-button" onClick={login} disabled={status === 'checking'}>
            {status === 'checking' ? '正在验证…' : '验证并进入'}
          </button>
          <p className="message">{message}</p>
          <div className={selfCheckOk ? 'health healthy' : 'health'}>
            <span />
            {selfCheckOk ? '客户端核心与后台正常' : '正在检查客户端核心'}
          </div>
        </section>
      </main>
    );
  }

  return (
    <main className="app-shell">
      <aside className="side glass">
        <div className="side-brand">
          <BrandLogo url={logoUrl} compact />
          <div>
            <strong>玉龙VPN</strong>
            <span>Windows 1.0.4</span>
          </div>
        </div>

        <nav>
          <button className={activeView === 'console' ? 'nav-active' : ''} onClick={() => {
            setActiveView('console');
            document.querySelector('.topbar')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
            setMessage('已返回连接控制台');
          }}>连接控制台</button>
          <button className={activeView === 'nodes' ? 'nav-active' : ''} onClick={() => {
            setActiveView('nodes');
            nodeCardRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
            setMessage('已定位到节点列表，点击节点名称即可切换');
          }}>节点列表</button>
          <button onClick={async () => {
            await refreshState();
            setMessage('状态已刷新');
          }}>刷新状态</button>
          <button className="logout-nav" onClick={logout}>退出登录</button>
        </nav>

        <div className="side-meta">
          <span>代理核心</span>
          <strong>{coreVersion || '正在检测'}</strong>
        </div>
      </aside>

      <section className="content">
        <header className="topbar glass">
          <div>
            <p className="muted">当前状态</p>
            <h2>{statusText}</h2>
          </div>
          <p className="top-feedback" title={message}>{message}</p>
          <div className="top-actions">
            <span className="current-node-pill">{currentNode || '自动选择'}</span>
            <div className={`status-dot ${status}`} />
          </div>
        </header>

        <section className="grid">
          <div className="card glass hero-card">
            <p className="muted">一键安全连接</p>
            <h1>{status === 'connected' ? '网络保护中' : '准备连接'}</h1>
            <p>{message}</p>
            <div className="button-row">
              <button className="primary" onClick={connect} disabled={status === 'connecting' || status === 'connected'}>
                {status === 'connecting' ? '连接中…' : status === 'connected' ? '已连接' : '一键连接'}
              </button>
              <button className="ghost" onClick={disconnect} disabled={status === 'ready' || status === 'disconnected'}>一键断开</button>
            </div>
          </div>

          <div className="card glass expiry-card">
            <p className="muted">到期时间</p>
            <h3>{formatExpiry(expiresAt)}</h3>
            <p>后台密码变更后，客户端会在 30 秒内自动断开并退回登录页。</p>
          </div>

          <div className="card glass notice-card">
            <p className="muted">后台公告</p>
            <h3>{notice.title}</h3>
            <p>{notice.content}</p>
          </div>

          <div className="card glass">
            <p className="muted">快捷设置</p>
            <div className="switch-line">
              <span>系统代理</span>
              <button className={systemProxy ? 'switch on' : 'switch'} onClick={toggleSystemProxy}>
                {systemProxy ? '已开' : '已关'}
              </button>
            </div>
            <div className="switch-line">
              <span>开机自启</span>
              <button className={autoStart ? 'switch on' : 'switch'} onClick={toggleAutoStart}>
                {autoStart ? '已开' : '已关'}
              </button>
            </div>
            <button className="ghost full" onClick={() => refreshConfig(false)}>立即更新配置</button>
            <p className="config-time">上次更新：{formatUpdatedAt(configUpdatedAt)}</p>
          </div>

          <div className="card glass node-card" ref={nodeCardRef}>
            <div className="node-head">
              <div>
                <p className="muted">节点列表</p>
                <h3>{group || '自动选择节点'}</h3>
              </div>
              <span>{nodes.length} 个</span>
            </div>

            {nodes.length ? (
              <div className="node-grid">
                {nodes.map((node) => {
                  const selected = currentNode === node || (!currentNode && node === '自动选择');
                  return (
                    <button
                      type="button"
                      className={selected ? 'node-option selected' : 'node-option'}
                      key={node}
                      onClick={() => chooseNode(node)}
                      disabled={busyNode !== null}
                    >
                      <span>{node}</span>
                      <em>{busyNode === node ? '切换中' : selected ? '当前' : '选择'}</em>
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="empty-nodes">更新配置后将在这里显示后台节点。</div>
            )}
          </div>
        </section>
      </section>
    </main>
  );
}
