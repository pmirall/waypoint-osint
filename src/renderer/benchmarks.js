'use strict';

// Local benchmark dashboard. This bridge has no GitHub Actions commands.
(function () {
  const el = (id) => document.getElementById(id);
  const rows = [];
  let total = 0;
  let profile = 'coarse';
  const percent = (n) => Number.isFinite(n) ? (n * 100).toFixed(1) + '%' : '—';
  const metres = (n) => {
    if (typeof n !== 'number' || !Number.isFinite(n)) return '—';
    return n >= 1000 ? (n / 1000).toFixed(2) + ' km' : Math.round(n) + ' m';
  };

  function setBenchmarkBusy(busy) {
    state.benchmarkBusy = !!busy;
    el('benchStartBtn').disabled = !!busy || state.busy;
    el('benchCancelBtn').classList.toggle('hidden', !busy);
    el('benchPreset').disabled = !!busy;
    el('benchProfile').disabled = !!busy;
    el('benchTimeout').disabled = !!busy;
    el('benchCache').disabled = !!busy;
    refreshRunButtons();
  }

  function renderRows() {
    const body = el('benchRows');
    body.replaceChildren();
    if (!rows.length) {
      const tr = document.createElement('tr');
      const td = document.createElement('td');
      td.colSpan = 6;
      td.className = 'bench-empty';
      td.textContent = 'No results yet.';
      tr.appendChild(td);
      body.appendChild(tr);
      return;
    }
    for (const r of rows) {
      const tr = document.createElement('tr');
      const fields = [r.id || '', r.country || '', r.status || 'unknown',
        r.method || '—', metres(r.error_m), r.duration_s == null ? '—' : Number(r.duration_s).toFixed(1) + ' s'];
      for (let i = 0; i < fields.length; i++) {
        const td = document.createElement('td');
        td.textContent = fields[i];
        if (i === 2 && ['complete', 'timeout', 'error', 'cancelled'].includes(r.status)) {
          td.className = 'bench-status-' + r.status;
        }
        tr.appendChild(td);
      }
      body.appendChild(tr);
    }
  }

  function renderMetrics(n) {
    const completed = rows.filter(r => r.status === 'complete');
    const errors = completed.map(r => r.error_m).filter(Number.isFinite).sort((a, b) => a - b);
    const rate = threshold => n ? percent(errors.filter(e => e <= threshold).length / n) : '—';
    el('benchCoverage').textContent = n ? percent(completed.length / n) : '—';
    el('bench10').textContent = rate(10);
    el('bench100').textContent = rate(100);
    el('bench1k').textContent = rate(1000);
    el('benchMedian').textContent = errors.length ? metres(errors[Math.floor(errors.length / 2)]) : '—';
    el('benchCounter').textContent = rows.length + ' / ' + n;
    el('benchMeterFill').style.width = n ? Math.min(100, rows.length / n * 100) + '%' : '0%';
    el('benchMetricNote').textContent = profile === 'coarse'
      ? 'Coarse mode measures distance to the PLONK cluster centre. It does NOT measure precise camera position.'
      : 'Street-level mode measures the selected reference coordinate or coarse fallback. The target camera may be elsewhere.';
  }

  function renderReport(report) {
    if (!report) return;
    rows.length = 0;
    rows.push(...(Array.isArray(report.rows) ? report.rows : []));
    total = Number(report.metrics?.total) || rows.length;
    profile = report.profile || 'coarse';
    renderMetrics(total);
    renderRows();
    const processed = Number(report.metrics?.processed) || rows.length;
    const complete = Number(report.metrics?.completed) || 0;
    el('benchStatusText').textContent = report.finished
      ? 'Completed: ' + complete + ' successful of ' + total + ' photos. ' + (processed - complete) + ' failures or timeouts.'
      : 'Partial report: ' + processed + ' of ' + total + ' photographs.';
  }

  async function restore() {
    try {
      const status = await window.api.benchmarkStatus();
      el('benchReportPath').textContent = status.report_dir || '';
      if (status.last_report) renderReport(status.last_report);
      setBenchmarkBusy(!!status.running);
      if (status.running) el('benchStatusText').textContent = 'Local benchmark is running.';
    } catch (e) {
      el('benchStatusText').textContent = 'Could not load local reports: ' + String(e);
    }
  }

  el('benchStartBtn').addEventListener('click', async () => {
    if (state.busy || state.benchmarkBusy) return;
    const preset = el('benchPreset').value;
    profile = el('benchProfile').value;
    const count = { smoke: 4, pilot: 50, full: 500 }[preset];
    if (!count) return;
    if (count >= 50 && profile === 'precision' &&
        !window.confirm('This performs up to ' + count + ' street-image searches on your PC and can take a long time. Start?')) return;
    rows.length = 0;
    total = count;
    renderRows();
    renderMetrics(total);
    el('benchStatusText').textContent = 'Preparing local models and dataset…';
    setBenchmarkBusy(true);
    try {
      await window.api.benchmarkStart({
        preset, profile, maxSeconds: Number(el('benchTimeout').value),
        useCache: el('benchCache').checked,
      });
    } catch (e) {
      setBenchmarkBusy(false);
      el('benchStatusText').textContent = 'Could not start: ' + String(e);
    }
  });

  el('benchCancelBtn').addEventListener('click', async () => {
    el('benchStatusText').textContent = 'Stopping after the current model/network operation…';
    el('benchCancelBtn').disabled = true;
    try { await window.api.benchmarkCancel(); }
    catch (e) { el('benchStatusText').textContent = String(e); }
  });

  el('benchLatestBtn').addEventListener('click', restore);

  window.api.onBenchmarkEvent(p => {
    switch (p.event) {
      case 'status':
        el('benchStatusText').textContent = p.message || '';
        break;
      case 'photo':
        el('benchStatusText').textContent = 'Image ' + p.index + '/' + p.total + ': ' + p.id + ', ' + p.country + ' · ' + p.message;
        break;
      case 'stage':
        if (p.step === 'stage_start') el('benchStatusText').textContent = 'Running ' + p.stage + ' on this PC…';
        break;
      case 'result':
        if (p.row) {
          rows.push(p.row);
          renderMetrics(p.total || total);
          renderRows();
        }
        break;
      case 'finished':
        if (p.report) renderReport(p.report);
        if (p.report_path) el('benchReportPath').textContent = p.report_path;
        setBenchmarkBusy(false);
        el('benchCancelBtn').disabled = false;
        break;
      case 'failed':
        el('benchStatusText').textContent = 'Benchmark failed: ' + p.error;
        setBenchmarkBusy(false);
        el('benchCancelBtn').disabled = false;
        break;
    }
  });

  restore();
})();