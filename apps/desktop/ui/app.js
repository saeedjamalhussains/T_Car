const { invoke } = window.__TAURI__.core;
const { writeText } = window.__TAURI__.clipboardManager;
const { openUrl } = window.__TAURI__.opener;

const $ = (sel) => document.querySelector(sel);
const el = (tag, props = {}, ...kids) => {
  const n = Object.assign(document.createElement(tag), props);
  kids.flat().forEach((k) => n.append(k));
  return n;
};

// ------------------------------------------------------------------ tabs --
document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t === tab));
    document.querySelectorAll(".view").forEach((v) => {
      v.classList.toggle("hidden", v.id !== `view-${tab.dataset.view}`);
    });
  });
});

// -------------------------------------------------------- result rendering --
function copyButton(text) {
  const b = el("button", { className: "secondary", textContent: "Copy" });
  b.addEventListener("click", async () => {
    await writeText(text);
    b.textContent = "Copied";
    setTimeout(() => (b.textContent = "Copy"), 1200);
  });
  return b;
}

function field(label, value, opts = {}) {
  if (!value) return [];
  const dd = el("dd", {}, String(value));
  if (opts.copy) dd.append(copyButton(String(value)));
  return [el("dt", { textContent: label }), dd];
}

function renderLookup(l) {
  const card = el("div", { className: "card" });
  card.append(el("h2", { textContent: l.national }));
  const where = l.countryName || l.countryIso || `+${l.callingCode}`;
  card.append(el("p", { className: "sub", textContent: `${l.number} · ${where} · ${l.source}` }));

  const badges = el("div", { className: "badges" });
  if (l.wireless === true) badges.append(el("span", { className: "badge wireless", textContent: "wireless" }));
  if (l.wireless === false) badges.append(el("span", { className: "badge landline", textContent: "landline" }));
  badges.append(el("span", {
    className: "badge",
    textContent: l.authoritative ? "per-number lookup" : "prefix-block estimate",
  }));
  card.append(badges);

  card.append(el("dl", {},
    field("Carrier", l.carrierComment || l.carrier),
    field("Carrier key", l.carrier),
    field("Line type", l.lineType),
    field("SMS gateway", l.smsGateway, { copy: true }),
    field("MMS gateway", l.mmsGateway, { copy: true }),
    field("City", l.city),
    field("State", l.state || l.stateCode),
    field("Company", l.company),
    field("Country", l.countryName || l.countryIso),
  ));

  if (l.smsGateway || l.mmsGateway) {
    const send = el("button", { className: "secondary", textContent: "Compose text via gateway" });
    send.addEventListener("click", async () => {
      try {
        const link = await invoke("mailto_link", { lookup: l, message: "", mms: false });
        await openUrl(link);
      } catch (e) {
        send.textContent = String(e);
      }
    });
    card.append(el("div", { className: "row", style: "margin-top:12px" }, send));
  }

  l.notes?.forEach((n) => card.append(el("p", { className: "note", textContent: n })));
  return card;
}

// ---------------------------------------------------------- single lookup --
const numberInput = $("#number");

numberInput.addEventListener("input", async () => {
  const v = numberInput.value.trim();
  if (!v) return ($("#parsed").textContent = "");
  try {
    const p = await invoke("parse_number", { number: v });
    const where = p.countryName || (p.countryIso ? p.countryIso : `+${p.callingCode}`);
    $("#parsed").textContent = `${p.national}  →  ${p.e164}   (${where})`;
    $("#parsed").classList.remove("error");
  } catch (e) {
    $("#parsed").textContent = String(e);
    $("#parsed").classList.add("error");
  }
});

$("#lookup-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const btn = $("#lookup-btn");
  btn.disabled = true;
  btn.textContent = "Looking up…";
  $("#result").replaceChildren();
  try {
    const l = await invoke("lookup", {
      number: numberInput.value.trim(),
      provider: $("#provider").value || null,
    });
    $("#result").append(renderLookup(l));
  } catch (e) {
    $("#result").append(el("div", { className: "card error" },
      el("pre", { textContent: String(e), style: "white-space:pre-wrap;margin:0" })));
  } finally {
    btn.disabled = false;
    btn.textContent = "Look up";
  }
});

// ------------------------------------------------------------------ bulk --
let bulkItems = [];

$("#bulk-btn").addEventListener("click", async () => {
  const btn = $("#bulk-btn");
  btn.disabled = true;
  $("#bulk-status").textContent = "Looking up…";
  try {
    bulkItems = await invoke("lookup_many", { text: $("#bulk-input").value });
    const ok = bulkItems.filter((i) => i.ok).length;
    $("#bulk-status").textContent = `${ok} of ${bulkItems.length} resolved`;
    $("#bulk-csv").disabled = false;
    renderBulk(bulkItems);
  } catch (e) {
    $("#bulk-status").textContent = String(e);
    $("#bulk-result").replaceChildren();
  } finally {
    btn.disabled = false;
  }
});

$("#bulk-csv").addEventListener("click", async () => {
  await writeText(await invoke("to_csv", { items: bulkItems }));
  $("#bulk-status").textContent = "CSV copied to clipboard";
});

function renderBulk(items) {
  const head = ["Number", "Country", "Carrier", "Type", "SMS gateway", "Location"];
  const table = el("table", {}, el("thead", {}, el("tr", {}, head.map((h) => el("th", { textContent: h })))));
  const body = el("tbody");
  items.forEach((it) => {
    const l = it.lookup;
    const row = el("tr", { className: it.ok ? "" : "bad" });
    if (!l) {
      row.append(el("td", { textContent: it.input }), el("td", { colSpan: 5, textContent: it.error || "failed" }));
    } else {
      [l.national,
       l.countryName || l.countryIso || `+${l.callingCode}`,
       l.carrierComment || l.carrier || "—",
       l.wireless === true ? "wireless" : l.wireless === false ? "landline" : l.lineType || "—",
       l.smsGateway || "—",
       [l.city, l.stateCode].filter(Boolean).join(", ") || "—",
      ].forEach((v) => row.append(el("td", { textContent: v })));
    }
    body.append(row);
  });
  table.append(body);
  $("#bulk-result").replaceChildren(table);
}

// ------------------------------------------------------------------ data --
// A plain browser over the embedded tables: pick a table, filter it, drill into
// one area code, copy what you are looking at as CSV.

const dataState = { table: "areas", area: null, rows: [], columns: [] };

const DATA_TABLES = {
  areas: {
    load: () => invoke("data_area_codes"),
    columns: [
      { key: "area", label: "Area code", cls: "num" },
      { key: "state", label: "State" },
      { key: "city", label: "Main city" },
      { key: "prefixes", label: "Exchange blocks", cls: "num" },
      { key: "wirelessBlocks", label: "Wireless blocks", cls: "num" },
    ],
    search: (r, q) =>
      [r.area, r.state, r.stateCode, r.city].some((v) => (v || "").toLowerCase().includes(q)),
    onRow: (r) => showArea(r.area),
  },
  carriers: {
    load: () => invoke("data_carriers"),
    columns: [
      { key: "carrier", label: "Carrier" },
      { key: "name", label: "Key" },
      { key: "sms", label: "SMS gateway" },
      { key: "mms", label: "MMS gateway" },
      { key: "wireless", label: "Wireless", fmt: (v) => (v === null ? "—" : v ? "yes" : "no") },
      { key: "historic", label: "Status", fmt: (v) => (v ? "historic" : "in the wild") },
    ],
    search: (r, q) =>
      [r.carrier, r.name, r.sms, r.mms].some((v) => (v || "").toLowerCase().includes(q)),
  },
  countries: {
    load: () => invoke("countries"),
    columns: [
      { key: "callingCode", label: "Calling code", fmt: (v) => `+${v}`, cls: "num" },
      { key: "iso", label: "ISO" },
      { key: "name", label: "Country" },
    ],
    search: (r, q) =>
      [r.name, r.iso, r.callingCode].some((v) => (v || "").toLowerCase().includes(q)),
  },
};

const PREFIX_COLUMNS = [
  { key: "prefix", label: "Exchange", cls: "num" },
  { key: "city", label: "City" },
  { key: "stateCode", label: "State" },
  { key: "carrier", label: "Carrier" },
  { key: "company", label: "Registered company" },
  { key: "lineType", label: "Type" },
  { key: "wireless", label: "Wireless", fmt: (v) => (v === null ? "—" : v ? "yes" : "no") },
];

function cellText(row, col) {
  const raw = row[col.key];
  const v = col.fmt ? col.fmt(raw) : raw;
  return v === null || v === undefined || v === "" ? "—" : String(v);
}

function renderDataTable(rows, columns, onRow) {
  const table = el("table", {},
    el("thead", {}, el("tr", {}, columns.map((c) => el("th", { textContent: c.label })))));
  const body = el("tbody");
  // Big tables (some area codes hold 800 blocks) render in one pass; the
  // wrapper scrolls rather than the page.
  rows.forEach((r) => {
    const tr = el("tr", { className: onRow ? "clickable" : "" },
      columns.map((c) => el("td", { textContent: cellText(r, c), className: c.cls || "" })));
    if (onRow) tr.addEventListener("click", () => onRow(r));
    body.append(tr);
  });
  table.append(body);
  return el("div", { className: "data-table-wrap" }, table);
}

function dataCsv(rows, columns) {
  const esc = (v) => (/[",\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);
  return [
    columns.map((c) => esc(c.label)).join(","),
    ...rows.map((r) => columns.map((c) => esc(cellText(r, c))).join(",")),
  ].join("\n");
}

function filteredRows() {
  const q = $("#data-search").value.trim().toLowerCase();
  if (!q) return dataState.rows;
  const match = dataState.area
    ? (r) => [r.prefix, r.city, r.carrier, r.company, r.lineType]
        .some((v) => (v || "").toLowerCase().includes(q))
    : DATA_TABLES[dataState.table].search;
  return dataState.rows.filter((r) => match(r, q));
}

function paintData() {
  const rows = filteredRows();
  const out = $("#data-result");
  out.replaceChildren();

  if (dataState.area) {
    const back = el("button", { className: "secondary", textContent: "← All area codes" });
    back.addEventListener("click", () => showTable("areas"));
    out.append(el("div", { className: "crumb" }, back,
      el("strong", { textContent: `Area code ${dataState.area}` })));
  }

  out.append(renderDataTable(rows, dataState.columns, dataState.onRow));
  const total = dataState.rows.length;
  $("#data-status").textContent =
    rows.length === total ? `${total} rows` : `${rows.length} of ${total} rows`;
}

async function showTable(name) {
  dataState.table = name;
  dataState.area = null;
  const spec = DATA_TABLES[name];
  dataState.columns = spec.columns;
  dataState.onRow = spec.onRow || null;
  $("#data-status").textContent = "Loading…";
  dataState.rows = await spec.load();
  document.querySelectorAll("#data-tabs .seg").forEach((b) =>
    b.classList.toggle("active", b.dataset.table === name));
  paintData();
}

async function showArea(area) {
  $("#data-status").textContent = "Loading…";
  dataState.rows = await invoke("data_prefixes", { area });
  dataState.area = area;
  dataState.columns = PREFIX_COLUMNS;
  dataState.onRow = null;
  paintData();
}

document.querySelectorAll("#data-tabs .seg").forEach((b) =>
  b.addEventListener("click", () => showTable(b.dataset.table)));
$("#data-search").addEventListener("input", paintData);
$("#data-csv").addEventListener("click", async () => {
  await writeText(dataCsv(filteredRows(), dataState.columns));
  $("#data-status").textContent = "Copied to clipboard";
});

// Load the tables the first time the Data tab is opened.
let dataLoaded = false;
document.querySelector('.tab[data-view="data"]').addEventListener("click", () => {
  if (dataLoaded) return;
  dataLoaded = true;
  showTable("areas");
});

// ------------------------------------------------------------- providers --
let providers = [];
let config = null;

async function loadProviders() {
  let countries;
  [providers, config, countries] = await Promise.all([
    invoke("providers"),
    invoke("get_config"),
    invoke("countries"),
  ]);

  // "Numbers typed without a + are from ..." picker.
  const cc = $("#default-country");
  cc.replaceChildren();
  countries
    .slice()
    .sort((a, b) => (a.name || "").localeCompare(b.name || ""))
    .forEach((c) =>
      cc.append(el("option", { value: c.callingCode, textContent: `${c.name} (+${c.callingCode})` })));
  cc.value = config.defaultCallingCode;

  const sel = $("#provider");
  sel.replaceChildren(el("option", { value: "", textContent: "Use my chain" }));
  providers.forEach((p) => sel.append(el("option", { value: p.id, textContent: p.label })));

  const list = $("#provider-list");
  list.replaceChildren();
  // Chain members first, in order, then everything else.
  const ordered = [
    ...config.chain.map((id) => providers.find((p) => p.id === id)).filter(Boolean),
    ...providers.filter((p) => !config.chain.includes(p.id)),
  ];

  ordered.forEach((p) => {
    const cred = config.credentials[p.id] || {};
    const check = el("input", { type: "checkbox", checked: config.chain.includes(p.id) });
    check.dataset.id = p.id;

    const HEALTH = {
      ok: ["works", "ok"],
      "needs-account": ["needs an account", "warn"],
      blocked: ["blocked by bot protection", "bad"],
      offline: ["host no longer exists", "bad"],
    };
    const [healthText, healthClass] = HEALTH[p.health] || ["unknown", "warn"];

    const info = el("div", {},
      el("div", { className: "row", style: "gap:8px" },
        el("strong", { textContent: p.label }),
        el("span", { className: `badge ${healthClass}`, textContent: healthText })),
      el("div", { className: "meta", textContent:
        [p.legacy ? "legacy 2014 service" : "bundled data",
         p.authoritative ? "per-number" : "prefix-block",
         p.credentials === "none" ? "no credentials" :
         p.credentials === "api-key" ? "needs API key" : "needs user + secret",
        ].join(" · ") }));
    if (p.note) info.append(el("div", { className: "meta", textContent: p.note }));

    if (p.credentials !== "none") {
      const creds = el("div", { className: "creds" });
      if (p.credentials === "api-key") {
        creds.append(el("input", { type: "password", placeholder: "API key", value: cred.key || "", dataset: { id: p.id, field: "key" } }));
      } else {
        creds.append(
          el("input", { type: "text", placeholder: "user / account SID", value: cred.user || "", dataset: { id: p.id, field: "user" } }),
          el("input", { type: "password", placeholder: "password / auth token", value: cred.pass || "", dataset: { id: p.id, field: "pass" } }));
      }
      info.append(creds);
    }
    if (p.docs) {
      const a = el("button", { className: "secondary", textContent: "Docs", style: "margin-top:8px;padding:2px 8px;font-size:12px" });
      a.addEventListener("click", () => openUrl(p.docs));
      info.append(el("div", {}, a));
    }
    list.append(el("div", { className: "provider" }, check, info));
  });
}

$("#save-providers").addEventListener("click", async () => {
  const chain = [...document.querySelectorAll(".provider input[type=checkbox]")]
    .filter((c) => c.checked)
    .map((c) => c.dataset.id);
  const credentials = { ...config.credentials };
  document.querySelectorAll(".creds input").forEach((i) => {
    const { id, field } = i.dataset;
    credentials[id] = { ...(credentials[id] || {}), [field]: i.value || null };
  });
  config = {
    ...config,
    chain: chain.length ? chain : ["offline"],
    credentials,
    defaultCallingCode: $("#default-country").value || "1",
  };
  try {
    await invoke("set_config", { config });
    $("#providers-status").textContent = "Saved";
    await loadProviders();
  } catch (e) {
    $("#providers-status").textContent = String(e);
  }
  setTimeout(() => ($("#providers-status").textContent = ""), 2500);
});

// ------------------------------------------------------------------ init --
(async function init() {
  await loadProviders();
  const info = await invoke("db_info");
  $("#db-info").replaceChildren(
    el("li", { textContent: `Offline database: ${info.areaCodes} area codes, ${info.prefixBlocks.toLocaleString()} exchange blocks` }),
    el("li", { textContent: `Carrier gateway registry: ${info.carriers} carriers` }),
    el("li", { textContent: `Country calling codes: ${info.countries}` }),
    el("li", { textContent: `Source: ${info.source}` }),
  );
})();
