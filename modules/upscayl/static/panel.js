// The Upscayl panel. Plain JS, no build step. Every URL is relative ("api/jobs",
// "tools/start_job"), so the page works at /m/upscayl/ hosted and at / standalone.
// Work is submitted as jobs, so a long upscale never holds this page's request open.
(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);
  let models = []; // asset records: {id, name, properties, token}

  function report(id, text) {
    const el = $(id);
    el.textContent = text || "";
    el.hidden = !text;
  }

  async function getJson(url) {
    const res = await fetch(url, { headers: { accept: "application/json" } });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.detail || `${url} returned ${res.status}`);
    return body;
  }

  async function callTool(tool, inputs) {
    const res = await fetch(`tools/${tool}`, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ inputs }),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(body.detail || `${tool} returned ${res.status}`);
    return body.outputs;
  }

  const val = (id) => {
    const el = $(id);
    return el && el.value != null ? String(el.value).trim() : "";
  };
  const num = (id) => Number($(id).value);
  const checked = (id) => {
    const el = $(id);
    return Boolean(el.checked ?? el.hasAttribute("checked"));
  };

  // ---------------------------------------------------------------- the form

  function syncVisibility() {
    const mode = $("mode").value || "image";
    document.querySelectorAll("[data-mode]").forEach((el) => {
      el.hidden = el.dataset.mode !== mode;
    });
    const size = $("size_mode").value || "scale";
    $("scale").hidden = size !== "scale";
    $("width").hidden = size !== "width";
    $("resize").hidden = size !== "resize";
    $("resize_filter").hidden = size === "scale";
    $("compression").toggleAttribute("disabled", (val("format") || "png") !== "webp");
  }

  function showLicense() {
    const m = models.find((x) => x.token === val("model"));
    const el = $("license");
    if (!m) {
      el.textContent = "";
      return;
    }
    const p = m.properties || {};
    const use = p.commercial_use === "forbidden" ? "non-commercial use only"
      : p.commercial_use === "allowed" ? "commercial use allowed" : "commercial use unknown";
    el.textContent = `${p.license || "unknown license"} · ${use}${p.attribution ? " · " + p.attribution : ""}`;
    el.dataset.commercial = p.commercial_use || "unknown";
  }

  function fillModels() {
    // hd-enum builds from its children once, so a fresh element is made with them.
    const old = $("model");
    const fresh = document.createElement("hd-enum");
    fresh.id = "model";
    fresh.setAttribute("label", "Model");
    for (const m of models) {
      const o = document.createElement("option");
      o.value = m.token;
      const p = m.properties || {};
      o.textContent = `${m.name} (${p.license || "unknown"})${p.staged === false ? " - not staged" : ""}`;
      fresh.appendChild(o);
    }
    // Keep the current pick across a refresh (an import refreshes the list).
    const kept = models.find((m) => m.token === val("model"));
    const first = kept || models.find((m) => m.name === "upscayl-standard-4x") || models[0];
    if (first) fresh.setAttribute("value", first.token);
    old.replaceWith(fresh);
    fresh.addEventListener("change", showLicense);
    showLicense();

    const rows = $("models");
    rows.replaceChildren();
    for (const m of models) {
      const p = m.properties || {};
      const row = document.createElement("div");
      row.className = "k-row";
      const label = document.createElement("span");
      label.className = "k-row__label";
      label.textContent = m.name;
      const value = document.createElement("span");
      value.className = "k-row__value";
      value.textContent = [
        p.license || "unknown",
        p.commercial_use === "forbidden" ? "non-commercial" : p.commercial_use || "unknown",
        p.staged === false ? "not staged" : null,
      ].filter(Boolean).join(" · ");
      if (p.attribution) value.setAttribute("tooltip", p.attribution);
      row.append(label, value);
      rows.appendChild(row);
    }
    $("models-skeleton").hidden = true;
  }

  async function loadModels() {
    const list = await getJson("api/assets");
    models = (list.assets || [])
      .filter((a) => a.kind === "ncnn_model")
      .map((a) => ({
        id: a.id,
        name: a.name,
        properties: a.properties || {},
        token: a.id.startsWith("bundled-") ? `upscayl:bundled/${a.properties.model_name}` : `upscayl:imported/${a.id}`,
      }));
    fillModels();
  }

  function inputs() {
    const mode = $("mode").value || "image";
    const size = $("size_mode").value || "scale";
    const i = {
      kind: mode,
      custom_model: val("custom_model") || val("model"),
      format: val("format") || "png",
      compression: num("compression") || 0,
      tta: checked("tta"),
      double_upscayl: checked("double_upscayl"),
      copy_metadata: checked("copy_metadata"),
      overwrite: checked("overwrite"),
      gpu_id: val("gpu_id"),
      tile_size: val("tile_size"),
      threads: val("threads"),
      model_scale: num("model_scale") || 0,
    };
    if (size === "scale") i.scale = num("scale");
    if (size === "width") i.width = num("width");
    if (size === "resize") i.resize = val("resize");
    if (size !== "scale") i.resize_filter = val("resize_filter") || "default";
    if (mode === "image") {
      i.input = val("input");
      i.output = val("output") ? val("output").replace(/[\\/]*$/, "\\") : "";
    } else {
      i.input_folder = val("input_folder");
      i.output_folder = val("output_folder");
    }
    return i;
  }

  async function start() {
    report("error", "");
    const button = $("start");
    button.setAttribute("loading", "");
    try {
      const job = await callTool("start_job", inputs());
      window.hdeck?.post?.("hdeck:report", { severity: "info", message: `Upscayl job ${job.job_id} queued` });
      await loadJobs();
    } catch (err) {
      report("error", err.message || String(err));
    } finally {
      button.removeAttribute("loading");
    }
  }

  // ---------------------------------------------------------------- jobs

  // Each job's row is built once and updated in place on every poll. Rebuilding the list
  // each second would replace the cancel button under the pointer between mousedown and
  // mouseup, and the click would never land.
  function renderJob(li, j) {
    if (!li) {
      li = document.createElement("li");
      li.className = "upscayl-job";
      li.dataset.job = j.job_id;
      const head = document.createElement("div");
      head.className = "upscayl-job__head";
      const name = document.createElement("span");
      name.className = "mono";
      name.textContent = j.job_id;
      const badge = document.createElement("span");
      badge.className = "k-badge";
      head.append(name, badge);
      const detail = document.createElement("div");
      detail.className = "upscayl-job__detail";
      li.append(head, detail);
    }
    const badge = li.querySelector(".k-badge");
    badge.textContent = j.state;
    badge.dataset.tone = j.state === "done" ? "success"
      : j.state === "failed" || j.state === "interrupted" ? "error"
      : j.state === "cancelled" ? "warning" : "accent";
    let bar = li.querySelector("hd-progress");
    if (!j.finished) {
      if (!bar) {
        bar = document.createElement("hd-progress");
        bar.setAttribute("max", "100");
        bar.setAttribute("cancelable", "");
        bar.addEventListener("hd-cancel", () => cancel(j.job_id));
        li.insertBefore(bar, li.querySelector(".upscayl-job__detail"));
      }
      const pct = Math.round(j.progress || 0);
      const set = (k, v) => { if (bar.getAttribute(k) !== v) bar.setAttribute(k, v); };
      if (j.state === "running") set("value", String(pct));
      set("label", j.state === "running" ? `${pct}%` : "queued");
    } else if (bar) {
      bar.remove();
    }
    const parts = [];
    if (j.output) parts.push(j.output);
    else if (j.outputs && j.outputs.length) parts.push(`${j.outputs.length} file(s) written`);
    if (j.failed && j.failed.length) {
      // A cancelled 500-file batch lists 500 reasons; the row names a few, the title all.
      const shown = j.failed.slice(0, 3).join("; ");
      parts.push(`${j.failed.length} failed: ${shown}${j.failed.length > 3 ? "; …" : ""}`);
    }
    if (j.error) parts.push(j.error);
    if (j.finished) parts.push(`${(j.elapsed_ms / 1000).toFixed(1)} s`);
    const all = j.failed && j.failed.length > 3 ? j.failed.join("\n") : "";
    if (li.title !== all) li.title = all;
    const detail = li.querySelector(".upscayl-job__detail");
    const text = parts.join(" · ");
    if (detail.textContent !== text) detail.textContent = text;
    return li;
  }

  let pollTimer = null;
  let pollFailed = false; // only a polling error is cleared by the next good poll
  async function loadJobs() {
    clearTimeout(pollTimer);
    try {
      const { jobs } = await getJson("api/jobs");
      if (pollFailed) {
        pollFailed = false;
        report("error", "");
      }
      $("jobs-skeleton").hidden = true;
      $("jobs-empty").hidden = jobs.length > 0;
      const list = $("jobs");
      const known = new Map([...list.children].map((li) => [li.dataset.job, li]));
      const rows = jobs.slice(0, 20).map((j) => renderJob(known.get(j.job_id) || null, j));
      rows.forEach((li, i) => {
        if (list.children[i] !== li) list.insertBefore(li, list.children[i] || null);
      });
      while (list.children.length > rows.length) list.lastElementChild.remove();
      const active = jobs.some((j) => !j.finished);
      pollTimer = setTimeout(loadJobs, active ? 1000 : 5000);
    } catch (err) {
      pollFailed = true;
      report("error", err.message || String(err));
      pollTimer = setTimeout(loadJobs, 5000);
    }
  }

  async function cancel(id) {
    try {
      await callTool("cancel_job", { job_id: id });
    } catch (err) {
      report("error", err.message || String(err));
    }
    loadJobs();
  }

  // ---------------------------------------------------------------- import

  async function importModel() {
    report("error", "");
    const button = $("imp_go");
    button.setAttribute("loading", "");
    try {
      const out = await callTool("import_model", {
        param: val("imp_param"),
        name: val("imp_name"),
        license: val("imp_license"),
        commercial_use: val("imp_commercial") || "unknown",
        source_url: val("imp_source"),
        attribution: val("imp_attribution"),
      });
      window.hdeck?.post?.("hdeck:report", { severity: "success", message: `Imported ${out.name} as ${out.model}` });
      await loadModels();
    } catch (err) {
      report("error", err.message || String(err));
    } finally {
      button.removeAttribute("loading");
    }
  }

  // ---------------------------------------------------------------- start up

  async function boot() {
    for (const id of ["mode", "size_mode", "format"]) {
      $(id).addEventListener("change", syncVisibility);
      $(id).addEventListener("input", syncVisibility);
    }
    $("start").addEventListener("click", start);
    $("imp_go").addEventListener("click", importModel);
    syncVisibility();
    try {
      const status = await getJson("api/status");
      if (!status.engine_staged) {
        report("setup", `The engine is not staged (${status.engine_path}). Run modules/upscayl/scripts/sync-engine.ps1, then reload.`);
      }
      await loadModels();
      $("page").dataset.status = "ready";
    } catch (err) {
      $("page").dataset.status = "error";
      report("error", err.message || String(err));
    }
    loadJobs();
  }

  boot();
})();
