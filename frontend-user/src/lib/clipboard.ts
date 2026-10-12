export async function copyTextElement(element: HTMLElement | null) {
  if (!element) throw new Error("没有可复制的内容");
  const text = element.textContent ?? "";
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      /* HTTP origins can use selection copying. */
    }
  }
  const selection = window.getSelection();
  if (!selection) throw new Error("复制失败，请选中文字后手动复制");
  const previous = Array.from({ length: selection.rangeCount }, (_, index) =>
    selection.getRangeAt(index).cloneRange(),
  );
  const range = document.createRange();
  range.selectNodeContents(element);
  selection.removeAllRanges();
  selection.addRange(range);
  try {
    if (!document.execCommand("copy")) throw new Error("复制失败，请选中文字后手动复制");
  } finally {
    selection.removeAllRanges();
    previous.forEach((saved) => selection.addRange(saved));
  }
}
