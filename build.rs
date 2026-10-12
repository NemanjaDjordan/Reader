use std::{env, path::PathBuf, process::Command};

fn main() {
    slint_build::compile("ui/app.slint").expect("Slint UI se nije mogao kompajlirati");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
        let resource = output.join("reader-icon.o");
        let windres = "x86_64-w64-mingw32-windres";
        let status = Command::new(&windres)
            .args(["packaging/windows/reader.rc", "-O", "coff", "-o"])
            .arg(&resource)
            .status()
            .unwrap_or_else(|error| panic!("Could not run {windres}: {error}"));
        assert!(status.success(), "Windows icon resource was not created");
        println!("cargo:rustc-link-arg-bin=citac={}", resource.display());
        println!("cargo:rustc-link-arg-bin=citac=-mwindows");
        println!("cargo:rerun-if-changed=packaging/windows/reader.rc");
        println!("cargo:rerun-if-changed=assets/app-icon/app-icon.ico");
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR nije postavljen"));
        let pthread_stub = output.join("libpthread.a");
        let status = Command::new("ar")
            .args(["crs"])
            .arg(&pthread_stub)
            .status()
            .expect("Nije moguće pokrenuti ar za Android pthread shim");
        assert!(status.success(), "Android pthread shim nije napravljen");
        println!("cargo:rustc-link-search=native={}", output.display());
    }

    println!("cargo:rustc-check-cfg=cfg(citac_qt_epub)");
    println!("cargo:rerun-if-changed=prototype/epub_qt/CMakeLists.txt");
    println!("cargo:rerun-if-changed=prototype/epub_qt/main.cpp");

    if env::var_os("CARGO_FEATURE_QT_EPUB").is_none()
        || env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux")
    {
        return;
    }

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR nije postavljen"));
    let build = output.join("qt-epub-renderer");
    let configure = Command::new("cmake")
        .args(["-S", "prototype/epub_qt", "-B"])
        .arg(&build)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .status();
    let configured = matches!(configure, Ok(status) if status.success());
    let compiled = configured
        && matches!(
            Command::new("cmake")
                .arg("--build")
                .arg(&build)
                .args(["--target", "citac-epub-qt-prototype", "--parallel", "2"])
                .status(),
            Ok(status) if status.success()
        );

    if compiled {
        let helper = build.join("citac-epub-qt-prototype");
        println!("cargo:rustc-cfg=citac_qt_epub");
        println!(
            "cargo:rustc-env=CITAC_QT_EPUB_HELPER_BUILD={}",
            helper.display()
        );
    } else {
        println!(
            "cargo:warning=Qt EPUB renderer nije izgrađen; aplikacija koristi ugrađeni EPUB fallback"
        );
    }
}
