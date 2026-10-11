(() => {
  const T = window.__TAURI_INTERNALS__;
  const invoke = (name, args) => T.invoke(name, args).catch(() => {});

  const tabsEl = document.getElementById("tabs");
  const urlEl = document.getElementById("url");
  const barEl = document.getElementById("bar");

  if (/Mac/.test(navigator.platform || "")) document.body.classList.add("mac");

  let state = { tabs: [], active: 0 };
  let shownFor = null; // tab id whose address the bar currently shows

  function render(next) {
    state = next;
    const active = state.tabs.find((t) => t.id === state.active);

    tabsEl.textContent = "";
    for (const tab of state.tabs) {
      const el = document.createElement("div");
      el.className = "tab" + (tab.id === state.active ? " active" : "");
      el.dataset.id = tab.id;
      el.setAttribute("role", "tab");
      el.title = tab.title;

      const label = document.createElement("span");
      label.className = "label" + (tab.title ? "" : " empty");
      label.textContent = tab.title || "new tab";

      const close = document.createElement("button");
      close.className = "close";
      close.tabIndex = -1;
      close.setAttribute("aria-label", "close tab");
      close.textContent = "×";

      el.append(label, close);
      tabsEl.append(el);
    }

    document.body.classList.toggle("loading", !!(active && active.loading));

    if (active && (shownFor !== active.id || document.activeElement !== urlEl)) {
      urlEl.value = active.url;
      shownFor = active.id;
    }
  }

  tabsEl.addEventListener("mousedown", (e) => {
    const tab = e.target.closest(".tab");
    if (!tab || e.button !== 0) return;
    if (e.target.closest(".close")) return;
    invoke("switch_tab", { id: Number(tab.dataset.id) });
  });
  tabsEl.addEventListener("click", (e) => {
    const close = e.target.closest(".close");
    const tab = e.target.closest(".tab");
    if (close && tab) invoke("close_tab", { id: Number(tab.dataset.id) });
  });
  tabsEl.addEventListener("auxclick", (e) => {
    const tab = e.target.closest(".tab");
    if (tab && e.button === 1) invoke("close_tab", { id: Number(tab.dataset.id) });
  });

  document.getElementById("plus").addEventListener("click", () => invoke("new_tab"));
  document.getElementById("fill").addEventListener("dblclick", () => invoke("new_tab"));

  for (const btn of document.querySelectorAll("#controls button")) {
    btn.addEventListener("click", () => invoke("window_action", { action: btn.dataset.act }));
  }

  barEl.addEventListener("submit", (e) => {
    e.preventDefault();
    const input = urlEl.value;
    urlEl.blur();
    invoke("navigate", { input });
  });
  urlEl.addEventListener("focus", () => urlEl.select());
  urlEl.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    const active = state.tabs.find((t) => t.id === state.active);
    urlEl.value = active ? active.url : "";
    urlEl.blur();
    invoke("focus_page");
  });

  const listen = window.__TAURI__.event.listen;
  listen("tabs", (e) => render(e.payload));
  listen("focus-url", () => {
    urlEl.focus();
    urlEl.select();
  });

  invoke("get_state").then((s) => {
    if (s) render(s);
    invoke("chrome_ready");
  });
})();
