(() => {
  window.bindFingerprintForm = (form) => {
    if (!form || form.dataset.fingerprintBound === 'true') return;
    form.dataset.fingerprintBound = 'true';
    const proxy = form.elements.proxy_id;
    const timezone = form.elements.timezone;
    const button = form.querySelector('[data-apply-proxy-timezone]');
    const error = form.querySelector('[data-proxy-timezone-error]');
    if (!proxy || !timezone || !button || !error) return;
    let pending = null;
    const reset = () => {
      pending?.abort(); pending = null;
      button.hidden = !proxy.value;
      button.disabled = false;
      button.textContent = '应用代理时区';
      error.hidden = true;
    };
    reset();
    proxy.addEventListener('change', reset);
    timezone.addEventListener('input', reset);
    form.addEventListener('submit', () => pending?.abort());
    button.addEventListener('click', async () => {
      if (!proxy.value || button.disabled) return;
      const proxyId = proxy.value;
      const controller = new AbortController();
      pending = controller;
      button.disabled = true;
      button.textContent = '查询中…';
      error.hidden = true;
      try {
        const response = await fetch(`/admin/proxies/${encodeURIComponent(proxyId)}/timezone`, {
          method: 'POST', signal: controller.signal,
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body: new URLSearchParams({ csrf: form.elements.csrf.value }),
        });
        if (response.redirected) throw new Error('登录已失效，请刷新页面重新登录');
        const result = await response.json();
        if (!response.ok || !result.timezone) throw new Error(result.error || '未获取到代理时区');
        if (pending !== controller || proxy.value !== proxyId) return;
        timezone.value = result.timezone;
        timezone.dispatchEvent(new Event('change', { bubbles: true }));
      } catch (reason) {
        if (reason.name !== 'AbortError' && pending === controller) {
          error.textContent = reason.message || '查询代理时区失败';
          error.hidden = false;
        }
      } finally {
        if (pending === controller) {
          pending = null;
          button.disabled = false;
          button.textContent = '应用代理时区';
        }
      }
    });
  };
  document.querySelectorAll('.fingerprint-form').forEach(window.bindFingerprintForm);
})();
