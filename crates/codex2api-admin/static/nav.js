(() => {
  const menu = document.querySelector('[data-admin-menu]');
  if (!menu) return;
  const toggle = menu.querySelector('summary');
  menu.addEventListener('toggle', () => toggle.setAttribute('aria-expanded', String(menu.open)));
  document.addEventListener('click', (event) => {
    if (!menu.contains(event.target)) menu.open = false;
  });
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && menu.open) {
      menu.open = false;
      toggle.focus();
    }
  });
  menu.addEventListener('focusout', (event) => {
    if (!menu.contains(event.relatedTarget)) menu.open = false;
  });
})();
