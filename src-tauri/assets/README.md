# Vendored assets

## `melfilters.bytes`

The 80 x 201 mel filterbank Whisper's audio front-end needs: 16,080
little-endian `f32` coefficients (80 mel bands over `N_FFT / 2 + 1 = 201`
frequency bins).

- **Source:** [huggingface/candle](https://github.com/huggingface/candle),
  `candle-examples/examples/whisper/melfilters.bytes`
- **License:** MIT / Apache-2.0 (candle is dual-licensed; this project is
  Apache-2.0, so the Apache-2.0 option applies)
- **Size:** 64,320 bytes

It is vendored rather than downloaded so a first run needs exactly one network
fetch — the model weights — instead of two. It's a fixed mathematical constant of
the Whisper architecture, not user data or a model checkpoint, and at 64 KB it
costs nothing to keep in-tree.

Consumed by `src/core/whisper.rs` via `include_bytes!`, and its shape is asserted
in that module's tests.
