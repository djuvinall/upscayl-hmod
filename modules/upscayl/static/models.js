// The Upscayl Models view: every model with its license, and the import form.
(function () {
  "use strict";

  const { $, report, callTool, loadModels } = window.upscayl;

  const val = (id) => {
    const el = $(id);
    return el && el.value != null ? String(el.value).trim() : "";
  };

  function render(models) {
    const rows = $("models");
    rows.replaceChildren();
    for (const m of models) {
      const p = m.properties;
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

  async function refresh() {
    render(await loadModels());
  }

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
      await refresh();
    } catch (err) {
      report("error", err.message || String(err));
    } finally {
      button.removeAttribute("loading");
    }
  }

  async function boot() {
    $("imp_go").addEventListener("click", importModel);
    try {
      await refresh();
      $("page").dataset.status = "ready";
    } catch (err) {
      $("page").dataset.status = "error";
      report("error", err.message || String(err));
    }
  }

  boot();
})();
