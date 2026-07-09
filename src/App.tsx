import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart';

type AppStatus = 'locked' | 'ready' | 'connecting' | 'connected' | 'disconnected' | 'error';

type LoginResponse = {
  ok: boolean;
  expires_at?: string | null;
  message: string;
};

type NoticeResponse = {
  title: string;
  content: string;
};

type ConnectResponse = {
  ok: boolean;
  status: string;
  nodes: string[];
  message: string;
};

const defaultNotice: NoticeResponse = {
  title: '欢迎使用玉龙VPN Windows',
  content: '输入动态验证码后即可进入主界面，一键连接电脑端代理。',
};

export default function App() {
  const [code, setCode] = useState('');
  const [status, setStatus] = useState<AppStatus>('locked');
  const [message, setMessage] = useState('请输入动态验证码登录');
  const [expiresAt, setExpiresAt] = useState<string>('未登录');
  const [notice, setNotice] = useState<NoticeResponse>(defaultNotice);
  const [nodes, setNodes] = useState<string[]>(['自动选择']);
  const [systemProxy, setSystemProxy] = useState(false);
  const [autoStart, setAutoStart] = useState(false);

  const loggedIn = status !== 'locked';

  const statusText = useMemo(() => {
    switch (status) {
      case 'locked': return '未登录';
      case 'ready': return '已登录，未连接';
      case 'connecting': return '连接中';
      case 'connected': return '已连接';
      case 'disconnected': return '已断开';
      case 'error': return '异常';
      default: return '未知状态';
    }
  }, [status]);

  useEffect(() => {
    isEnabled()
      .then(setAutoStart)
      .catch(() => setAutoStart(false));

    invoke<NoticeResponse>('fetch_notice')
      .then(setNotice)
      .catch(() => setNotice(defaultNotice));
  }, []);

  async function login() {
    const clean = code.trim();
    if (!/^\d{4,12}$/.test(clean)) {
      setMessage('请输入正确的动态验证码');
      return;
    }

    setMessage('正在验证动态验证码...');
    try {
      const res = await invoke<LoginResponse>('login_access_code', { code: clean });
      if (!res.ok) {
        setStatus('locked');
        setMessage(res.message || '密码错误或已过期');
        return;
      }

      setStatus('ready');
      setExpiresAt(res.expires_at || '后台未返回到期时间');
      setMessage('登录成功，可以连接');
    } catch (err) {
      setStatus('error');
      setMessage(`登录失败：${String(err)}`);
    }
  }

  async function connect() {
    if (!code.trim()) return;
    setStatus('connecting');
    setMessage('正在下载配置并启动代理核心...');

    try {
      const res = await invoke<ConnectResponse>('connect_proxy', { code: code.trim() });
      setNodes(res.nodes.length ? res.nodes : ['自动选择']);
      setSystemProxy(true);
      setStatus(res.ok ? 'connected' : 'error');
      setMessage(res.message);
    } catch (err) {
      setStatus('error');
      setMessage(`连接失败：${String(err)}`);
    }
  }

  async function disconnect() {
    setMessage('正在断开连接...');
    try {
      const res = await invoke<ConnectResponse>('disconnect_proxy');
      setSystemProxy(false);
      setStatus('disconnected');
      setMessage(res.message);
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
      setMessage(next ? '系统代理已开启' : '系统代理已关闭');
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

  async function refreshConfig() {
    if (!code.trim()) return;
    setMessage('正在自动更新配置...');
    try {
      const res = await invoke<ConnectResponse>('refresh_config', { code: code.trim() });
      setNodes(res.nodes.length ? res.nodes : ['自动选择']);
      setMessage(res.message);
    } catch (err) {
      setMessage(`更新配置失败：${String(err)}`);
    }
  }

  if (!loggedIn) {
    return (
      <main className="login-shell">
        <section className="login-card glass">
          <div className="brand-logo">玉</div>
          <h1>玉龙VPN Windows</h1>
          <p className="muted">输入动态验证码后进入电脑端控制台</p>

          <input
            className="code-input"
            value={code}
            onChange={(event) => setCode(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && login()}
            placeholder="请输入动态验证码"
            autoFocus
          />

          <button className="primary" onClick={login}>登录</button>
          <p className="message">{message}</p>
        </section>
      </main>
    );
  }

  return (
    <main className="app-shell">
      <aside className="side glass">
        <div className="side-brand">
          <div className="mini-logo">玉</div>
          <div>
            <strong>玉龙VPN</strong>
            <span>Windows Client</span>
          </div>
        </div>

        <nav>
          <button className="nav-active">控制台</button>
          <button>节点列表</button>
          <button>系统代理</button>
          <button>设置</button>
        </nav>
      </aside>

      <section className="content">
        <header className="topbar glass">
          <div>
            <p className="muted">当前状态</p>
            <h2>{statusText}</h2>
          </div>
          <div className={`status-dot ${status}`} />
        </header>

        <section className="grid">
          <div className="card glass hero-card">
            <p className="muted">一键连接</p>
            <h1>{status === 'connected' ? '网络保护中' : '准备连接'}</h1>
            <p>{message}</p>
            <div className="button-row">
              <button className="primary" onClick={connect} disabled={status === 'connecting'}>
                一键连接
              </button>
              <button className="ghost" onClick={disconnect}>一键断开</button>
            </div>
          </div>

          <div className="card glass">
            <p className="muted">到期时间</p>
            <h3>{expiresAt}</h3>
            <p>此时间来自后台动态验证码接口。</p>
          </div>

          <div className="card glass notice-card">
            <p className="muted">后台公告</p>
            <h3>{notice.title}</h3>
            <p>{notice.content}</p>
          </div>

          <div className="card glass">
            <p className="muted">快捷开关</p>
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
            <button className="ghost full" onClick={refreshConfig}>自动更新配置</button>
          </div>

          <div className="card glass node-card">
            <div className="node-head">
              <div>
                <p className="muted">节点列表</p>
                <h3>自动选择节点</h3>
              </div>
              <span>{nodes.length} 个</span>
            </div>
            <ul>
              {nodes.map((node) => (
                <li key={node}>
                  <span>{node}</span>
                  <em>可用</em>
                </li>
              ))}
            </ul>
          </div>
        </section>
      </section>
    </main>
  );
}
