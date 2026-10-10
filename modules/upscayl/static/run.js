// The Upscayl run view: every setting, a drop target, and the results. Work is
// submitted as jobs, so a long upscale never holds this page's request open. A finished
// result opens in the Preview view through the module's server (api/select).
(function () {
  "use strict";

  const { $, report, getJson, callTool, loadModels, licenseLine, select } = window.upscayl;
  let models = [];
  let dataDir = "";

  const val = (id) => {
    const el = $(id);
    return el && el.value != null ? String(el.value).trim() : "";
  };
  const num = (id) => Number($(id).value);
  const checked = (id) => {
    const el = $(id);
    return Boolean(el.checked ?? el.hasAttribute("checked"));
  };
  const setVal = (id, value) => {
    const el = $(id);
    el.value = value;
    el.setAttribute("value", value);
    el.dispatchEvent(new Event("change", { bubbles: true }));
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
    el.textContent = m ? licenseLine(m.properties) : "";
    el.dataset.commercial = m ? m.properties.commercial_use || "unknown" : "unknown";
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
      const p = m.properties;
      o.textContent = `${m.name} (${p.license || "unknown"})${p.staged === false ? " - not staged" : ""}`;
      fresh.appendChild(o);
    }
    // Keep the current pick across a refresh.
    const kept = models.find((m) => m.token === val("model"));
    const first = kept || models.find((m) => m.name === "upscayl-standard-4x") || models[0];
    if (first) fresh.setAttribute("value", first.token);
    old.replaceWith(fresh);
    fresh.addEventListener("change", showLicense);
    showLicense();
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

  // ---------------------------------------------------------------- drop

  // A drop in a HollowDeck view is a browser File, never a path (the shell turns
  // Tauri's drop handler off), so files are uploaded to the module's inbox in slices
  // the module's 16 MB body limit allows, and the form is pointed at the copies.
  const SLICE = 8 * 1024 * 1024;
  const IMAGE = /\.(png|jpe?g|webp)$/i;

  // Every image under the dropped items, folders walked one level deep and more.
  async function droppedFiles(dataTransfer) {
    const entries = [...dataTransfer.items]
      .map((item) => (item.webkitGetAsEntry ? item.webkitGetAsEntry() : null))
      .filter(Boolean);
    if (!entries.length) return [...dataTransfer.files];
    const files = [];
    const walk = async (entry) => {
      if (entry.isFile) {
        files.push(await new Promise((ok, fail) => entry.file(ok, fail)));
      } else if (entry.isDirectory) {
        const reader = entry.createReader();
        for (;;) {
          const batch = await new Promise((ok, fail) => reader.readEntries(ok, fail));
          if (!batch.length) break;
          for (const e of batch) await walk(e);
        }
      }
    };
    for (const e of entries) await walk(e);
    return files;
  }

  async function upload(drop, file, onBytes) {
    let last = null;
    for (let offset = 0; offset === 0 || offset < file.size; offset += SLICE) {
      const q = `drop=${encodeURIComponent(drop)}&name=${encodeURIComponent(file.name)}&offset=${offset}`;
      const res = await fetch(`api/upload?${q}`, {
        method: "POST",
        headers: { "content-type": "application/octet-stream", accept: "application/json" },
        body: file.slice(offset, offset + SLICE),
      });
      last = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(last.detail || `uploading ${file.name} returned ${res.status}`);
      onBytes(Math.min(offset + SLICE, file.size));
      if (file.size === 0) break;
    }
    return last;
  }

  async function takeDrop(dataTransfer) {
    report("error", "");
    const all = await droppedFiles(dataTransfer);
    const files = all.filter((f) => IMAGE.test(f.name));
    const skipped = all.length - files.length;
    if (!files.length) {
      report("error", "Nothing to upscale in that drop: only PNG, JPEG and WebP images are taken.");
      return;
    }
    const names = new Set();
    const clash = files.find((f) => (names.has(f.name) ? true : (names.add(f.name), false)));
    if (clash) {
      report("error", `Two dropped files are both named ${clash.name}; drop them separately.`);
      return;
    }
    const drop = `d${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
    const total = files.reduce((n, f) => n + f.size, 0);
    let done = 0;
    let last = null;
    for (const f of files) {
      const before = done;
      last = await upload(drop, f, (bytes) => {
        done = before + bytes;
        report("dropped", `Copying ${files.length} file(s) into the module… ${Math.round((100 * done) / Math.max(total, 1))}%`);
      });
    }
    // Dropped files have no folder of their own, so results go to the module's outputs
    // folder unless "Save to" already names one.
    const sep = dataDir.includes("\\") ? "\\" : "/";
    const outputs = dataDir ? `${dataDir}${sep}outputs` : "";
    if (files.length === 1) {
      setVal("mode", "image");
      setVal("input", last.path);
      if (!val("output") && outputs) setVal("output", outputs);
    } else {
      setVal("mode", "batch");
      setVal("input_folder", last.folder);
      if (!val("output_folder") && outputs) setVal("output_folder", outputs);
    }
    syncVisibility();
    report("dropped", `${files.length === 1 ? files[0].name : files.length + " images"} ready${skipped ? ` (${skipped} non-image file(s) skipped)` : ""}. Press Upscale.`);
    $("start").focus?.();
  }

  function wireDrop() {
    const overlay = $("drop");
    let depth = 0;
    const hasFiles = (ev) => [...(ev.dataTransfer?.types || [])].includes("Files");
    document.addEventListener("dragenter", (ev) => {
      if (!hasFiles(ev)) return;
      ev.preventDefault();
      depth += 1;
      overlay.hidden = false;
    });
    document.addEventListener("dragover", (ev) => {
      if (!hasFiles(ev)) return;
      ev.preventDefault();
      ev.dataTransfer.dropEffect = "copy";
    });
    document.addEventListener("dragleave", (ev) => {
      if (!hasFiles(ev)) return;
      depth = Math.max(0, depth - 1);
      if (!depth) overlay.hidden = true;
    });
    document.addEventListener("drop", (ev) => {
      if (!hasFiles(ev)) return;
      ev.preventDefault();
      depth = 0;
      overlay.hidden = true;
      takeDrop(ev.dataTransfer).catch((err) => {
        report("dropped", "");
        report("error", err.message || String(err));
      });
    });
  }

  // ---------------------------------------------------------------- results

  // Each row is built once and updated in place on every poll; rebuilding would replace
  // the cancel button under the pointer and the click would never land.
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
    // A finished result with a file opens in Preview.
    let open = li.querySelector(".upscayl-job__open");
    if (j.outputs && j.outputs.length && !open) {
      open = document.createElement("hd-button");
      open.className = "upscayl-job__open";
      open.setAttribute("variant", "ghost");
      open.setAttribute("tooltip", "Show before and after in the Upscayl Preview view");
      open.textContent = "Preview";
      open.addEventListener("click", () => {
        select(j.job_id, 0).catch((err) => report("error", err.message || String(err)));
      });
      li.querySelector(".upscayl-job__head").appendChild(open);
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

  // ---------------------------------------------------------------- start up

  async function boot() {
    for (const id of ["mode", "size_mode", "format"]) {
      $(id).addEventListener("change", syncVisibility);
      $(id).addEventListener("input", syncVisibility);
    }
    $("start").addEventListener("click", start);
    wireDrop();
    syncVisibility();
    // A model imported in the Models view appears here the next time this view is used.
    window.addEventListener("focus", () => {
      loadModels().then((m) => { models = m; fillModels(); }).catch(() => {});
    });
    try {
      const status = await getJson("api/status");
      dataDir = status.data_dir || "";
      if (!status.engine_staged) {
        report("setup", `The engine is not staged (${status.engine_path}). Run modules/upscayl/scripts/sync-engine.ps1, then reload.`);
      }
      models = await loadModels();
      fillModels();
      $("page").dataset.status = "ready";
    } catch (err) {
      $("page").dataset.status = "error";
      report("error", err.message || String(err));
    }
    loadJobs();
  }

  boot();
})();
