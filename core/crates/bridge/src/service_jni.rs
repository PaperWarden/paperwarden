//! The JNI entry point of the Android render service (#59).
//!
//! `RenderService` runs in its own `android:isolatedProcess` and calls this
//! with the staged input and the job's output as file descriptors it has
//! detached from their `ParcelFileDescriptor`s. Everything after taking
//! ownership of the descriptors is safe Rust in
//! [`paperwarden_pdf::service::serve`].
//!
//! The arguments are JNI primitives only (`jint`, `jlong`), so no JNI
//! environment calls are needed. The answer is a `ServiceStatus` code.

use std::ffi::c_void;
use std::fs::File;
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::Path;
use std::time::Duration;

use paperwarden_pdf::service::{serve, ServiceRequest, ServiceStatus};

/// PaperWarden's PDFium build, found by name in the app's native libraries.
/// It is loaded only here, in the service process. The APK ships it under its
/// own name because every Android process already maps the system's
/// `libpdfium.so` (inherited from zygote); a distinct name means this can
/// only ever load the pinned, verified build (`scripts/fetch_pdfium.py`).
const PDFIUM: &str = "libpaperwarden_pdfium.so";

fn take(fd: i32) -> Option<File> {
    if fd < 0 {
        return None;
    }
    // SAFETY: the caller detached `fd` from its ParcelFileDescriptor and
    // passes its ownership here; nothing else uses or closes it afterwards.
    Some(File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
}

fn unsigned(value: i64) -> Option<u64> {
    u64::try_from(value).ok()
}

/// `dev.paperwarden.paperwarden.RenderNative.render`.
#[no_mangle]
#[allow(
    clippy::too_many_arguments,
    reason = "the JNI signature is flat by design"
)]
pub extern "system" fn Java_dev_paperwarden_paperwarden_RenderNative_render(
    _env: *mut c_void,
    _class: *mut c_void,
    input: i32,
    output: i32,
    max_width: i32,
    max_height: i32,
    max_input_bytes: i64,
    max_output_bytes: i64,
    time_limit_ms: i64,
    memory_budget_bytes: i64,
) -> i32 {
    // Both descriptors are owned from here on, and closed on every path.
    let (input, output) = (take(input), take(output));
    let request = (|| {
        Some(ServiceRequest {
            max_width: u32::try_from(max_width).ok()?,
            max_height: u32::try_from(max_height).ok()?,
            max_input_bytes: unsigned(max_input_bytes)?,
            max_output_bytes: unsigned(max_output_bytes)?,
            time_limit: Duration::from_millis(unsigned(time_limit_ms)?),
            memory_budget: unsigned(memory_budget_bytes)?,
        })
    })();
    let (Some(input), Some(output), Some(request)) = (input, output, request) else {
        return ServiceStatus::InvalidRequest.code();
    };
    serve(Path::new(PDFIUM), input, output, &request).code()
}
