(() => {
  const formatter = new Intl.DateTimeFormat("zh-CN", {
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false
  });
  document.querySelectorAll("[data-request-time]").forEach(node => {
    const date = new Date(Number(node.dataset.requestTime));
    if (!Number.isNaN(date.getTime())) { node.textContent = formatter.format(date); node.dateTime = date.toISOString(); }
  });
  const form = document.getElementById("usage-filter-form");
  const offset = form.elements.tz_offset;
  const previousOffset = Number(offset.value);
  for (const name of ["from", "until"]) {
    const input = form.elements[name];
    if (!input.value) continue;
    const utc = Date.parse(`${input.value}Z`) + previousOffset * 60000;
    const date = new Date(utc);
    if (!Number.isNaN(date.getTime())) {
      input.value = new Date(utc - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
    }
  }
  form.addEventListener("submit", () => {
    // Submit absolute UTC boundaries, so daylight-saving changes are handled for each date.
    for (const name of ["from", "until"]) {
      const input = form.elements[name];
      if (input.value) input.value = new Date(input.value).toISOString().slice(0, 16);
    }
    offset.value = "0";
  });
})();
