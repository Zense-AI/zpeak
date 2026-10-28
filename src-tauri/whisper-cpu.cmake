# Included before every project() in the whisper.cpp build (see .cargo/config.toml).
# ggml defaults to GGML_NATIVE, which tunes for the CI runner's CPU and crashes on older
# student PCs. Pin a fixed x86-64 level instead; `local::cpu_supported` gates it at runtime.
# Keep the two in sync. On arm64 these x86 options are ignored.
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(GGML_AVX ON CACHE BOOL "" FORCE)
set(GGML_AVX2 ON CACHE BOOL "" FORCE)
set(GGML_FMA ON CACHE BOOL "" FORCE)
set(GGML_F16C ON CACHE BOOL "" FORCE)

# cmake-rs replaces CMAKE_<LANG>_FLAGS_RELEASE for MSVC and drops every /O flag, which
# leaves whisper.cpp unoptimised (about 8x slower). Restore CMake's MSVC release defaults.
if(CMAKE_HOST_WIN32)
  set(CMAKE_C_FLAGS_RELEASE "/MD /O2 /Ob2 /DNDEBUG" CACHE STRING "" FORCE)
  set(CMAKE_CXX_FLAGS_RELEASE "/MD /O2 /Ob2 /DNDEBUG" CACHE STRING "" FORCE)
endif()
