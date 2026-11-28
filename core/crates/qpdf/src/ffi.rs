//! The only `unsafe` code in this crate: qpdf's C API (PW-012).
//!
//! qpdf's C functions catch every C++ exception and report through return
//! values, so nothing unwinds across this boundary. Every handle is owned by
//! a guard that cleans it up on every path. Only these entry points are
//! declared, by hand from `qpdfjob-c.h`, `qpdflogger-c.h` and `qpdf-c.h`;
//! there is no generated binding.

use std::ffi::{c_char, c_int, c_void, CStr};

#[repr(C)]
struct JobHandle {
    _private: [u8; 0],
}
#[repr(C)]
struct LoggerHandle {
    _private: [u8; 0],
}
#[repr(C)]
struct QpdfData {
    _private: [u8; 0],
}

type LogFn = Option<unsafe extern "C" fn(*const c_char, usize, *mut c_void) -> c_int>;

/// `qpdf_log_dest_discard`: qpdf's messages go nowhere. They can name
/// files and objects, and PaperWarden reports through return values only.
const LOG_DISCARD: c_int = 3;
/// `QPDF_ERRORS`, a bit of `qpdf_read`'s result.
const QPDF_ERRORS: c_int = 1 << 1;
/// `QPDF_TRUE`.
const QPDF_TRUE: c_int = 1;

extern "C" {
    fn qpdfjob_init() -> *mut JobHandle;
    fn qpdfjob_cleanup(job: *mut *mut JobHandle);
    fn qpdfjob_set_logger(job: *mut JobHandle, logger: *mut LoggerHandle);
    fn qpdfjob_initialize_from_json(job: *mut JobHandle, json: *const c_char) -> c_int;
    fn qpdfjob_run(job: *mut JobHandle) -> c_int;

    fn qpdflogger_create() -> *mut LoggerHandle;
    fn qpdflogger_default_logger() -> *mut LoggerHandle;
    fn qpdflogger_cleanup(logger: *mut *mut LoggerHandle);
    fn qpdflogger_set_info(logger: *mut LoggerHandle, dest: c_int, f: LogFn, udata: *mut c_void);
    fn qpdflogger_set_warn(logger: *mut LoggerHandle, dest: c_int, f: LogFn, udata: *mut c_void);
    fn qpdflogger_set_error(logger: *mut LoggerHandle, dest: c_int, f: LogFn, udata: *mut c_void);

    fn qpdf_init() -> *mut QpdfData;
    fn qpdf_cleanup(qpdf: *mut *mut QpdfData);
    fn qpdf_silence_errors(qpdf: *mut QpdfData);
    fn qpdf_set_suppress_warnings(qpdf: *mut QpdfData, value: c_int);
    fn qpdf_read(qpdf: *mut QpdfData, filename: *const c_char, password: *const c_char) -> c_int;
    fn qpdf_get_num_pages(qpdf: *mut QpdfData) -> c_int;
    fn qpdf_get_error(qpdf: *mut QpdfData) -> *mut c_void;

    fn qpdf_global_set_uint32(param: c_int, value: u32) -> c_int;
    fn qpdf_global_get_uint32(param: c_int, value: *mut u32) -> c_int;
}

struct Logger(*mut LoggerHandle);

impl Drop for Logger {
    fn drop(&mut self) {
        // SAFETY: the handle came from qpdflogger_create and is cleaned up
        // once; qpdflogger_cleanup accepts a null handle.
        unsafe { qpdflogger_cleanup(&mut self.0) };
    }
}

struct Job(*mut JobHandle);

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle came from qpdfjob_init and is cleaned up once.
        unsafe { qpdfjob_cleanup(&mut self.0) };
    }
}

struct Document(*mut QpdfData);

impl Drop for Document {
    fn drop(&mut self) {
        // SAFETY: the handle came from qpdf_init and is cleaned up once.
        unsafe { qpdf_cleanup(&mut self.0) };
    }
}

/// Runs a QPDFJob described by `json` and returns its exit code, or `None`
/// if qpdf could not even start (out of memory) or refused the job's JSON.
pub(crate) fn run_job(json: &CStr) -> Option<c_int> {
    // SAFETY: qpdflogger_create has no preconditions.
    let logger = Logger(unsafe { qpdflogger_create() });
    if logger.0.is_null() {
        return None;
    }
    // SAFETY: the logger is live; a discard destination takes no function.
    unsafe {
        qpdflogger_set_info(logger.0, LOG_DISCARD, None, std::ptr::null_mut());
        qpdflogger_set_warn(logger.0, LOG_DISCARD, None, std::ptr::null_mut());
        qpdflogger_set_error(logger.0, LOG_DISCARD, None, std::ptr::null_mut());
    }
    // SAFETY: qpdfjob_init has no preconditions.
    let job = Job(unsafe { qpdfjob_init() });
    if job.0.is_null() {
        return None;
    }
    // SAFETY: both handles are live; the job keeps its own reference to the
    // logger, so dropping our handle first is fine.
    unsafe { qpdfjob_set_logger(job.0, logger.0) };
    // SAFETY: `json` is a NUL-terminated string that outlives the call.
    if unsafe { qpdfjob_initialize_from_json(job.0, json.as_ptr()) } != 0 {
        return None;
    }
    // SAFETY: the job was initialized above.
    Some(unsafe { qpdfjob_run(job.0) })
}

/// The number of pages in the PDF at `path`, opened with `password` (none:
/// a null pointer, which qpdf reads as no password), or `None` if qpdf
/// cannot open it.
pub(crate) fn page_count(path: &CStr, password: Option<&CStr>) -> Option<u32> {
    // SAFETY: qpdf_init has no preconditions.
    let document = Document(unsafe { qpdf_init() });
    if document.0.is_null() {
        return None;
    }
    // SAFETY: the handle is live and both strings outlive the calls.
    unsafe {
        qpdf_silence_errors(document.0);
        qpdf_set_suppress_warnings(document.0, QPDF_TRUE);
        let password = password.map_or(std::ptr::null(), CStr::as_ptr);
        let read = qpdf_read(document.0, path.as_ptr(), password);
        let pages = qpdf_get_num_pages(document.0);
        // Collect any error, so qpdf_cleanup has none left to report.
        let _ = qpdf_get_error(document.0);
        if read & QPDF_ERRORS != 0 {
            return None;
        }
        u32::try_from(pages).ok()
    }
}

/// Points qpdf's process-wide default logger at nothing, so no qpdf code
/// path can print (some, such as `qpdf_cleanup`, use it directly). `false`
/// if qpdf has no default logger.
pub(crate) fn silence_default_logger() -> bool {
    // SAFETY: qpdflogger_default_logger has no preconditions and returns a
    // new handle to the shared logger, which is cleaned up below.
    let logger = Logger(unsafe { qpdflogger_default_logger() });
    if logger.0.is_null() {
        return false;
    }
    // SAFETY: the handle is live; a discard destination takes no function.
    unsafe {
        qpdflogger_set_info(logger.0, LOG_DISCARD, None, std::ptr::null_mut());
        qpdflogger_set_warn(logger.0, LOG_DISCARD, None, std::ptr::null_mut());
        qpdflogger_set_error(logger.0, LOG_DISCARD, None, std::ptr::null_mut());
    }
    true
}

/// Sets one of qpdf's process-wide limits (`qpdf_param_e`); `false` if qpdf
/// refused it.
pub(crate) fn set_global(param: c_int, value: u32) -> bool {
    // SAFETY: plain values; qpdf validates `param`.
    // qpdf_result_e: anything up to qpdf_r_success (0xffff) is success.
    unsafe { qpdf_global_set_uint32(param, value) <= 0xffff }
}

/// Reads one of qpdf's process-wide limits back, or `None` if qpdf refused.
pub(crate) fn get_global(param: c_int) -> Option<u32> {
    let mut value = 0_u32;
    // SAFETY: `value` is a valid, writable u32 for the duration of the call.
    let result = unsafe { qpdf_global_get_uint32(param, &mut value) };
    (result <= 0xffff).then_some(value)
}
