use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

type DynError = Box<dyn std::error::Error>;

#[derive(Debug)]
struct Paths {
    include_paths: Vec<PathBuf>,
    link_paths: Vec<PathBuf>,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            include_paths: vec![search().join("include").join("opus")],
            link_paths: vec![search().join("lib")],
        }
    }
}

impl From<pkg_config::Library> for Paths {
    fn from(val: pkg_config::Library) -> Self {
        Self {
            include_paths: val.include_paths,
            link_paths: val.link_paths,
        }
    }
}

fn version() -> String {
    "1.3.1".to_string()
}

fn output() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").unwrap())
}

fn source() -> PathBuf {
    output().join(format!("opus-{}", version()))
}

fn search() -> PathBuf {
    let mut absolute = env::current_dir().unwrap();
    absolute.push(&output());
    absolute.push("dist");

    absolute
}

fn fetch() -> io::Result<()> {
    #[cfg(windows)]
    let configure = "CMakeLists.txt";
    #[cfg(unix)]
    let configure = "autogen.sh";
    let configure_path = &output()
        .join(format!("opus-{}", version()))
        .join(configure);
    if fs::metadata(configure_path).is_ok() {
        return Ok(());
    }
    let url =
        env::var("OPUS_GIT_URL").unwrap_or_else(|_| "https://github.com/xiph/opus".to_string());
    let status = Command::new("git")
        .current_dir(&output())
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg("-b")
        .arg(format!("v{}", version()))
        .arg(url)
        .arg(format!("opus-{}", version()))
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "fetch failed"))
    }
}

fn check_prog(name: &str, args: &[&str]) -> bool {
    if let Ok(out) = Command::new(name).args(args).output() {
        out.status.success()
    } else {
        false
    }
}

#[cfg(windows)]
fn build() -> io::Result<Paths> {
    let is_target_env_gnu = env::var("CARGO_CFG_TARGET_ENV").map_or(false, |v| v == "gnu");

    // make sure the `make/nmake` exists
    let (make_prog_name, make_prog_args) = if is_target_env_gnu {
        ("make", ["--version"])
    } else {
        ("nmake", ["/?"])
    };
    if !check_prog(make_prog_name, &make_prog_args) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("The `{}` not found, install or add to PATH and try again!", make_prog_name),
        ));
    }

    // make sure the `cmake` exists
    if !check_prog("cmake", &["--version"]) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "The `cmake` not found, install or add to PATH and try again!",
        ));
    }

    let generator = if is_target_env_gnu {
        "Unix Makefiles"
    } else {
        "NMake Makefiles"
    };
    let mut configure = Command::new("cmake");
    configure.current_dir(&source());
    configure.args(&["-G", generator]);
    configure.arg(format!("-DCMAKE_BUILD_TYPE={}", "Release"));
    configure.arg(format!("-DCMAKE_INSTALL_PREFIX={}", search().to_string_lossy()));
    configure.arg("-DOPUS_STACK_PROTECTOR=OFF");

    // run ./configure
    let output = configure
        .output()
        .unwrap_or_else(|_| panic!("{:?} failed", configure));
    if !output.status.success() {
        println!("configure: {}", String::from_utf8_lossy(&output.stdout));

        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "configure failed {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    // run make
    if !Command::new(make_prog_name)
        .current_dir(&source())
        .status()?
        .success()
    {
        return Err(io::Error::new(io::ErrorKind::Other, "make failed"));
    }

    // run make install
    if !Command::new(make_prog_name)
        .arg("install")
        .current_dir(&source())
        .status()?
        .success()
    {
        return Err(io::Error::new(io::ErrorKind::Other, "make install failed"));
    }

    Ok(Paths::default())
}

#[cfg(unix)]
fn build() -> io::Result<Paths> {
    // make sure the `make` exists
    if !check_prog("make", &["--version"]) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "The `make` not found, install or add to PATH and try again!",
        ));
    }

    // make sure the `autoreconf` exists
    if !check_prog("autoreconf", &["--version"]) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "The `autoreconf` not found, install or add to PATH and try again!",
        ));
    }

    // make sure the `libtool` exists
    if !check_prog("libtool", &["--version"]) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "The libtool` not found, install or add to PATH and try again!",
        ));
    }

    let mut autogen_sh = Command::new("./autogen.sh");
    autogen_sh.current_dir(&source());

    let mut configure = Command::new("./configure");
    configure.current_dir(&source());
    configure.arg(format!("--prefix={}", search().to_string_lossy()));

    if env::var("TARGET").unwrap() != env::var("HOST").unwrap() {
        let target = env::var("TARGET").unwrap();
        let linker = env::var("RUSTC_LINKER").expect("Missing RUSTC_LINKER for cross compile");
        if linker.contains(&target) {
            configure.arg(format!("--host={}", target));
        } else {
            let (target, _) = &linker.trim().split_at(linker.rfind('-').unwrap());
            configure.arg(format!("--host={}", target));
        }
    }

    // make it static
    configure.arg("--enable-static");
    configure.arg("--disable-shared");

    // don't build docs and programs
    configure.arg("--disable-doc");
    configure.arg("--disable-extra-programs");
    configure.arg("--with-pic");

    // run ./autogen.sh
    let _output = autogen_sh
        .output()
        .unwrap_or_else(|_| panic!("{:?} failed", autogen_sh));

    // run ./configure
    let output = configure
        .output()
        .unwrap_or_else(|_| panic!("{:?} failed", configure));
    if !output.status.success() {
        println!("configure: {}", String::from_utf8_lossy(&output.stdout));

        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "configure failed {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    // run make
    if !Command::new("make")
        .arg("-j")
        .arg(num_cpus::get().to_string())
        .current_dir(&source())
        .status()?
        .success()
    {
        return Err(io::Error::new(io::ErrorKind::Other, "make failed"));
    }

    // run make install
    if !Command::new("make")
        .arg("install")
        .current_dir(&source())
        .status()?
        .success()
    {
        return Err(io::Error::new(io::ErrorKind::Other, "make install failed"));
    }

    Ok(Paths::default())
}

fn probe_prebuilt() -> Result<Paths, DynError> {
    let lib_name = if env::var("CARGO_CFG_TARGET_ENV").map_or(false, |v| v == "gnu") {
        "libopus.a"
    } else {
        "opus.lib"
    };

    match fs::metadata(&search().join("lib").join(lib_name)) {
        Ok(_) => Ok(Paths::default()),
        Err(_) => Err(Box::new(io::Error::new(io::ErrorKind::NotFound, ""))),
    }
}

fn main() -> Result<(), DynError> {
    let paths = pkg_config::probe_library("opus").map_or_else(
        |_| {
            let paths = probe_prebuilt()
                .or_else(|_| {
                    fs::create_dir_all(&output()).expect("Failed to create build directory");
                    fetch().unwrap();
                    build()
                })
                .expect("Unable to build libopus from source");

            let lib_path = search().join("lib");
            println!("cargo:rustc-link-search=native={}", lib_path.display());
            println!("cargo:rustc-link-lib={}={}", "static", "opus");

            paths
        },
        Paths::from,
    );

    let include_paths = paths
        .include_paths
        .iter()
        .map(|x| format!("-I{}", x.display()))
        .collect::<Vec<String>>();

    let wrapper_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("wrapper.h");
    let wrapper_path = wrapper_path.to_str().unwrap();
    let mut wrapper = File::create(wrapper_path).unwrap();
    writeln!(wrapper, "#include <opus.h>")?;

    let bindings = bindgen::Builder::default()
        .header(wrapper_path)
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: false,
        })
        .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
        .allowlist_function("^opus_.*")
        .allowlist_type("^opus_.*")
        .allowlist_type("^OPUS_.*")
        .allowlist_type("^Opus.*")
        .allowlist_var("^OPUS_.*")
        .use_core()
        .clang_args(include_paths)
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    Ok(())
}
