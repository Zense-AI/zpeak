# Included before every project() in the whisper.cpp build (see .cargo/config.toml).
# ggml defaults to GGML_NATIVE, which tunes for the CI runner's CPU and crashes on older
# student PCs. Pin a fixed x86-64 level instead; `local::cpu_supported` gates it at runtime.
# Keep the two in sync. On arm64 these x86 options are ignored.
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(GGML_AVX ON CACHE BOOL "" FORCE)
set(GGML_AVX2 ON CACHE BOOL "" FORCE)
set(GGML_FMA ON CACHE BOOL "" FORCE)
set(GGML_F16C ON CACHE BOOL "" FORCE)
