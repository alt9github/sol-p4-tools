import {
  p4ListWorkspaces,
  p4SetConnection,
  p4ClearConnection,
  p4GetStream,
  p4GetPending,
  p4GetDiff,
  p4CheckStaleRevisions,
  p4CheckConcurrentEdits,
  categorizeP4Error,
  formatP4Error,
} from '@alt9github/sol-p4-tools';

const app = document.getElementById('app')!;

app.innerHTML = `
  <section>
    <h2>P4 Connection</h2>
    <label>Server <input id="in-server" placeholder="admin:1666" /></label>
    <label>User <input id="in-user" placeholder="jonghyun" /></label>
    <label>Client <input id="in-client" placeholder="(auto)" /></label>
    <button id="btn-connect">Connect</button>
    <button id="btn-clear">Clear Override</button>
    <button id="btn-list-ws">List Workspaces</button>
    <pre id="out-connection"></pre>
  </section>

  <section>
    <h2>Stream Detection</h2>
    <label>Data Dir <input id="in-datadir" placeholder="/path/to/MetaData/Data" /></label>
    <button id="btn-stream">Detect Stream</button>
    <pre id="out-stream"></pre>
  </section>

  <section>
    <h2>Pending Changes</h2>
    <label>Pattern <input id="in-pattern" placeholder="//Sol/Dev1Next/MetaData/..." /></label>
    <button id="btn-pending">Get Pending</button>
    <button id="btn-stale">Check Stale</button>
    <button id="btn-concurrent">Check Concurrent</button>
    <pre id="out-pending"></pre>
  </section>

  <section>
    <h2>Diff</h2>
    <label>File Path <input id="in-diff-path" placeholder="/path/to/file.json" /></label>
    <button id="btn-diff">Get Diff</button>
    <pre id="out-diff"></pre>
  </section>

  <section>
    <h2>Error Categorization (client-side test)</h2>
    <label>Raw Error <input id="in-error" placeholder="Connect to server failed; check $P4PORT" /></label>
    <button id="btn-categorize">Categorize</button>
    <pre id="out-error"></pre>
  </section>
`;

const $ = (id: string) => document.getElementById(id) as HTMLInputElement;
const out = (id: string, text: string, cls = '') => {
  const el = $(id);
  el.textContent = text;
  el.className = cls;
};

$('btn-connect').onclick = async () => {
  try {
    await p4SetConnection($('in-server').value, $('in-user').value, $('in-client').value);
    out('out-connection', 'Connected (override set)', 'ok');
  } catch (e) { out('out-connection', formatP4Error(e, '연결'), 'error'); }
};

$('btn-clear').onclick = async () => {
  try {
    await p4ClearConnection();
    out('out-connection', 'Override cleared', 'ok');
  } catch (e) { out('out-connection', String(e), 'error'); }
};

$('btn-list-ws').onclick = async () => {
  try {
    const ws = await p4ListWorkspaces($('in-server').value, $('in-user').value);
    out('out-connection', JSON.stringify(ws, null, 2), 'ok');
  } catch (e) { out('out-connection', formatP4Error(e, '워크스페이스'), 'error'); }
};

$('btn-stream').onclick = async () => {
  try {
    const s = await p4GetStream($('in-datadir').value || undefined);
    out('out-stream', `stream: ${s}`, 'ok');
  } catch (e) { out('out-stream', formatP4Error(e, '스트림'), 'error'); }
};

$('btn-pending').onclick = async () => {
  try {
    const p = await p4GetPending($('in-pattern').value);
    out('out-pending', JSON.stringify(p, null, 2), 'ok');
  } catch (e) { out('out-pending', formatP4Error(e, 'pending'), 'error'); }
};

$('btn-stale').onclick = async () => {
  try {
    const s = await p4CheckStaleRevisions($('in-pattern').value);
    out('out-pending', s.length ? s.join('\n') : '(no stale files)', 'ok');
  } catch (e) { out('out-pending', formatP4Error(e, 'stale'), 'error'); }
};

$('btn-concurrent').onclick = async () => {
  try {
    const c = await p4CheckConcurrentEdits($('in-pattern').value);
    out('out-pending', c.length ? c.join('\n') : '(no concurrent edits)', 'ok');
  } catch (e) { out('out-pending', formatP4Error(e, 'concurrent'), 'error'); }
};

$('btn-diff').onclick = async () => {
  try {
    const d = await p4GetDiff($('in-diff-path').value, 'edit');
    out('out-diff', d.diff || '(no diff)', 'ok');
  } catch (e) { out('out-diff', formatP4Error(e, 'diff'), 'error'); }
};

$('btn-categorize').onclick = () => {
  const raw = $('in-error').value;
  const result = categorizeP4Error(raw);
  out('out-error', JSON.stringify(result, null, 2));
};
