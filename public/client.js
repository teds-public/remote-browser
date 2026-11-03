'use strict';

const token = new URLSearchParams(location.search).get('token') || null;
const socket = io({ reconnection: true, auth: token ? { token } : {} });
const $ = (id) => document.getElementById(id);
const img = $('shot');
const view = $('view');
const coarse = matchMedia('(pointer: coarse)').matches;

let myName = localStorage.getItem('rb-name') || '';
let lastTabId = localStorage.getItem('rb-tab') || null;
let tabs = [];
let activeTabId = null;
let lastObjectUrl = null;
let frames = 0;
let lastFrameAt = 0;
let lastClickAt = 0;
let lastTouchEnd = 0;
let serverViewport = null;

function toast(msg) {
  const el = $('toast');
  el.textContent = msg;
  el.style.display = 'block';
  clearTimeout(toast.t);
  toast.t = setTimeout(() => { el.style.display = 'none'; }, 3000);
}

function setLoading(on) {
  $('loading').classList.toggle('hidden', !on);
}

function askName() {
  $('nameOverlay').classList.remove('hidden');
  $('nameInput').value = myName;
}

function submitName() {
  const v = $('nameInput').value.trim().slice(0, 20);
  if (!v) return;
  myName = v;
  localStorage.setItem('rb-name', myName);
  $('nameOverlay').classList.add('hidden');
  socket.emit('join', { name: myName, quality: streamQuality(), paused: streamPaused() });
}

function streamQuality() {
  return localStorage.getItem('rb-quality') || 'med';
}

function streamPaused() {
  return localStorage.getItem('rb-paused') === '1';
}

function applyStreamUi() {
  $('qualitySel').value = streamQuality();
  $('pauseBtn').textContent = streamPaused() ? 'Resume' : 'Pause';
}

function renderTabs() {
  const bar = $('sidebar');
  bar.innerHTML = '';
  for (const t of tabs) {
    const el = document.createElement('div');
    el.className = 'tab' + (t.id === activeTabId ? ' active' : '');
    const row = document.createElement('div');
    row.className = 'row';
    const title = document.createElement('span');
    title.className = 'title';
    title.textContent = `${t.title || t.url || t.id}${t.crashed ? ' (crashed)' : ''}`;
    title.title = t.url || '';
    const x = document.createElement('span');
    x.className = 'x';
    x.textContent = 'x';
    x.onclick = (e) => {
      e.stopPropagation();
      if (confirm(`Close tab "${t.title || t.url}" (created by ${t.creator || '?'})?`)) {
        socket.emit('tab:close', { tabId: t.id });
      }
    };
    row.appendChild(title);
    row.appendChild(x);
    const sub = document.createElement('span');
    sub.className = 'sub';
    sub.textContent = `${t.creator || '?'} • ${(t.viewers || []).join(', ') || 'no viewers'}`;
    el.appendChild(row);
    el.appendChild(sub);
    el.onclick = () => watchTab(t.id);
    bar.appendChild(el);
  }
  const meta = tabs.find((t) => t.id === activeTabId);
  const hasTabs = tabs.length > 0;
  $('view').classList.toggle('hidden', !hasTabs);
  $('empty').classList.toggle('show', !hasTabs);
  if (meta) {
    const names = (meta.viewers || []).join(', ');
    $('meta').textContent = `Created by ${meta.creator || '?'} • viewing: ${names || '—'}`;
  } else {
    $('meta').textContent = '';
  }
}

function watchTab(tabId) {
  if (activeTabId === tabId) return;
  activeTabId = tabId;
  localStorage.setItem('rb-tab', tabId);
  lastFrameAt = 0;
  $('streamBadge').classList.remove('show');
  setLoading(true);
  setFrame(null);
  socket.emit('tab:watch', { tabId });
  renderTabs();
  if (window.innerWidth <= 700) $('sidebar').classList.remove('open');
}

function setFrame(blob) {
  if (lastObjectUrl) URL.revokeObjectURL(lastObjectUrl);
  lastObjectUrl = null;
  if (!blob) return;
  lastObjectUrl = URL.createObjectURL(blob);
  img.src = lastObjectUrl;
}

function normalizeClient(raw) {
  let url = String(raw || '').trim();
  if (!url) return '';
  if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(url)) url = `https://${url}`;
  return url;
}

function go(url) {
  if (!activeTabId) return toast('No tab yet.');
  const target = normalizeClient(url);
  if (!target) return;
  setLoading(true);
  socket.emit('nav', { tabId: activeTabId, url: target });
}

function toPageCoords(clientX, clientY) {
  if (!serverViewport) return null;
  const rect = img.getBoundingClientRect();
  const vw = serverViewport.width;
  const vh = serverViewport.height;
  if (!vw || !vh || !rect.width || !rect.height) return null;
  const scale = Math.min(rect.width / vw, rect.height / vh);
  const offX = rect.left + (rect.width - vw * scale) / 2;
  const offY = rect.top + (rect.height - vh * scale) / 2;
  return { x: (clientX - offX) / scale, y: (clientY - offY) / scale };
}

function sendClick(x, y, clientX, clientY) {
  if (!activeTabId) return;
  if (clientX !== undefined) showDot(clientX, clientY);
  lastClickAt = performance.now();
  let done = false;
  socket.emit('input:click', { tabId: activeTabId, x: Math.round(x), y: Math.round(y) }, (res) => {
    done = true;
    const ms = Math.round(performance.now() - lastClickAt);
    $('stats').dataset.click = `${ms}ms`;
    if (res && !res.ok) toast(res.message || 'Click failed.');
  });
  setTimeout(() => {
    if (!done) toast('Click did not reach server (over 2s).');
  }, 2000);
}

function showDot(clientX, clientY) {
  const d = document.createElement('div');
  d.className = 'clickdot';
  d.style.left = `${clientX - view.getBoundingClientRect().left}px`;
  d.style.top = `${clientY - view.getBoundingClientRect().top}px`;
  view.appendChild(d);
  setTimeout(() => d.remove(), 1300);
}

view.addEventListener('click', (e) => {
  if (coarse && Date.now() - lastTouchEnd < 600) return;
  const p = toPageCoords(e.clientX, e.clientY);
  if (p) sendClick(p.x, p.y, e.clientX, e.clientY);
});

let single = null;
let pinchD = 0;
let pinchAt = 0;

function dist(a, b) {
  return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
}

view.addEventListener('touchstart', (e) => {
  if (e.touches.length === 2) {
    pinchD = dist(e.touches[0], e.touches[1]);
    single = null;
  } else if (e.touches.length === 1) {
    const t = e.touches[0];
    single = { x: t.clientX, y: t.clientY, at: Date.now(), moved: false, last: 0 };
  }
}, { passive: true });

view.addEventListener('touchmove', (e) => {
  if (!activeTabId) return;
  if (e.touches.length === 2) {
    const d = dist(e.touches[0], e.touches[1]);
    const now = Date.now();
    if (pinchD && now - pinchAt > 80) {
      pinchAt = now;
      socket.emit('input:zoom', { tabId: activeTabId, dy: Math.round((pinchD - d) * 2) });
      pinchD = d;
    }
  } else if (single && e.touches.length === 1) {
    const t = e.touches[0];
    if (Math.abs(t.clientX - single.x) + Math.abs(t.clientY - single.y) > 12) single.moved = true;
    const now = Date.now();
    if (now - single.last > 50) {
      single.last = now;
      socket.emit('input:scroll', {
        tabId: activeTabId,
        dx: Math.round((single.x - t.clientX) * 2),
        dy: Math.round((single.y - t.clientY) * 2),
      });
      single.x = t.clientX;
      single.y = t.clientY;
    }
  }
  e.preventDefault();
}, { passive: false });

view.addEventListener('touchend', (e) => {
  lastTouchEnd = Date.now();
  if (single && !single.moved && Date.now() - single.at < 300 && e.changedTouches.length) {
    const t = e.changedTouches[0];
    const p = toPageCoords(t.clientX, t.clientY);
    if (p) sendClick(p.x, p.y, t.clientX, t.clientY);
  }
  if (e.touches.length === 0) single = null;
  if (e.touches.length < 2) pinchD = 0;
});

$('nameBtn').onclick = submitName;
$('menuBtn').onclick = () => {
  if (window.innerWidth <= 700) $('sidebar').classList.toggle('open');
  else $('sidebar').classList.toggle('hide');
};

function openNewTabPopup() {
  $('newUrlInput').value = '';
  $('urlOverlay').classList.remove('hidden');
  setTimeout(() => $('newUrlInput').focus(), 50);
}

function closeNewTabPopup() {
  $('urlOverlay').classList.add('hidden');
}

function confirmNewTab() {
  const url = normalizeClient($('newUrlInput').value);
  socket.emit('tab:create', { url: url || 'about:blank' });
  closeNewTabPopup();
}

$('newTabBtn').onclick = openNewTabPopup;
$('emptyNewBtn').onclick = openNewTabPopup;
$('urlCancelBtn').onclick = closeNewTabPopup;
$('urlOpenBtn').onclick = confirmNewTab;
$('newUrlInput').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') confirmNewTab();
  if (e.key === 'Escape') closeNewTabPopup();
});
$('backBtn').onclick = () => { if (activeTabId) socket.emit('nav:back', { tabId: activeTabId }); };
$('qualitySel').onchange = () => {
  localStorage.setItem('rb-quality', $('qualitySel').value);
  socket.emit('stream:prefs', { quality: $('qualitySel').value, paused: streamPaused() });
};
$('pauseBtn').onclick = () => {
  const next = !streamPaused();
  localStorage.setItem('rb-paused', next ? '1' : '0');
  applyStreamUi();
  if (next) {
    setFrame(null);
    img.removeAttribute('src');
  } else {
    setLoading(true);
  }
  socket.emit('stream:prefs', { quality: streamQuality(), paused: next });
};
$('reloadBtn').onclick = () => {
  if (!activeTabId) return;
  const tab = tabs.find((t) => t.id === activeTabId);
  setLoading(true);
  socket.emit(tab && tab.crashed ? 'tab:reload' : 'nav:reload', { tabId: activeTabId });
};
$('streamBadge').onclick = () => {
  if (!activeTabId) return;
  $('streamBadge').classList.remove('show');
  setLoading(true);
  socket.emit('shot:refresh', { tabId: activeTabId });
};

socket.on('connect', () => {
  applyStreamUi();
  if (myName) socket.emit('join', { name: myName, quality: streamQuality(), paused: streamPaused() });
  else askName();
});

socket.on('joined', ({ name, viewport }) => {
  myName = name;
  localStorage.setItem('rb-name', name);
  if (viewport && viewport.width && viewport.height) serverViewport = viewport;
});

function applyTabs(list) {
  tabs = list;
  if (!tabs.length) {
    if (activeTabId) {
      activeTabId = null;
      localStorage.removeItem('rb-tab');
      setFrame(null);
      img.removeAttribute('src');
    }
    renderTabs();
    return;
  }
  if (!activeTabId || !tabs.some((t) => t.id === activeTabId)) {
    const fallback = tabs.some((t) => t.id === lastTabId) ? lastTabId : tabs[0].id;
    watchTab(fallback);
    return;
  }
  renderTabs();
}

socket.on('tabs:sync', applyTabs);
socket.on('tabs:update', applyTabs);

socket.on('tab:created', ({ tabId }) => watchTab(tabId));

socket.on('frame', (tabId, blob) => {
  if (tabId !== activeTabId || streamPaused()) return;
  setLoading(false);
  lastFrameAt = Date.now();
  $('streamBadge').classList.remove('show');
  if (blob) {
    setFrame(blob instanceof Blob ? blob : new Blob([blob], { type: 'image/jpeg' }));
    frames += 1;
  }
});

socket.on('shot', ({ tabId, dataUrl }) => {
  if (!tabId || tabId !== activeTabId) return;
  setLoading(false);
  lastFrameAt = Date.now();
  $('streamBadge').classList.remove('show');
  if (dataUrl) img.src = dataUrl;
});

socket.on('error', ({ message }) => toast(message || 'Unknown error.'));

setInterval(() => {
  const click = $('stats').dataset.click ? ` • click ${$('stats').dataset.click}` : '';
  $('stats').textContent = `${streamPaused() ? 'paused' : `${frames}fps`}${click} • ${socket.connected ? 'online' : 'reconnecting...'}`;
  frames = 0;
  if (activeTabId && !streamPaused() && lastFrameAt && Date.now() - lastFrameAt > 3000) {
    $('streamBadge').classList.add('show');
  }
}, 1000);

applyStreamUi();
if (!myName) askName();
