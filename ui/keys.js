// Keyboard shortcuts. Shared by the Grove Lite interface pages and injected
// into every web page, so the shortcuts work wherever focus is. It only ever
// asks the app for one of a short list of action names; the app rejects
// anything else.
(() => {
  if (window.__groveKeys) return;
  window.__groveKeys = true;

  const isMac = /Mac|iPhone|iPad/.test(navigator.platform || "");

  function actionFor(e) {
    const k = e.key;
    const mod = e.ctrlKey || e.metaKey;

    if (mod && !e.altKey) {
      if (k === "Tab") return e.shiftKey ? "prev_tab" : "next_tab";
      if (!e.shiftKey) {
        switch (k.toLowerCase()) {
          case "t": return "new_tab";
          case "w": return "close_tab";
          case "l": return "focus_url";
          case "h": return "history";
          case "r": return "reload";
          case "[": return "back";
          case "]": return "forward";
        }
        if (k >= "1" && k <= "8") return "select:" + k;
        if (k === "9") return "select:last";
        if (k === "PageDown") return "next_tab";
        if (k === "PageUp") return "prev_tab";
      }
      return null;
    }
    if (e.altKey && !mod && !e.shiftKey && !isMac) {
      if (k === "ArrowLeft") return "back";
      if (k === "ArrowRight") return "forward";
    }
    if (k === "F5" && !mod && !e.altKey && !e.shiftKey) return "reload";
    return null;
  }

  window.addEventListener("keydown", (e) => {
    const action = actionFor(e);
    if (!action) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    try {
      const internals = window.__TAURI_INTERNALS__;
      if (internals && internals.invoke) internals.invoke("shortcut", { action }).catch(() => {});
    } catch (_) { /* no IPC available, nothing to do */ }
  }, true);
})();
