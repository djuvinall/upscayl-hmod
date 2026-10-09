# Glossary

Terms with a narrow meaning in this repo. HollowDeck's own vocabulary (module, tool, asset, envelope, socket) is defined in its `glossary.md` and `INTEROP.md`; it is not repeated here.

**upstream** — `upscayl/upscayl`, the repo this one forks. Also the git remote of that name (push disabled).

**mirror** — the `main` branch: a byte-for-byte copy of `upstream/main`, never committed to.

**hmod** — this fork's work: the `hmod/main` branch, the module, and the docs. Short for "HollowDeck module".

**engine** — `upscayl-bin`, the upscayl-ncnn CLI. Also the module's `engine/` folder, where the sync script stages the binary, the models and exiftool (gitignored).

**sync script** — the module's script that copies engine files from upstream's `resources/` and `models/` into `modules/upscayl/engine/`, refusing any model with no license record.

**bundled model** — a model that ships inside the `.hmod`. Requires a known license.

**imported model** — a model a person adds through `import_model`; lives in the module's data dir.

**model token** — the string a model asset emits and `custom_model` accepts: `upscayl:bundled/<name>` or `upscayl:imported/<id>`. Never a filesystem path.

**license record** — the license fields every model asset carries in `properties` (`docs/arch/model-assets.md`).

**native scale** — the factor a model was trained for (2, 3 or 4), read from its name or given as `-z`. Distinct from **output scale** (`-s`).

**double upscayl** — two engine passes, the second on the first's output; compression, width and TTA apply only to the second.

**TTA** — test-time augmentation (`-x`): slower, sometimes cleaner output.

**job** — an upscale run started by `start_job` that outlives the tool call; tracked under the module's data dir.
