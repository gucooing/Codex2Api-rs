(() => {
  const writeClipboard = async (text, button) => {
    if (navigator.clipboard?.writeText) {
      try {
        await navigator.clipboard.writeText(text);
        return;
      } catch (_) {
        // HTTP deployments and denied clipboard permissions need the selection fallback.
      }
    }
    const focused = document.activeElement;
    const selection = window.getSelection();
    const ranges = [];
    if (selection) {
      for (let index = 0; index < selection.rangeCount; index++) {
        ranges.push(selection.getRangeAt(index).cloneRange());
      }
    }
    const inputSelection = focused && typeof focused.selectionStart === 'number'
      ? [focused.selectionStart, focused.selectionEnd, focused.selectionDirection] : null;
    const textarea = document.createElement('textarea');
    textarea.value = text;
    textarea.readOnly = true;
    textarea.style.cssText = 'position:fixed;top:0;left:0;width:1px;height:1px;padding:0;border:0;opacity:0;font-size:16px;';
    // A modal dialog makes elements outside it inert, including a body-level textarea.
    (button.closest('dialog[open]') || document.body).append(textarea);
    try {
      textarea.focus({ preventScroll: true });
      textarea.select();
      textarea.setSelectionRange(0, text.length);
      if (!document.execCommand('copy')) throw new Error('浏览器拒绝复制，请检查剪贴板权限');
    } finally {
      textarea.remove();
      focused?.focus({ preventScroll: true });
      if (inputSelection) focused.setSelectionRange(...inputSelection);
      if (selection) {
        selection.removeAllRanges();
        ranges.forEach((range) => selection.addRange(range));
      }
    }
  };

  window.bindCopyButtons = (root = document) => root.querySelectorAll('[data-copy-target], [data-key-copy]').forEach((button) => {
    if (button.dataset.copyBound === 'true') return;
    button.dataset.copyBound = 'true';
    const originalTitle = button.getAttribute('title');
    const originalAriaLabel = button.getAttribute('aria-label');
    const label = document.createElement('span');
    label.className = 'copy-label';
    label.append(...button.childNodes);
    const feedback = document.createElement('span');
    feedback.className = 'copy-feedback';
    feedback.setAttribute('aria-hidden', 'true');
    feedback.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12 4 4 10-10" pathLength="1"/></svg><span></span>';
    const message = feedback.querySelector('span');
    button.append(label, feedback);
    button.classList.add('copy-button');
    button.setAttribute('aria-live', 'polite');
    let timer;
    const reset = () => {
      button.classList.remove('copy-success', 'copy-error');
      for (const [name, value] of [['title', originalTitle], ['aria-label', originalAriaLabel]]) {
        if (value === null) button.removeAttribute(name);
        else button.setAttribute(name, value);
      }
    };
    button.addEventListener('click', async () => {
      if (button.disabled) return;
      window.clearTimeout(timer);
      reset();
      button.disabled = true;
      button.setAttribute('aria-busy', 'true');
      try {
        let text;
        if (button.dataset.keyCopy) {
          const manager = button.closest('[data-key-manager]');
          const response = await fetch(button.dataset.keyCopy, {
            method: 'POST', credentials: 'same-origin', cache: 'no-store', redirect: 'error',
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            body: new URLSearchParams({ csrf: manager.dataset.keyCsrf })
          });
          const result = await response.json();
          if (!response.ok) throw new Error(result.error || `请求失败（HTTP ${response.status}）`);
          text = result.token;
        } else {
          const target = document.getElementById(button.dataset.copyTarget);
          text = target && ('value' in target ? target.value : target.textContent);
        }
        if (typeof text !== 'string' || !text) throw new Error('没有可复制的内容');
        await writeClipboard(text, button);
        message.textContent = '已复制';
        button.title = '复制成功';
        button.setAttribute('aria-label', '已复制');
        button.classList.add('copy-success');
      } catch (error) {
        message.textContent = '复制失败';
        button.title = error instanceof Error ? error.message : String(error);
        button.setAttribute('aria-label', '复制失败');
        button.classList.add('copy-error');
      } finally {
        button.disabled = false;
        button.removeAttribute('aria-busy');
        timer = window.setTimeout(reset, 1800);
      }
    });
  });
  window.bindCopyButtons();
})();
