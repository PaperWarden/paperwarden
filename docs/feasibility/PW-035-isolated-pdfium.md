# PW-035: PDFium in an isolated Android process

**Issue:** #59  
**Decision record:** ADR-0007 (containment, Android)  
**Result:** PDFium runs only in a disposable `android:isolatedProcess`. A
render that overruns its time limit or memory ceiling stops only that process;
the app gets a typed error and no workspace is left. Every corpus PDF meets
its outcome through the service.  
**Evidence:** `docs/feasibility/evidence/PW-035/`

## Design

```text
Dart: renderPdf(path, engine: renderInService)
  Rust job runner (app process)
    stage the input into an isolated workspace, validate magic bytes
    pre-check (paperwarden-pdf-scan); notices recorded
    create the empty output
    engine(request)  -- Dart callback --> MethodChannel --> RenderClient (Kotlin)
        bind RenderService (android:isolatedProcess, :pdfium)
        Binder call with two file descriptors: staged input, output
          RenderService --JNI--> paperwarden_pdf::service::serve (Rust)
            watchdog: SIGKILL at the time limit + 500 ms
            RLIMIT_AS = current size + memory budget (512 MiB by default)
            load libpaperwarden_pdfium.so, render, write the first page (PPM)
        unbind; wait for the process to die
    check output size, verify the PPM, export on request, remove the workspace
```

- **Rust owns the bounds.** Kotlin only binds, passes descriptors and calls
  JNI. The watchdog and the address-space ceiling are set in Rust, through
  `nix`'s safe wrappers, before PDFium is loaded. Neither can be lifted again.
- **One job per process.** A second request in the same process is refused
  (`AlreadyUsed`), and the client waits for each process to die before the
  next job, so every job starts with fresh limits and a fresh PDFium.
- **The pre-check stays in the app process.** It is PaperWarden's own bounded
  code; the service only sees documents that pass it.
- **Unsafe code** is confined to `core/crates/bridge/src/service_jni.rs`: the
  `#[no_mangle]` JNI export and taking ownership of the two descriptors.

## PaperWarden's PDFium, not the system's

Every Android process maps `/system/lib64/libpdfium.so`, inherited from
zygote (the platform's `PdfRenderer` uses it). The first device run caught
this: a check for "libpdfium" in the app's memory map found the system copy.
Two consequences:

- The APK ships PaperWarden's build as **`libpaperwarden_pdfium.so`**, and the
  service loads it by that name, so it can only ever load the pinned,
  verified build from `scripts/fetch_pdfium.py`. CI fails an APK that carries
  a plain `libpdfium.so`.
- Native libraries load straight from the APK (`extractNativeLibs` is false),
  so the memory map names `base.apk`. The device test reads the APK's zip
  directory to find where `libpaperwarden_pdfium.so` is stored, and checks
  mappings by file offset: the service maps that range, and the app process
  never does.

The system copy in the app process is an OS component PaperWarden never
calls.

## Results (Android 15 emulator, x86_64, PDFium `chromium/8125`)

`flutter test integration_test -d emulator-5554` gives **16 passed**: the 8
bridge tests and these 8 (`device-run.txt`).

| Test | Result |
| --- | --- |
| A job renders through the service | PPM 200 x 200, verified and exported; no workspace left |
| Isolation | service is `isolatedProcess`, other pid and uid, no `INTERNET` even in a debug build whose app holds it; maps PaperWarden's PDFium; the app process does not |
| Fresh process per job | consecutive jobs have different pids |
| Memory ceiling (96 MiB budget) | a 512 px page renders; a 4096 px page (64 MiB bitmap) is stopped: `PageFailed` or the process dies, depending on where the ceiling is hit; the next job renders |
| Time limit (1 ms) | stopped within the limit plus grace; the next job renders |
| A killed render inside a job | `EngineFailed` or `TimedOut`, no output exported, no workspace left |
| Corpus (16 PDFs) | every fixture meets its outcome (`corpus-android-x64.jsonl`) |
| Flate bomb sent straight to the service (96 MiB budget) | the process dies; a normal page renders within the same budget |

Corpus, through the job runner and the service:

| Outcome | Fixtures | Result |
| --- | --- | --- |
| accept | 5 | rendered, no notices |
| repair | 5 | rendered, "repaired" notice |
| neutralize | 2 | rendered, scripts / launch and web links flagged |
| bound | 3 | deep nesting and the bomb refused by the pre-check; extreme page size refused at render |
| reject | 2 | "the PDF has no content" |

Each job takes about 1.8 s end to end, against 8 to 13 ms for files the
pre-check refuses before any process starts; `large-image` takes 5.2 s. The
split between process start, PDFium and teardown was not measured. That is
acceptable for opening a document, and too slow for thumbnails (see below).

The merged release manifest passes `scripts/check_render_service.py`:
`RenderService` isolated, unexported, alone in `:pdfium`, and no permission
requested beyond AndroidX's own signature permission
(`release-manifest-check.txt`).

## Limits and follow-ups

- **The ceiling is on address space, not resident memory.** ART and PDFium
  reserve address space they never touch, so the budget is not "512 MiB of
  RAM". Loading PDFium alone needs about 64 MiB of headroom on the emulator.
  The 512 MiB default rendered every corpus file; measure on an arm64 phone
  before shipping (the existing PW-010 follow-up).
- **Cancellation waits for the service.** A cancelled job stops at the next
  checkpoint after the service answers or is killed at its time limit. The
  viewer (#69) should add a cancel call that unbinds at once.
- **One process per job** is slow (above). Thumbnails or page-by-page
  viewing need a longer-lived process serving several pages of one document
  under one ceiling; decide that with #69.
- **iOS** has no equivalent for apps (PW-011); there the pre-check remains the
  containment.
