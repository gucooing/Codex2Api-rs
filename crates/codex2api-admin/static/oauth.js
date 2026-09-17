(() => {
  const dialog = document.getElementById('oauth-method-dialog');
  document.querySelectorAll('[data-oauth-dialog-open]').forEach((button) => {
    button.addEventListener('click', () => dialog.showModal());
  });
  if (dialog) dialog.querySelector('[data-oauth-cancel]').addEventListener('click', () => dialog.close());
  document.querySelectorAll('[data-callback-form]').forEach((form) => {
    form.addEventListener('submit', () => { form.querySelector('[type="submit"]').disabled = true; });
  });
  const device = document.querySelector('[data-device-login]');
  if (!device) return;
  const status = device.querySelector('[data-device-status]');
  let stopped = false;
  let timer;
  window.addEventListener('pagehide', () => { stopped = true; window.clearTimeout(timer); });
  const poll = async () => {
    if (stopped) return;
    try {
      const response = await fetch('/admin/oauth/device/poll', {
        method: 'POST', credentials: 'same-origin', cache: 'no-store', redirect: 'error',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({ csrf: device.dataset.csrf, state: device.dataset.state })
      });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error || `授权查询失败（HTTP ${response.status}）`);
      if (stopped) return;
      if (result.status === 'complete') { window.location.assign(result.redirect); return; }
      timer = window.setTimeout(poll, Math.max(1, Math.min(900, Number(device.dataset.interval) || 1)) * 1000);
    } catch (error) {
      status.textContent = error instanceof Error ? error.message : String(error);
      status.className = 'flash err';
    }
  };
  void poll();
})();
