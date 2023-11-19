fn main() {
    println!("cargo:rerun-if-changed=ffi/minimp3.c");
    let mut build = cc::Build::new();

    build.include("ffi/minimp3");

    if cfg!(feature = "float") {
        build.define("MINIMP3_FLOAT_OUTPUT", None);
    }
    if cfg!(not(feature = "simd")) {
        build.define("MINIMP3_NO_SIMD", None);
    }
    if cfg!(not(feature = "mp1-mp2")) {
        build.define("MINIMP3_ONLY_MP3", None);
    }

    build
        .define("MINIMP3_IMPLEMENTATION", None)
        //.target("xtensa-esp32-none-elf")
        .compiler("/home/user/.rustup/toolchains/esp/xtensa-esp-elf/esp-13.2.0_20230928/xtensa-esp-elf/bin/xtensa-esp32-elf-cc")
        .file("ffi/minimp3.c")
        .compile("minimp3");
}
