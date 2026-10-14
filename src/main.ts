import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { periodLabel, resetLabel, percentText, type Snapshot, type QuotaWindow } from './model';
import './style.css';

document.querySelector<HTMLDivElement>('#app')!.innerHTML = `
  <section class="widget" aria-label="Codex 剩余额度">
    <header id="drag-region">
      <div class="brand"><span class="brand-mark" aria-hidden="true">⌘</span><strong>Codex</strong><span class="subtitle">额度概览</span></div>
      <div class="actions">
        <select id="bucket" aria-label="选择额度类型" hidden></select>
        <button id="refresh" title="立即刷新" aria-label="立即刷新"><svg viewBox="0 0 24 24"><path d="M20 7v5h-5M4 17v-5h5M6.1 7a7 7 0 0 1 11.5-1L20 9M4 15l2.4 3A7 7 0 0 0 17.9 17"/></svg></button>
        <button id="hide" title="隐藏到托盘" aria-label="隐藏到托盘"><svg viewBox="0 0 24 24"><path d="M6 12h12"/></svg></button>
      </div>
    </header>
    <div class="quotas">
      <article id="primary" class="quota"><div class="quota-heading"><span class="period"></span><span class="remaining-label">剩余</span></div><div class="number"><strong></strong><span class="unit">%</span><span class="unknown" hidden>暂无数据</span></div><div class="track" role="progressbar"><div class="fill"></div></div><div class="reset"></div></article>
      <article id="secondary" class="quota"><div class="quota-heading"><span class="period"></span><span class="remaining-label">剩余</span></div><div class="number"><strong></strong><span class="unit">%</span><span class="unknown" hidden>暂无数据</span></div><div class="track" role="progressbar"><div class="fill"></div></div><div class="reset"></div></article>
    </div>
    <footer><span id="status-dot"></span><span id="status" role="status">正在连接本机 Codex…</span><button id="choose" title="手动选择 Codex CLI 程序">选择 CLI</button><span id="interval">每 60 秒更新</span></footer>
  </section>`;

let snapshot: Snapshot = { buckets: [], selected: 'codex', updatedAt: null, error: null, refreshing: true };
const select = document.querySelector<HTMLSelectElement>('#bucket')!;
function renderQuota(id: string, quota: QuotaWindow | null | undefined, fallback: string) {
  const root = document.getElementById(id)!;
  const value = quota?.remaining ?? null;
  root.querySelector('.period')!.textContent = periodLabel(quota?.minutes ?? null, fallback);
  root.querySelector('.number strong')!.textContent = percentText(value);
  (root.querySelector('.unit') as HTMLElement).hidden = value == null;
  (root.querySelector('.unknown') as HTMLElement).hidden = value != null;
  const fill = root.querySelector<HTMLElement>('.fill')!;
  fill.style.width = `${value ?? 0}%`;
  root.classList.toggle('low', value != null && value <= 20);
  const bar = root.querySelector<HTMLElement>('.track')!;
  bar.setAttribute('aria-label', `${periodLabel(quota?.minutes ?? null, fallback)}剩余`);
  bar.setAttribute('aria-valuemin', '0'); bar.setAttribute('aria-valuemax', '100');
  if (value == null) bar.removeAttribute('aria-valuenow'); else bar.setAttribute('aria-valuenow', String(value));
  const reset = root.querySelector<HTMLElement>('.reset')!;
  reset.textContent = resetLabel(quota?.resetsAt ?? null);
  reset.title = quota?.resetsAt ? new Date(quota.resetsAt * 1000).toLocaleString('zh-CN') : '';
}
function render() {
  const bucket = snapshot.buckets.find(b => b.id === snapshot.selected) ?? snapshot.buckets[0];
  renderQuota('primary', bucket?.primary, '短周期额度');
  renderQuota('secondary', bucket?.secondary, '长期额度');
  const options = snapshot.buckets.map(b => [b.id, b.name]);
  if (JSON.stringify(options) !== select.dataset.options) {
    select.replaceChildren(...options.map(([id, name]) => { const option = new Option(name, id); return option; }));
    select.dataset.options = JSON.stringify(options);
  }
  select.hidden = options.length < 2;
  select.value = bucket?.id ?? '';
  const refresh = document.querySelector<HTMLButtonElement>('#refresh')!;
  refresh.disabled = snapshot.refreshing;
  refresh.classList.toggle('spinning', snapshot.refreshing);
  const age = snapshot.updatedAt == null ? null : Math.max(0, Math.floor(Date.now() / 1000 - snapshot.updatedAt));
  const stale = !!snapshot.error || (age != null && age > 120);
  const status = document.querySelector<HTMLElement>('#status')!;
  status.textContent = snapshot.error ? `${snapshot.updatedAt ? '数据已过期 · ' : ''}${snapshot.error}`
    : snapshot.refreshing ? '正在更新额度…'
    : stale ? '数据已过期 · 等待更新'
    : age == null ? '暂无额度数据' : `已同步 · ${age < 60 ? '刚刚' : `${Math.floor(age / 60)} 分钟前`}`;
  status.title = status.textContent + (snapshot.updatedAt ? `\n最后同步：${new Date(snapshot.updatedAt * 1000).toLocaleString('zh-CN')}` : '');
  document.querySelector('#status-dot')!.className = stale ? 'stale' : snapshot.refreshing ? 'loading' : '';
  document.querySelector<HTMLElement>('#interval')!.hidden = !!snapshot.error;
}
async function command(name: string, args?: Record<string, unknown>) {
  try { await invoke(name, args); }
  catch { snapshot.error = '操作失败，请通过托盘重试'; snapshot.refreshing = false; render(); }
}
document.querySelector('#refresh')!.addEventListener('click', () => void command('refresh_quota'));
document.querySelector('#hide')!.addEventListener('click', () => void command('hide_widget'));
document.querySelector('#choose')!.addEventListener('click', () => void command('choose_cli'));
select.addEventListener('change', () => void command('select_bucket', { id: select.value }));
document.querySelector('#drag-region')!.addEventListener('pointerdown', event => {
  if ((event as PointerEvent).button === 0 && !(event.target as Element).closest('button, select')) void command('drag_widget');
});
function fit() { document.documentElement.style.setProperty('--scale', String(Math.min(innerWidth / 500, innerHeight / 200))); }
addEventListener('resize', fit); fit(); render();
void (async () => {
  await listen<Snapshot>('quota-updated', event => { snapshot = event.payload; render(); });
  snapshot = await invoke<Snapshot>('get_snapshot'); render();
})().catch(() => { snapshot.error = '无法连接本机后端'; snapshot.refreshing = false; render(); });
setInterval(render, 1000);
