(() => {
  const dateFormat = new Intl.DateTimeFormat("zh-CN", {
    year: "numeric", month: "2-digit", day: "2-digit",
    hour: "2-digit", minute: "2-digit", second: "2-digit", hourCycle: "h23"
  });
  document.querySelectorAll("time[data-account-time]").forEach((node) => {
    const date = new Date(node.dateTime);
    if (!Number.isNaN(date.getTime())) {
      node.textContent = dateFormat.format(date);
      node.title = date.toLocaleString();
    }
  });
  const cells = [...document.querySelectorAll("[data-account-quota]")];
  const loading = new WeakSet();
  const refresh = async (cell, force = false) => {
    if (loading.has(cell)) return;
    loading.add(cell);
    const button = cell.querySelector("[data-quota-refresh]");
    const content = cell.querySelector("[data-quota-content]");
    button.disabled = true;
    button.title = "正在刷新额度";
    cell.setAttribute("aria-busy", "true");
    try {
      const response = await fetch(`${cell.dataset.accountQuota}${force ? "?refresh=true" : ""}`, {
        credentials: "same-origin", cache: "no-store", redirect: "error"
      });
      if (!response.ok) {
        const problem = await response.json().catch(() => null);
        const reason = typeof problem?.error === "string" && problem.error
          ? problem.error : `请求失败（HTTP ${response.status}）`;
        throw new Error(reason);
      }
      content.innerHTML = await response.text();
      window.updateQuotaCountdowns?.();
      content.removeAttribute("title");
      button.title = "刷新套餐额度";
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      const message = `刷新失败：${reason}；点击重试`;
      content.title = message;
      content.querySelectorAll("[title]").forEach((node) => {
        node.title = message;
      });
      button.title = message;
    } finally {
      button.disabled = false;
      cell.setAttribute("aria-busy", "false");
      loading.delete(cell);
    }
  };
  for (const cell of cells) {
    cell.querySelector("[data-quota-refresh]").addEventListener("click", () => void refresh(cell, true));
  }
  const load = async () => {
    while (cells.length) {
      await refresh(cells.shift());
    }
  };
  const workers = Math.min(4, cells.length);
  for (let i = 0; i < workers; i++) void load();

  const wizardDialog = document.getElementById('account-wizard-dialog');
  const wizardContent = wizardDialog?.querySelector('[data-account-wizard-content]');
  const wizardError = document.querySelector('[data-account-wizard-error]');
  const methodLabels = {
    callback: '回调链接',
    device: '设备码',
    refresh_token: 'RT 授权',
  };
  let wizardCleanup = null;
  let wizardBusy = false;

  const bindWizard = (form) => {
    let step = 1;
    let pending = null;
    let stopped = false;
    let timer;
    const panels = [...form.querySelectorAll('[data-wizard-step]')];
    const indicators = [...form.querySelectorAll('[data-wizard-indicator]')];
    const refreshField = form.querySelector('[data-refresh-token-field]');
    const refreshToken = form.elements.refresh_token;
    const callbackUrl = form.elements.callback_url;
    const submit = form.querySelector('[data-wizard-submit]');
    const error = form.querySelector('[data-wizard-error]');
    const status = form.querySelector('[data-wizard-status]');
    const retry = form.querySelector('[data-wizard-poll-retry]');
    // Validate only the visible step, including when Enter submits the form.
    form.noValidate = true;

    const fail = (reason) => {
      error.textContent = reason instanceof Error ? reason.message : String(reason);
      error.hidden = false;
      status.hidden = true;
    };
    const request = async (path, body) => {
      const response = await fetch(path, {
        method: 'POST', credentials: 'same-origin', cache: 'no-store', redirect: 'error',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'Accept': 'application/json' },
        body: new URLSearchParams(body),
      });
      const result = await response.json().catch(() => null);
      if (!response.ok || !result) throw new Error(result?.error || `授权请求失败（HTTP ${response.status}）`);
      return result;
    };
    const setBusy = (busy) => {
      wizardBusy = busy;
      form.setAttribute('aria-busy', String(busy));
      form.querySelectorAll('[data-wizard-submit], [data-wizard-back], [data-account-wizard-close], [data-wizard-poll-retry]')
        .forEach((button) => { button.disabled = busy; });
    };
    const cancelPending = async () => {
      window.clearTimeout(timer);
      if (!pending) return;
      const state = pending.state;
      await request('/admin/oauth/cancel', { csrf: form.elements.csrf.value, state });
      pending = null;
    };
    const finish = (result) => {
      refreshToken.value = '';
      callbackUrl.value = '';
      pending = null;
      stopped = true;
      status.hidden = false;
      status.textContent = '授权成功，账户已保存';
      window.location.assign(result.redirect);
    };
    const poll = async () => {
      if (stopped || !pending || pending.method !== 'device') return;
      retry.hidden = true;
      error.hidden = true;
      setBusy(true);
      try {
        const result = await request('/admin/oauth/device/poll', { csrf: form.elements.csrf.value, state: pending.state });
        if (result.status === 'complete') { finish(result); return; }
        timer = window.setTimeout(poll, Math.max(1, Math.min(900, Number(pending.interval) || 1)) * 1000);
      } catch (reason) {
        fail(reason);
        retry.hidden = false;
      } finally {
        if (!stopped) setBusy(false);
      }
    };

    const showStep = (next) => {
      step = Math.max(1, Math.min(3, next));
      panels.forEach((panel) => { panel.hidden = Number(panel.dataset.wizardStep) !== step; });
      indicators.forEach((indicator) => {
        const current = Number(indicator.dataset.wizardIndicator) === step;
        if (current) indicator.setAttribute('aria-current', 'step');
        else indicator.removeAttribute('aria-current');
      });
      if (step === 3) {
        const method = form.elements.method.value;
        form.querySelector('[data-review-method]').textContent = methodLabels[method] || method;
        form.querySelector('[data-review-runtime]').textContent =
          `${form.elements.os_type.value} ${form.elements.os_version.value} / ${form.elements.arch.value}`;
        form.querySelector('[data-review-proxy]').textContent =
          form.elements.proxy_id.selectedOptions[0]?.textContent || '不使用代理';
        form.querySelector('[data-review-timezone]').textContent =
          form.elements.timezone.value.trim() || '跟随请求';
      }
      form.querySelector('[data-wizard-callback]').hidden = pending?.method !== 'callback';
      form.querySelector('[data-wizard-device]').hidden = pending?.method !== 'device';
      callbackUrl.disabled = pending?.method !== 'callback';
      callbackUrl.required = !callbackUrl.disabled;
      submit.hidden = pending?.method === 'device';
      submit.textContent = pending?.method === 'callback' ? '提交回调并保存' : '开始授权';
      updateMethod();
      const focus = panels[step - 1].querySelector('input:not([disabled]), textarea:not([disabled]), button');
      focus?.focus();
    };

    const validateStep = () => {
      const panel = panels.find((item) => Number(item.dataset.wizardStep) === step);
      for (const control of panel.querySelectorAll('input, select, textarea')) {
        if (!control.disabled && !control.checkValidity()) {
          control.reportValidity();
          return false;
        }
      }
      return true;
    };

    const updateMethod = () => {
      const usesRefreshToken = step === 3 && form.elements.method.value === 'refresh_token';
      refreshField.hidden = !usesRefreshToken;
      refreshToken.disabled = !usesRefreshToken;
      refreshToken.required = usesRefreshToken;
    };

    form.querySelectorAll('[data-wizard-next]').forEach((button) => {
      button.addEventListener('click', () => {
        if (validateStep() && !form.querySelector('[data-apply-proxy-timezone]').disabled) {
          error.hidden = true;
          showStep(step + 1);
        }
      });
    });
    form.querySelectorAll('[data-wizard-back]').forEach((button) => {
      button.addEventListener('click', async () => {
        if (wizardBusy) return;
        setBusy(true);
        try {
          await cancelPending();
          error.hidden = true;
          status.hidden = true;
          showStep(step - 1);
        } catch (reason) { fail(reason); }
        finally { setBusy(false); }
      });
    });
    form.querySelectorAll('[data-account-wizard-close]').forEach((button) => {
      button.addEventListener('click', () => { if (!wizardBusy) wizardDialog.close(); });
    });
    form.querySelectorAll('input[name="method"]').forEach((input) => {
      input.addEventListener('change', updateMethod);
    });
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      if (wizardBusy || !validateStep()) return;
      if (step < 3) {
        if (!form.querySelector('[data-apply-proxy-timezone]').disabled) showStep(step + 1);
        return;
      }
      if (pending?.method === 'device') return;
      error.hidden = true;
      status.hidden = false;
      status.textContent = pending ? '正在授权并保存…' : '正在开始授权…';
      setBusy(true);
      try {
        const result = pending
          ? await request('/admin/oauth/callback', { csrf: form.elements.csrf.value, state: pending.state, callback_url: callbackUrl.value })
          : await request(form.action, new FormData(form));
        if (result.status === 'complete') { finish(result); return; }
        pending = result;
        status.hidden = true;
        if (result.method === 'callback') {
          form.querySelector('[data-wizard-authorize]').href = result.authorize_url;
          form.querySelector('[data-wizard-url]').textContent = result.authorize_url;
        } else {
          form.querySelector('[data-wizard-verification]').href = result.verification_url;
          form.querySelector('[data-wizard-code]').textContent = result.user_code;
          timer = window.setTimeout(poll, Math.max(1, Number(result.interval) || 1) * 1000);
        }
        showStep(3);
      } catch (reason) { fail(reason); }
      finally { if (!stopped) setBusy(false); }
    });
    retry.addEventListener('click', () => void poll());
    wizardCleanup = () => {
      stopped = true;
      window.clearTimeout(timer);
      refreshToken.value = '';
      callbackUrl.value = '';
      if (pending) void request('/admin/oauth/cancel', { csrf: form.elements.csrf.value, state: pending.state }).catch(() => {});
    };
    showStep(1);
    window.bindFingerprintForm?.(form);
    window.bindCopyButtons?.(form);
  };

  document.querySelectorAll('[data-account-wizard-open]').forEach((button) => {
    button.addEventListener('click', async () => {
      button.disabled = true;
      wizardError.hidden = true;
      try {
        const response = await fetch('/admin/oauth/setup', {
          credentials: 'same-origin', cache: 'no-store', redirect: 'follow'
        });
        if (response.redirected && new URL(response.url).pathname === '/admin/login') {
          window.location.assign(response.url);
          return;
        }
        if (!response.ok) throw new Error(`无法生成账户配置（HTTP ${response.status}）`);
        wizardContent.innerHTML = await response.text();
        wizardDialog.showModal();
        bindWizard(wizardContent.querySelector('[data-account-wizard]'));
      } catch (error) {
        wizardError.textContent = error instanceof Error ? error.message : String(error);
        wizardError.hidden = false;
      } finally {
        button.disabled = false;
      }
    });
  });
  wizardDialog?.addEventListener('cancel', (event) => { if (wizardBusy) event.preventDefault(); });
  wizardDialog?.addEventListener('close', () => {
    wizardCleanup?.();
    wizardCleanup = null;
    wizardBusy = false;
    wizardContent.innerHTML = '';
  });
  window.addEventListener('pagehide', () => wizardCleanup?.());
})();
