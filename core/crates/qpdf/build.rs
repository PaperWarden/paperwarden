//! Builds qpdf, zlib and IJG libjpeg from the pinned upstream sources that
//! `scripts/fetch_qpdf.py` verifies (PW-012).
//!
//! The sources live outside git, in `third_party/qpdf/src` (or
//! `PAPERWARDEN_QPDF_SRC`). Without them the crate still builds, without
//! qpdf: every operation reports `QpdfError::Unavailable`, and the tests
//! that need qpdf are ignored unless run with `--ignored`.
//!
//! Only qpdf's own ("native") crypto is built: no OpenSSL or GnuTLS. The
//! configuration header that qpdf's CMake would generate is written here.

#![allow(
    clippy::panic,
    clippy::print_stdout,
    reason = "a build script reports to Cargo on stdout and fails by panicking"
)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const ZLIB: &[&str] = &[
    "adler32.c",
    "compress.c",
    "crc32.c",
    "deflate.c",
    "infback.c",
    "inffast.c",
    "inflate.c",
    "inftrees.c",
    "trees.c",
    "uncompr.c",
    "zutil.c",
];

// IJG libjpeg 9f's library sources (makefile.ansi's LIBOBJECTS), with the
// no-backing-store memory manager.
const JPEG: &[&str] = &[
    "jaricom.c",
    "jcapimin.c",
    "jcapistd.c",
    "jcarith.c",
    "jccoefct.c",
    "jccolor.c",
    "jcdctmgr.c",
    "jchuff.c",
    "jcinit.c",
    "jcmainct.c",
    "jcmarker.c",
    "jcmaster.c",
    "jcomapi.c",
    "jcparam.c",
    "jcprepct.c",
    "jcsample.c",
    "jctrans.c",
    "jdapimin.c",
    "jdapistd.c",
    "jdarith.c",
    "jdatadst.c",
    "jdatasrc.c",
    "jdcoefct.c",
    "jdcolor.c",
    "jddctmgr.c",
    "jdhuff.c",
    "jdinput.c",
    "jdmainct.c",
    "jdmarker.c",
    "jdmaster.c",
    "jdmerge.c",
    "jdpostct.c",
    "jdsample.c",
    "jdtrans.c",
    "jerror.c",
    "jfdctflt.c",
    "jfdctfst.c",
    "jfdctint.c",
    "jidctflt.c",
    "jidctfst.c",
    "jidctint.c",
    "jmemmgr.c",
    "jmemnobs.c",
    "jquant1.c",
    "jquant2.c",
    "jutils.c",
];

// qpdf 12.4.2's libqpdf_SOURCES plus libqpdf_crypto_native
// (libqpdf/CMakeLists.txt).
const QPDF: &[&str] = &[
    "BitStream.cc",
    "BitWriter.cc",
    "Buffer.cc",
    "BufferInputSource.cc",
    "ClosedFileInputSource.cc",
    "ContentNormalizer.cc",
    "CryptoRandomDataProvider.cc",
    "FileInputSource.cc",
    "InputSource.cc",
    "InsecureRandomDataProvider.cc",
    "JSON.cc",
    "JSONHandler.cc",
    "MD5.cc",
    "NNTree.cc",
    "OffsetInputSource.cc",
    "PDFVersion.cc",
    "Pipeline.cc",
    "Pl_AES_PDF.cc",
    "Pl_ASCII85Decoder.cc",
    "Pl_ASCIIHexDecoder.cc",
    "Pl_Base64.cc",
    "Pl_Buffer.cc",
    "Pl_Concatenate.cc",
    "Pl_Count.cc",
    "Pl_DCT.cc",
    "Pl_Discard.cc",
    "Pl_Flate.cc",
    "Pl_Function.cc",
    "Pl_LZWDecoder.cc",
    "Pl_OStream.cc",
    "Pl_PNGFilter.cc",
    "Pl_QPDFTokenizer.cc",
    "Pl_RC4.cc",
    "Pl_RunLength.cc",
    "Pl_SHA2.cc",
    "Pl_StdioFile.cc",
    "Pl_String.cc",
    "Pl_TIFFPredictor.cc",
    "QPDF.cc",
    "QPDFAcroFormDocumentHelper.cc",
    "QPDFAnnotationObjectHelper.cc",
    "QPDFArgParser.cc",
    "QPDFCryptoProvider.cc",
    "QPDFDocumentHelper.cc",
    "QPDFEFStreamObjectHelper.cc",
    "QPDFEmbeddedFileDocumentHelper.cc",
    "QPDFExc.cc",
    "QPDFFileSpecObjectHelper.cc",
    "QPDFFormFieldObjectHelper.cc",
    "QPDFJob.cc",
    "QPDFJob_argv.cc",
    "QPDFJob_config.cc",
    "QPDFJob_json.cc",
    "QPDFLogger.cc",
    "QPDFMatrix.cc",
    "QPDFObject.cc",
    "QPDFObjectHandle.cc",
    "QPDFObjectHelper.cc",
    "QPDFOutlineDocumentHelper.cc",
    "QPDFOutlineObjectHelper.cc",
    "QPDFPageLabelDocumentHelper.cc",
    "QPDFPageObjectHelper.cc",
    "QPDFParser.cc",
    "QPDFStreamFilter.cc",
    "QPDFSystemError.cc",
    "QPDFTokenizer.cc",
    "QPDFUsage.cc",
    "QPDFWriter.cc",
    "QPDF_Array.cc",
    "QPDF_Dictionary.cc",
    "QPDF_Stream.cc",
    "QPDF_String.cc",
    "QPDF_encryption.cc",
    "QPDF_json.cc",
    "QPDF_linearization.cc",
    "QPDF_objects.cc",
    "QPDF_pages.cc",
    "QTC.cc",
    "QUtil.cc",
    "RC4.cc",
    "ResourceFinder.cc",
    "SecureRandomDataProvider.cc",
    "SF_FlateLzwDecode.cc",
    "global.cc",
    "qpdf-c.cc",
    "qpdfjob-c.cc",
    "qpdflogger-c.cc",
    // Native crypto.
    "AES_PDF_native.cc",
    "MD5_native.cc",
    "QPDFCrypto_native.cc",
    "RC4_native.cc",
    "SHA2_native.cc",
    "rijndael.cc",
];
const QPDF_C: &[&str] = &["sha2.c", "sha2big.c"];

const JCONFIG: &str = "\
#ifndef JCONFIG_H
#define JCONFIG_H
#define HAVE_PROTOTYPES
#define HAVE_UNSIGNED_CHAR
#define HAVE_UNSIGNED_SHORT
#define HAVE_STDDEF_H
#define HAVE_STDLIB_H
#define HAVE_LOCALE_H
#ifdef _WIN32
# ifndef __RPCNDR_H__
typedef unsigned char boolean;
# endif
# ifndef FALSE
#  define FALSE 0
# endif
# ifndef TRUE
#  define TRUE 1
# endif
# define HAVE_BOOLEAN
#endif
#endif
";

fn main() {
    println!("cargo:rustc-check-cfg=cfg(paperwarden_qpdf)");
    println!("cargo:rerun-if-env-changed=PAPERWARDEN_QPDF_SRC");
    println!("cargo:rerun-if-changed=build.rs");
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let src = env::var_os("PAPERWARDEN_QPDF_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../../third_party/qpdf/src"));
    for name in ["qpdf", "zlib", "jpeg"] {
        println!(
            "cargo:rerun-if-changed={}",
            src.join(name).join("PAPERWARDEN_SOURCE").display()
        );
    }
    if !src.join("qpdf/libqpdf/QPDF.cc").is_file() {
        // No sources: build without qpdf (see the module comment).
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap_or_default());
    let config = out.join("config");
    fs::create_dir_all(config.join("qpdf")).unwrap_or_else(|e| panic!("create config dir: {e}"));
    fs::write(config.join("qpdf/qpdf-config.h"), qpdf_config())
        .unwrap_or_else(|e| panic!("write qpdf-config.h: {e}"));
    fs::write(config.join("jconfig.h"), JCONFIG).unwrap_or_else(|e| panic!("write jconfig.h: {e}"));

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    // The C and C++ here must target the same iOS release as the Rust that
    // links them; left to itself cc targets the SDK's newest, and the link
    // then fails on symbols such as ___chkstk_darwin. Xcode (and so Cargokit)
    // sets this for app builds; elsewhere it must be set to the app's
    // minimum, iOS 17 (ADR-0007).
    println!("cargo:rerun-if-env-changed=IPHONEOS_DEPLOYMENT_TARGET");
    if target_os == "ios" && env::var_os("IPHONEOS_DEPLOYMENT_TARGET").is_none() {
        panic!("set IPHONEOS_DEPLOYMENT_TARGET=17.0 to build qpdf for iOS (ADR-0007)");
    }
    let msvc = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "msvc";

    compile_c("pw_zlib", &src.join("zlib"), ZLIB, &[&config]);
    compile_c("pw_jpeg", &src.join("jpeg"), JPEG, &[&config]);
    compile_c(
        "pw_qpdf_sha2",
        &src.join("qpdf/libqpdf"),
        QPDF_C,
        &[
            &config,
            &src.join("qpdf/include"),
            &src.join("qpdf/libqpdf"),
        ],
    );

    let libqpdf = src.join("qpdf/libqpdf");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .warnings(false)
        .define("QPDF_DISABLE_QTC", "1")
        .include(&config)
        .include(src.join("qpdf/include"))
        .include(&libqpdf)
        .include(src.join("zlib"))
        .include(src.join("jpeg"))
        .files(QPDF.iter().map(|file| libqpdf.join(file)));
    if msvc {
        build.flag("/std:c++20").flag("/EHsc");
    } else {
        build.flag("-std=c++20");
    }
    if target_os == "android" {
        // Link libc++ statically, so the app needs no libc++_shared.so.
        build.cpp_link_stdlib(None);
        // Copy just libc++ into OUT_DIR (already a search path): the NDK
        // directory also holds a static libc.a, which must not be linked.
        let lib = ndk_libcxx_dir(&build);
        for archive in ["libc++_static.a", "libc++abi.a"] {
            fs::copy(lib.join(archive), out.join(archive))
                .unwrap_or_else(|e| panic!("copy {archive}: {e}"));
        }
    }
    build.compile("pw_qpdf");
    if target_os == "android" {
        println!("cargo:rustc-link-lib=static=c++_static");
        println!("cargo:rustc-link-lib=static=c++abi");
    }
    if target_os == "windows" {
        // SecureRandomDataProvider uses the Windows crypto API.
        println!("cargo:rustc-link-lib=advapi32");
    }
    println!("cargo:rustc-cfg=paperwarden_qpdf");
}

/// The NDK directory holding `libc++_static.a` for this target, found next to
/// the NDK compiler: `<toolchain>/bin/clang++` and
/// `<toolchain>/sysroot/usr/lib/<triple>/`.
fn ndk_libcxx_dir(build: &cc::Build) -> PathBuf {
    let triple = match env::var("CARGO_CFG_TARGET_ARCH")
        .unwrap_or_default()
        .as_str()
    {
        "aarch64" => "aarch64-linux-android",
        "arm" => "arm-linux-androideabi",
        "x86_64" => "x86_64-linux-android",
        "x86" => "i686-linux-android",
        other => panic!("no NDK libc++ for architecture {other}"),
    };
    let compiler = build.get_compiler();
    let toolchain = compiler
        .path()
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("cannot find the NDK from {}", compiler.path().display()));
    let lib = toolchain.join("sysroot/usr/lib").join(triple);
    if !lib.join("libc++_static.a").is_file() {
        panic!("libc++_static.a not found in {}", lib.display());
    }
    lib
}

fn compile_c(name: &str, dir: &Path, files: &[&str], includes: &[&Path]) {
    let mut build = cc::Build::new();
    build.warnings(false).include(dir);
    for include in includes {
        build.include(include);
    }
    if env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "msvc" {
        build.define("_CRT_SECURE_NO_WARNINGS", None);
    }
    build
        .files(files.iter().map(|file| dir.join(file)))
        .compile(name);
}

/// What qpdf's CMake would write to `qpdf/qpdf-config.h` for this target.
fn qpdf_config() -> String {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let family = env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let pointer_width: usize = env::var("CARGO_CFG_TARGET_POINTER_WIDTH")
        .ok()
        .and_then(|width| width.parse().ok())
        .unwrap_or(64);
    let unix = family.split(',').any(|f| f == "unix");
    let apple = matches!(os.as_str(), "ios" | "macos");
    let mut lines = vec![
        "#define DEFAULT_CRYPTO \"native\"".to_owned(),
        "#define USE_CRYPTO_NATIVE 1".to_owned(),
        "#define HAVE_INTTYPES_H 1".to_owned(),
        "#define HAVE_STDINT_H 1".to_owned(),
        format!("#define SIZEOF_SIZE_T {}", pointer_width / 8),
    ];
    if unix {
        lines.extend(
            [
                "#define HAVE_FSEEKO 1",
                "#define HAVE_LOCALTIME_R 1",
                "#define HAVE_RANDOM 1",
                "#define HAVE_TM_GMTOFF 1",
            ]
            .map(str::to_owned),
        );
        // Apple's time.h declares timezone() as a function, not a variable.
        if !apple {
            lines.push("#define HAVE_EXTERN_LONG_TIMEZONE 1".to_owned());
        }
        // 64-bit file offsets on 32-bit glibc; Android before API 24 must
        // not set it (qpdf's own CMake check).
        if pointer_width == 32 && os == "linux" {
            lines.push("#define _FILE_OFFSET_BITS 64".to_owned());
        }
    }
    lines.join("\n") + "\n"
}
