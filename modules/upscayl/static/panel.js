// The Upscayl panel. Plain JS, no build step. Every fetch URL is relative
// ("api/status"), so the page works at /m/upscayl/ hosted and at / standalone.
(function () {
  "use strict";

  const $ = (id) => document.getElementById(id);

  // panel.css paints k-badge tones: success | warning | error | accent.
  function setStatus(text, tone) {
    const el = $("status");
    el.textContent = text;
    el.dataset.tone = tone;
  }

  function show(key, text) {
    const cell = document.querySelector(`[data-detail="${key}"]`);
    if (cell) cell.textContent = text;
  }

  function showError(message) {
    $("error-text").textContent = message;
    $("error").hidden = false;
    $("skeleton").hidden = true;
    $("panel").dataset.status = "error";
    setStatus("error", "error");
  }

  async function load() {
    $("error").hidden = true;
    $("skeleton").hidden = false;
    $("details").hidden = true;
    $("panel").dataset.status = "loading";
    setStatus("loading", "accent");
    try {
      const res = await fetch("api/status", { headers: { accept: "application/json" } });
      if (!res.ok) throw new Error(`api/status returned ${res.status}`);
      const s = await res.json();
      show("version", s.version);
      show("engine", s.engine_staged
        ? `staged (${s.platform})`
        : `not staged: run the sync script to stage ${s.engine_path}`);
      show("licenses", s.licenses_present ? "present" : "missing licenses/models.json");
      show("data_dir", s.data_dir);
      $("skeleton").hidden = true;
      $("details").hidden = false;
      $("panel").dataset.status = "ready";
      const ready = s.engine_staged && s.licenses_present;
      setStatus(ready ? "ready" : "setup needed", ready ? "success" : "warning");
    } catch (err) {
      showError(err && err.message ? err.message : String(err));
    }
  }

  $("refresh").addEventListener("click", load);
  load();
})();
