(() => {
  const manager = document.querySelector('[data-key-manager]');
  if (!manager) return;
  const createDialog = document.getElementById('create-key-dialog');
  const bindDialog = document.getElementById('bind-key-dialog');
  const bindForm = bindDialog.querySelector('form');
  manager.querySelector('[data-add-key]').addEventListener('click', () => createDialog.showModal());
  manager.querySelectorAll('[data-key-bind]').forEach((button) => {
    button.addEventListener('click', () => {
      bindForm.action = button.dataset.keyBind;
      bindForm.querySelector('[name="account_id"]').value = button.dataset.boundAccount;
      bindForm.querySelector('[data-binding-key-name]').textContent = button.dataset.keyName;
      bindDialog.showModal();
    });
  });
  for (const dialog of [createDialog, bindDialog]) {
    const form = dialog.querySelector('form');
    const submit = form.querySelector('[type="submit"]');
    dialog.querySelector('[data-key-cancel]').addEventListener('click', () => dialog.close());
    dialog.addEventListener('close', () => { form.reset(); submit.disabled = false; });
    form.addEventListener('submit', () => { submit.disabled = true; });
  }
})();
