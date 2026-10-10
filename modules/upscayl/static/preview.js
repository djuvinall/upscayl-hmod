// The Upscayl Preview view: one result, before and after, split by a slider. Which
// result it shows is shared through the module's server, so pressing Preview in the
// run view (or picking here) moves every open Preview view.
(function () {
  "use strict";

  const { $, report, getJson, fileName, select, onSelection } = window.upscayl;
  let jobs = []; // finished jobs with at least one output, newest first
  let shown = null; // {job_id, n}

  // ---------------------------------------------------------------- the split

  let split = 50;
  function setSplit(value) {
    split = Math.max(0, Math.min(100, value));
    $("compare").style.setProperty("--split", `${split}%`);
    const h = $("handle");
    h.setAttribute("aria-valuenow", String(Math.round(split)));
    h.setAttribute("aria-valuetext", `${Math.round(split)}% before`);
  }

  function wireSplit() {
    const frame = $("compare");
    let dragging = false;
    const fromPointer = (ev) => {
      const r = frame.getBoundingClientRect();
      if (r.width > 0) setSplit(((ev.clientX - r.left) / r.width) * 100);
    };
    frame.addEventListener("pointerdown", (ev) => {
      if (ev.button !== 0) return;
      dragging = true;
      frame.setPointerCapture(ev.pointerId);
      fromPointer(ev);
      $("handle").focus({ preventScroll: true });
    });
    frame.addEventListener("pointermove", (ev) => { if (dragging) fromPointer(ev); });
    const stop = () => { dragging = false; };
    frame.addEventListener("pointerup", stop);
    frame.addEventListener("pointercancel", stop);
    $("handle").addEventListener("keydown", (ev) => {
      const step = { ArrowLeft: -2, ArrowRight: 2, ArrowDown: -2, ArrowUp: 2, PageDown: -10, PageUp: 10 }[ev.key];
      if (step !== undefined) setSplit(split + step);
      else if (ev.key === "Home") setSplit(0);
      else if (ev.key === "End") setSplit(100);
      else return;
      ev.preventDefault();
    });
  }

  // ---------------------------------------------------------------- pickers

  function rebuildEnum(id, label, options, value, onChange) {
    // hd-enum builds from its children once, so a fresh element is made with them.
    const old = $(id);
    const fresh = document.createElement("hd-enum");
    fresh.id = id;
    fresh.setAttribute("label", label);
    fresh.dataset.testid = id;
    for (const [v, text] of options) {
      const o = document.createElement("option");
      o.value = v;
      o.textContent = text;
      fresh.appendChild(o);
    }
    if (value != null) fresh.setAttribute("value", String(value));
    fresh.hidden = old.hidden;
    old.replaceWith(fresh);
    fresh.addEventListener("change", () => onChange(fresh.value));
    return fresh;
  }

  function jobLabel(j) {
    const what = j.output ? fileName(j.output) : `${j.outputs.length} files`;
    return `${what} · ${j.job_id}`;
  }

  function fillJobs() {
    const value = shown ? shown.job_id : jobs[0] && jobs[0].job_id;
    rebuildEnum("job", "Result", jobs.map((j) => [j.job_id, jobLabel(j)]), value,
      (id) => pick(id, 0));
  }

  function fillFiles(job, n) {
    const many = job.outputs.length > 1;
    const el = rebuildEnum("file", "File", job.outputs.map((o, i) => [String(i), fileName(o)]), n,
      (i) => pick(job.job_id, Number(i)));
    el.hidden = !many;
    $("prev").hidden = !many;
    $("next").hidden = !many;
  }

  // Picking here moves every Preview view, this one included, through the server.
  function pick(jobId, n) {
    select(jobId, n).catch((err) => report("error", err.message || String(err)));
  }

  function step(delta) {
    if (!shown) return;
    const job = jobs.find((j) => j.job_id === shown.job_id);
    if (!job || job.outputs.length < 2) return;
    pick(job.job_id, (shown.n + delta + job.outputs.length) % job.outputs.length);
  }

  // ---------------------------------------------------------------- showing

  async function show(sel) {
    report("error", "");
    let job = jobs.find((j) => j.job_id === sel.job_id);
    if (!job) {
      await loadJobs();
      job = jobs.find((j) => j.job_id === sel.job_id);
    }
    if (!job) {
      report("error", `Result ${sel.job_id} is not a finished job with output.`);
      return;
    }
    shown = { job_id: job.job_id, n: Math.min(sel.n || 0, job.outputs.length - 1) };
    fillJobs();
    fillFiles(job, shown.n);
    $("empty").hidden = true;
    $("stage").hidden = false;
    $("caption").textContent = "Loading…";
    const q = (side) => `api/image?job=${encodeURIComponent(shown.job_id)}&n=${shown.n}&side=${side}`;
    const load = (img, src) => new Promise((ok, fail) => {
      img.onload = () => ok(img);
      img.onerror = () => fail(new Error(`Could not load the ${img.id} image; its file may have moved.`));
      img.src = src;
    });
    try {
      const [after, before] = await Promise.all([load($("after"), q("after")), load($("before"), q("before"))]);
      const out = job.outputs[shown.n];
      $("compare").style.setProperty("--ar", String(after.naturalWidth / Math.max(after.naturalHeight, 1)));
      $("caption").textContent =
        `${before.naturalWidth}×${before.naturalHeight} → ${after.naturalWidth}×${after.naturalHeight} · ${out}`;
    } catch (err) {
      $("caption").textContent = "";
      report("error", err.message || String(err));
    }
  }

  async function loadJobs() {
    const { jobs: all } = await getJson("api/jobs");
    jobs = all.filter((j) => j.finished && j.outputs && j.outputs.length).slice(0, 30);
    if (!shown || jobs.some((j) => j.job_id === shown.job_id)) fillJobs();
  }

  // ---------------------------------------------------------------- start up

  async function boot() {
    wireSplit();
    setSplit(50);
    $("zoom").addEventListener("change", () => { $("stage").dataset.zoom = $("zoom").value || "fit"; });
    $("pixelated").addEventListener("change", () => {
      $("compare").classList.toggle("is-pixelated", Boolean($("pixelated").checked));
    });
    $("prev").addEventListener("click", () => step(-1));
    $("next").addEventListener("click", () => step(1));
    document.addEventListener("keydown", (ev) => {
      if (ev.target.closest?.("input, textarea, [contenteditable]")) return;
      if (ev.key === "[") step(-1);
      else if (ev.key === "]") step(1);
    });
    try {
      await loadJobs();
      const { selection } = await getJson("api/select");
      if (selection) await show(selection);
      else if (jobs[0]) await show({ job_id: jobs[0].job_id, n: 0 });
      $("page").dataset.status = "ready";
    } catch (err) {
      $("page").dataset.status = "error";
      report("error", err.message || String(err));
    }
    onSelection((sel) => {
      if (!shown || sel.job_id !== shown.job_id || sel.n !== shown.n) show(sel);
    });
    setInterval(() => loadJobs().catch(() => {}), 5000);
  }

  boot();
})();
