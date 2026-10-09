// Desktop-site mode can change the UA while retaining a phone-sized viewport.
// Set the layout viewport once before hydration so CSS and matchMedia agree.
export const desktopViewportScript = `
(() => {
  const mobileAgent = /Android|Mobi|iPhone|iPad|iPod|IEMobile|Opera Mini/i.test(navigator.userAgent);
  const smallTouchScreen = navigator.maxTouchPoints > 0 && Math.min(screen.width, screen.height) < 768;
  if (!mobileAgent && smallTouchScreen) {
    const viewport = document.querySelector('meta[name="viewport"]');
    if (viewport) viewport.setAttribute("content", "width=1280");
  }
})();
`;
