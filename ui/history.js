(() => {
  const T = window.__TAURI_INTERNALS__;
  const invoke = (name, args) => T.invoke(name, args);

  const qEl = document.getElementById("q");
  const listEl = document.getElementById("list");

  let rows = [];
  let sel = -1;
  let timer = null;
  let seq = 0;

  const pad = (n) => String(n).padStart(2, "0");
  const dayKey = (d) => d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate();

  function dayLabel(d) {
    const now = new Date();
    const yest = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
    if (dayKey(d) === dayKey(now)) return "today";
    if (dayKey(d) === dayKey(yest)) return "yesterday";
    return d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short", year: d.getFullYear() === now.getFullYear() ? undefined : "numeric" });
  }

  function display(url) {
    return url.replace(/^https:\/\//, "").replace(/^([^/?#]*)\/$/, "$1");
  }

  function render(entries) {
    listEl.textContent = "";
    rows = [];
    let lastDay = null;
    for (const e of entries) {
      const d = new Date(e.ts * 1000);
      const key = dayKey(d);
      if (key !== lastDay) {
        lastDay = key;
        const h = document.createElement("div");
        h.className = "day";
        h.textContent = dayLabel(d);
        listEl.append(h);
      }
      const row = document.createElement("div");
      row.className = "row";
      row.setAttribute("role", "option");

      const time = document.createElement("span");
      time.className = "time";
      time.textContent = pad(d.getHours()) + ":" + pad(d.getMinutes());
      const title = document.createElement("span");
      title.className = "title";
      title.textContent = e.title || display(e.url);
      const url = document.createElement("span");
      url.className = "url";
      url.textContent = display(e.url);

      row.append(time, title, url);
      row.addEventListener("click", () => open(e.url));
      listEl.append(row);
      rows.push({ el: row, url: e.url });
    }
    select(rows.length ? 0 : -1, false);
  }

  function select(i, scroll = true) {
    if (sel >= 0 && rows[sel]) rows[sel].el.classList.remove("sel");
    sel = i;
    if (sel >= 0 && rows[sel]) {
      rows[sel].el.classList.add("sel");
      if (scroll) rows[sel].el.scrollIntoView({ block: "nearest" });
    }
  }

  function open(url) {
    invoke("history_open", { url }).catch(() => {});
  }

  async function load() {
    const mine = ++seq;
    try {
      const entries = await invoke("history_list", { query: qEl.value });
      if (mine === seq) render(entries);
    } catch (_) { /* leave the list as it was */ }
  }

  qEl.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(load, 80);
  });

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      invoke("shortcut", { action: "close_tab" }).catch(() => {});
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (rows.length) select(Math.min(sel + 1, rows.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (rows.length) select(Math.max(sel - 1, 0));
    } else if (e.key === "Enter") {
      if (sel >= 0 && rows[sel]) {
        e.preventDefault();
        open(rows[sel].url);
      }
    } else if (e.key === "Delete" && e.shiftKey && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      invoke("history_clear").then(load).catch(() => {});
    }
  });

  qEl.focus();
  load();
})();
