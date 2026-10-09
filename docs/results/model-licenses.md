# Model licenses — the ten bundled NCNN models

Issue #4. Researched 2026-10-08 from primary sources: upstream's git history (rename commits, matching file hashes against `upscayl/custom-models`), the authors' own READMEs, Real-ESRGAN's `LICENSE` and model zoo, and OpenModelDB. The machine-readable record is `modules/upscayl/licenses/models.json`; license texts sit beside it.

No model is `unknown`. Two are **medium** confidence: see the notes column.

| Model | Original | Author | License | Commercial use | Confidence | Evidence |
|---|---|---|---|---|---|---|
| upscayl-standard-4x | RealESRGAN_x4plus | Xintao Wang | BSD-3-Clause | allowed | high | [rename 6cfaf45](https://github.com/upscayl/upscayl/commit/6cfaf45), [Real-ESRGAN LICENSE](https://github.com/xinntao/Real-ESRGAN/blob/master/LICENSE) |
| upscayl-lite-4x | realesr-general-x4v3 | Xintao Wang | BSD-3-Clause | allowed | high | [rename 801755c](https://github.com/upscayl/upscayl/commit/801755c), [custom-models README](https://github.com/upscayl/custom-models/blob/main/README.md) |
| high-fidelity-4x | 4xHFA2k | Helaman (Phhofm) | CC-BY-4.0 | allowed, **attribution required** | high | [author README](https://github.com/Phhofm/models/blob/main/4xHFA2k/README.md) |
| remacri-4x | 4x_foolhardy_Remacri | FoolhardyVEVO | CC-BY-NC-SA-4.0 | **forbidden** | medium | [OpenModelDB](https://openmodeldb.info/models/4x-Remacri), upstream label "Non-Commercial" |
| ultramix-balanced-4x | 4x-UltraMix_Balanced | Kim2091 | CC-BY-NC-SA-4.0 | **forbidden** | medium | [author repo](https://huggingface.co/Kim2091/UltraSharp) |
| ultrasharp-4x | 4x-UltraSharp v1 | Kim2091 | CC-BY-NC-SA-4.0 | **forbidden** | high | [author README](https://huggingface.co/Kim2091/UltraSharp), [OpenModelDB](https://openmodeldb.info/models/4x-UltraSharp) |
| digital-art-4x | RealESRGAN_x4plus_anime_6B | Xintao Wang | BSD-3-Clause | allowed | high | [rename 6cfaf45](https://github.com/upscayl/upscayl/commit/6cfaf45) |
| realesr-animevideov3-x2/x3/x4 | realesr-animevideov3 | Xintao Wang | BSD-3-Clause | allowed | medium | [anime video model doc](https://github.com/xinntao/Real-ESRGAN/blob/master/docs/anime_video_model.md) |

## Notes and open points

- **Remacri (medium):** the author's original statement (upscale.wiki's model database) could not be reached; the license rests on OpenModelDB and upstream's own "No commercial use" label. Consistent, but secondhand.
- **UltraMix Balanced (medium):** the license is the author's repository-level CC BY-NC-SA 4.0; UltraMix is an interpolation of UltraSharp with other models, whose terms could also apply. Non-commercial either way.
- **animevideov3 (medium):** BSD-3-Clause per the model repository and OpenModelDB; the NCNN conversion comes from `Real-ESRGAN-ncnn-vulkan`, which is MIT. Both permit commercial use. Upstream keeps these in the repo-root `models/` and does not package them in its app.
- **CC BY-NC-SA and redistribution:** bundling the three non-commercial models in a `.hmod` is a redistribution. It is permitted for non-commercial purposes with attribution and under the same license (ShareAlike). Whether to bundle them, ship them as an optional download, or leave them out is Devon's call; the license gate in the sync script (#8) only blocks `unknown`.
- **Attribution:** `models.json` carries an `attribution` string per model. The panel and the asset's properties should show it; CC BY models require it.

## Checked with

`git show --stat -M 6cfaf45` (renames at 100% similarity), `renderer/locales/en.json` (upstream's "Non-Commercial" labels), and `Real-ESRGAN_LICENSE.txt` in the repo root (BSD-3-Clause, Xintao Wang), on top of the sources linked above.
