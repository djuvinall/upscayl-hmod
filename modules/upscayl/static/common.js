// Shared by the Upscayl views. Plain JS, no build step, no globals but `upscayl`.
// Every URL is relative ("api/jobs", "tools/start_job"), so each view works hosted at
// /m/upscayl/ and standalone at /. Views are siblings with no channel to each other in
// the shell, so they share state through this module's server (api/select, api/live).
(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);

  function report(id, text) {
    const el = $(id);
    if (!el) return;
    el.textContent = text || "";
    el.hidden = !text;
  }

  async function getJson(url) {
    const res = await fetch(url, { headers: { accept: "application/json" } });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.detail || `${url} returned ${res.status}`);
    return body;
  }

  async function postJson(url, payload) {
    const res = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify(payload),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.detail || `${url} returned ${res.status}`);
    return body;
  }

  async function callTool(tool, inputs) {
    return (await postJson(`tools/${tool}`, { inputs })).outputs;
  }

  // The model assets as the views use them: {id, name, properties, token}.
  async function loadModels() {
    const list = await getJson("api/assets");
    return (list.assets || [])
      .filter((a) => a.kind === "ncnn_model")
      .map((a) => ({
        id: a.id,
        name: a.name,
        properties: a.properties || {},
        token: a.id.startsWith("bundled-")
          ? `upscayl:bundled/${a.properties.model_name}`
          : `upscayl:imported/${a.id}`,
      }));
  }

  function licenseLine(p) {
    const use = p.commercial_use === "forbidden" ? "non-commercial use only"
      : p.commercial_use === "allowed" ? "commercial use allowed" : "commercial use unknown";
    return `${p.license || "unknown license"} · ${use}${p.attribution ? " · " + p.attribution : ""}`;
  }

  const fileName = (path) => String(path || "").split(/[\\/]/).pop();

  // Show this result in every Preview view.
  const select = (jobId, n) => postJson("api/select", { job_id: jobId, n: n || 0 });

  // Call `onSelect({job_id, n})` now and on every change, from any view. Reconnects on
  // its own (EventSource does); the module keeps running while a view listens.
  function onSelection(onSelect) {
    const source = new EventSource("api/live");
    source.addEventListener("select", (ev) => {
      try {
        const value = JSON.parse(ev.data);
        if (value) onSelect(value);
      } catch (_) { /* a malformed frame is skipped, the next one is whole */ }
    });
    return source;
  }

  window.upscayl = {
    $, report, getJson, postJson, callTool, loadModels, licenseLine, fileName, select, onSelection,
  };
})();
