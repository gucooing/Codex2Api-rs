(() => {
  document.querySelectorAll('[data-oauth-time]').forEach((element) => {
    const date = new Date(element.dateTime);
    if (!Number.isNaN(date.getTime())) element.textContent = date.toLocaleString('zh-CN', { hour12: false });
  });
  const dialog = document.getElementById('oauth-credential-dialog');
  if (!dialog) return;
  const form = dialog.querySelector('form');
  const submit = form.querySelector('[type="submit"]');
  document.querySelector('[data-oauth-credential-add]').addEventListener('click', () => dialog.showModal());
  dialog.querySelector('[data-oauth-credential-cancel]').addEventListener('click', () => dialog.close());
  dialog.addEventListener('close', () => { form.reset(); submit.disabled = false; });
  form.addEventListener('submit', () => { submit.disabled = true; });
})();
