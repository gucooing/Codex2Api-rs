(() => {
  const start = () => {
    if (document.documentElement.dataset.usageReady) return;
    document.documentElement.dataset.usageReady = "true";
    const dateFormat = new Intl.DateTimeFormat("zh-CN", {
      month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false
    });
    document.querySelectorAll("time[data-local-time]").forEach((node) => {
      const date = new Date(Number(node.dataset.localTime) * 1000);
      if (!Number.isNaN(date.getTime())) {
        node.textContent = `${dateFormat.format(date)} 重置`;
        node.title = date.toLocaleString();
      }
    });
    const updateCountdowns = () => {
      document.querySelectorAll("[data-reset-countdown]").forEach((node) => {
        const minutes = Math.ceil((Number(node.dataset.resetCountdown) * 1000 - Date.now()) / 60000);
        if (!Number.isFinite(minutes)) return;
        if (minutes <= 0) node.textContent = node.dataset.resetElapsed || "已到重置时间，请刷新额度";
        else if (minutes >= 1440) node.textContent = `${Math.floor(minutes / 1440)} 天 ${Math.floor((minutes % 1440) / 60)} 小时后重置`;
        else if (minutes >= 60) node.textContent = `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟后重置`;
        else node.textContent = `${minutes} 分钟后重置`;
      });
    };
    window.updateQuotaCountdowns = updateCountdowns;
    updateCountdowns();
    if (document.querySelector("[data-reset-countdown], [data-account-quota]")) window.setInterval(updateCountdowns, 60000);
    document.querySelectorAll(".usage-chart").forEach((chart) => {
      const tooltip = chart.querySelector(".usage-tooltip");
      const scroll = chart.querySelector(".usage-chart-scroll");
      const hide = () => { tooltip.hidden = true; };
      chart.querySelectorAll("[data-usage-tooltip]").forEach((point) => {
        const show = () => {
          tooltip.textContent = point.dataset.usageTooltip;
          tooltip.hidden = false;
          const bounds = chart.getBoundingClientRect();
          const bar = point.getBoundingClientRect();
          const maximum = Math.max(0, bounds.width - tooltip.offsetWidth);
          tooltip.style.left = `${Math.min(maximum, Math.max(0, bar.left - bounds.left + bar.width / 2 - tooltip.offsetWidth / 2))}px`;
          tooltip.style.top = `${Math.max(0, bar.top - bounds.top - tooltip.offsetHeight - 8)}px`;
        };
        point.addEventListener("pointerenter", show);
        point.addEventListener("pointerleave", hide);
        point.addEventListener("focus", () => {
          point.scrollIntoView({ block: "nearest", inline: "nearest" });
          window.requestAnimationFrame(show);
        });
        point.addEventListener("blur", hide);
      });
      scroll.addEventListener("scroll", hide, { passive: true });
      scroll.scrollLeft = scroll.scrollWidth;
    });
  };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start, { once: true });
  else start();
})();
