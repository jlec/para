# Vendored preprocessor assets

These two ONNX graphs perform mel-spectrogram feature extraction for the NeMo Conformer model
family (NVIDIA Parakeet). They are small (~138KB / ~87KB), stable, and shared across many models —
vendored directly into `para` rather than downloaded at runtime (research.md §10).

| File | Used by | features_size | SHA-256 |
|---|---|---|---|
| `nemo128.onnx` | TDT models (`parakeet-tdt-0.6b-v3`, `parakeet-tdt-0.6b-v2`) | 128 | `95afc3b529db4f84e038461d7d02e090c5aa2d28c68bc6c83f4255a9b3562f60` |
| `nemo80.onnx` | CTC models (`parakeet-ctc-0.6b`) | 80 | `ea9d24c9bc3ea5ff1b8a2796ad7d1168487b0d004ed1bd860d6d257ea71ac1b8` |

**Source**: extracted from the `onnx-asr` PyPI wheel `onnx_asr-0.11.0-py3-none-any.whl`
(`onnx_asr/preprocessors/data/nemo128.onnx` and `.../nemo80.onnx`), the only source where both
files are reliably and directly available — individual HuggingFace model repos in this family
bundle `nemo128.onnx` inconsistently (present in the two TDT repos, absent from the CTC and RNNT
repos) and never bundle `nemo80.onnx` at all.

**License**: MIT, copyright (c) 2025 Ilya Stupakov (`onnx-asr` project,
https://github.com/istupakov/onnx-asr). Full license text: `LICENSE` in this directory.

**Provenance verification**: the SHA-256 values above were computed directly from the files as
extracted from the wheel — not asserted or copied from any third-party listing (Constitution
Principle V).
