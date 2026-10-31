use std::env;

fn add_sources(build: &mut cc::Build, root: &str, files: &[&str]) {
    let root = std::path::Path::new(root);
    build.files(files.iter().map(|src| {
        let mut p = root.join(src);
        p.set_extension("c");
        p
    }));

    build.include(root);
}

// MSVC pulls Skia's libpng objects as well as ours. Use libpng's own exported
// and internal symbol lists so FreeType retains its PNG glyph decoder without
// colliding with another bundled libpng (ipdf Windows native build).
fn prefix_png() {
    let out = std::path::PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let config = std::fs::read_to_string("libpng/scripts/pnglibconf.h.prebuilt").unwrap();
    std::fs::write(out.join("pnglibconf.out"), &config).unwrap();
    let symbols = |script: &str| {
        let mut preprocessor = cc::Build::new();
        preprocessor
            .include("libpng")
            .include(&out)
            .define("ZLIB_STATIC", None);
        if let Ok(paths) = env::var("DEP_Z_INCLUDE") {
            for path in paths.split(',') {
                preprocessor.include(path);
            }
        }
        preprocessor.file(format!("libpng/scripts/{script}.c"));
        let expanded = String::from_utf8(preprocessor.expand()).unwrap();
        expanded
            .split("PNG_DFN")
            .skip(1)
            .filter_map(|item| item.split('"').nth(2))
            .map(str::trim)
            .filter(|name| name.starts_with("png_"))
            .map(|name| format!("#define {name} freetype_sys_{name}\n"))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<String>()
    };
    let public = symbols("prefix");
    let internal = symbols("intprefix");
    let macros = std::fs::read_to_string("libpng/scripts/macro.lst")
        .unwrap()
        .split_whitespace()
        .map(|name| format!("#define freetype_sys_png_{name} PNG_{name}\n"))
        .collect::<String>();
    std::fs::write(
        "libpng/pnglibconf.h",
        format!("{config}\n#define PNG_PREFIX freetype_sys_\n{public}{macros}"),
    )
    .unwrap();
    std::fs::write("libpng/pngprefix.h", internal).unwrap();
}

fn main() {
    if !cfg!(feature = "bundled") {
        let target = env::var("TARGET").unwrap();
        if !target.contains("android") && !target.contains("ohos") {
            pkg_config::Config::new()
                .atleast_version("24.3.18")
                .probe("freetype2")
                .unwrap();
        }
        return;
    }

    // From libpng/png.h:
    // "If pnglibconf.h is missing, you can copy scripts/pnglibconf.h.prebuilt to pnglibconf.h"
    std::fs::copy(
        "libpng/scripts/pnglibconf.h.prebuilt",
        "libpng/pnglibconf.h",
    )
    .unwrap();

    if env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc" {
        prefix_png();
    }

    let mut build = cc::Build::new();

    build
        .warnings(false)
        .include(".")
        .include("freetype2/include")
        .include("libpng")
        .define("FT2_BUILD_LIBRARY", None)
        .define("FT_CONFIG_OPTION_USE_PNG", None);

    add_sources(
        &mut build,
        "freetype2/src",
        &[
            "autofit/autofit",
            "base/ftbase",
            "base/ftbbox",
            "base/ftbdf",
            "base/ftbitmap",
            "base/ftcid",
            "base/ftdebug",
            "base/ftfstype",
            "base/ftgasp",
            "base/ftglyph",
            "base/ftgxval",
            "base/ftinit",
            "base/ftmm",
            "base/ftotval",
            "base/ftpatent",
            "base/ftpfr",
            "base/ftstroke",
            "base/ftsynth",
            "base/ftsystem",
            "base/fttype1",
            "base/ftwinfnt",
            "bdf/bdf",
            "bzip2/ftbzip2",
            "cache/ftcache",
            "cff/cff",
            "cid/type1cid",
            "gzip/ftgzip",
            "hvf/hvf",
            "lzw/ftlzw",
            "pcf/pcf",
            "pfr/pfr",
            "psaux/psaux",
            "pshinter/pshinter",
            "psnames/psnames",
            "raster/raster",
            "sdf/sdf",
            "svg/svg",
            "sfnt/sfnt",
            "smooth/smooth",
            "truetype/truetype",
            "type1/type1",
            "type42/type42",
            "winfonts/winfnt",
        ],
    );

    build.compile("freetype2");

    println!("cargo:rustc-link-lib=z");

    let mut build = cc::Build::new();
    build.include("libpng");
    // Use DEP_Z_INCLUDE from libz-sys to find zlib headers.
    // The upstream crate hardcodes "libz-sys/src/zlib" which is a git submodule
    // not included in the published crate, breaking Windows MSVC builds.
    if let Ok(z_include) = std::env::var("DEP_Z_INCLUDE") {
        for path in z_include.split(',') {
            build.include(path);
        }
    }
    // On Windows, define ZLIB_STATIC so libpng uses static zlib symbols
    // (avoids __imp_ DLL import link errors).
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        build.define("ZLIB_STATIC", None);
    }
    build
        .file("libpng/png.c")
        .file("libpng/pngerror.c")
        .file("libpng/pngget.c")
        .file("libpng/pngmem.c")
        .file("libpng/pngpread.c")
        .file("libpng/pngread.c")
        .file("libpng/pngrio.c")
        .file("libpng/pngrtran.c")
        .file("libpng/pngrutil.c")
        .file("libpng/pngset.c")
        .file("libpng/pngtrans.c")
        .file("libpng/pngwio.c")
        .file("libpng/pngwrite.c")
        .file("libpng/pngwtran.c")
        .file("libpng/pngwutil.c");

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if arch == "arm" || arch == "aarch64" {
        build
            .file("libpng/arm/arm_init.c")
            .file("libpng/arm/filter_neon_intrinsics.c")
            .file("libpng/arm/filter_neon.S")
            .file("libpng/arm/palette_neon_intrinsics.c");
    }

    build.compile("libpng.a");
}
