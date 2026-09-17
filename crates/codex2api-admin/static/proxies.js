(() => {
  const manager = document.querySelector('[data-proxy-manager]');
  if (!manager) return;
  const dialog = document.getElementById('create-proxy-dialog');
  const form = dialog.querySelector('form');
  const submit = form.querySelector('[type="submit"]');
  const password = form.elements.password;
  const toggle = form.querySelector('[data-toggle-proxy-password]');
  const error = manager.querySelector('[data-proxy-error]');
  const title = document.getElementById('create-proxy-title');
  const formatTimes = () => manager.querySelectorAll('[data-proxy-time]').forEach(node => {
    const date = new Date(node.dateTime);
    if (!Number.isNaN(date.getTime())) node.textContent = date.toLocaleString('zh-CN', { hour12: false });
  });
  formatTimes();
  manager.querySelector('[data-add-proxy]').addEventListener('click', () => {
    form.reset(); form.action = '/admin/proxies';
    form.elements.account_count.value = '0';
    title.textContent = '添加代理'; submit.textContent = '添加';
    dialog.showModal();
  });
  dialog.querySelectorAll('[data-proxy-cancel]').forEach(button => button.addEventListener('click', () => dialog.close()));
  dialog.addEventListener('close', () => {
    form.reset(); submit.disabled = false; password.type = 'password';
    toggle.textContent = '显示'; toggle.setAttribute('aria-label', '显示密码');
  });
  form.addEventListener('submit', () => { submit.disabled = true; });
  toggle.addEventListener('click', () => {
    const show = password.type === 'password';
    password.type = show ? 'text' : 'password';
    toggle.textContent = show ? '隐藏' : '显示';
    toggle.setAttribute('aria-label', show ? '隐藏密码' : '显示密码');
  });
  manager.addEventListener('click', async event => {
    const button = event.target.closest('[data-proxy-check], [data-proxy-edit], [data-proxy-delete]');
    if (!button || button.disabled) return;
    const row = button.closest('[data-proxy-row]');
    const buttons = row.querySelectorAll('[data-proxy-check], [data-proxy-edit], [data-proxy-delete]');
    const label = button.textContent;
    buttons.forEach(node => { node.disabled = true; });
    button.textContent = button.dataset.proxyEdit ? '加载中…' : button.dataset.proxyDelete ? '删除中…' : '检测中…';
    row.setAttribute('aria-busy', 'true');
    error.hidden = true;
    try {
      if (button.dataset.proxyEdit) {
        const response = await fetch(button.dataset.proxyEdit, { cache: 'no-store' });
        if (response.redirected) throw new Error('登录已失效，请刷新页面重新登录');
        const result = await response.json();
        if (!response.ok) throw new Error(result.error || '无法加载代理配置');
        form.reset(); form.action = button.dataset.proxyEdit;
        for (const name of ['name', 'protocol', 'host', 'port', 'username', 'password']) {
          form.elements[name].value = result[name] ?? '';
        }
        form.elements.account_count.value = String(result.account_count ?? 0);
        title.textContent = '编辑代理'; submit.textContent = '保存';
        dialog.showModal();
        return;
      }
      const response = await fetch(button.dataset.proxyDelete || button.dataset.proxyCheck, {
        method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({ csrf: manager.dataset.proxyCsrf }),
      });
      if (response.redirected) throw new Error('登录已失效，请刷新页面重新登录');
      const result = await response.json();
      if (button.dataset.proxyDelete) {
        if (result.confirmation_required) {
          if (!window.confirm(`该代理绑定了 ${result.account_count} 个账户。删除将解除所有绑定，相关账户改为直连。确定删除？`)) return;
          const confirmed = await fetch(button.dataset.proxyDelete, {
            method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            body: new URLSearchParams({ csrf: manager.dataset.proxyCsrf, confirm_unbind: 'true' }),
          });
          if (confirmed.redirected) throw new Error('登录已失效，请刷新页面重新登录');
          const deleted = await confirmed.json();
          if (!confirmed.ok || !deleted.deleted) throw new Error(deleted.error || '删除失败，请重试');
        } else if (!response.ok || !result.deleted) {
          throw new Error(result.error || '删除失败，请重试');
        }
        window.location.reload();
        return;
      }
      if (!response.ok || !result.row) throw new Error(result.error || '检测失败，请重试');
      row.outerHTML = result.row;
      formatTimes();
    } catch (reason) {
      error.textContent = reason.message || '检测失败，请重试'; error.hidden = false;
    } finally {
      buttons.forEach(node => { node.disabled = false; });
      button.textContent = label; row.removeAttribute('aria-busy');
    }
  });
})();
