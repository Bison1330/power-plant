#[cfg(feature = "std")]
fn main() {
    let mainnet = std::env::var("CARGO_FEATURE_MAINNET_RUNTIME").is_ok();
    let testnet = std::env::var("CARGO_FEATURE_TESTNET_RUNTIME").is_ok();

    let file_name = match (mainnet, testnet) {
        (true, false) => "vitreus_power_plant_mainnet_runtime",
        (false, true) => "vitreus_power_plant_testnet_runtime",
        (false, false) => panic!("Either the mainnet or testnet runtime must be enabled."),
        (true, true) => {
            panic!("The mainnet and testnet runtimes cannot be enabled simultaneously.")
        },
    };

    let mut builder =
        substrate_wasm_builder::WasmBuilder::init_with_defaults().set_file_name(file_name);

    // Keep machine-specific paths out of the wasm. rustc embeds absolute
    // source paths (panic `Location`s) for every workspace and dependency
    // crate, and wasm-builder compiles core/alloc from source (`-Zbuild-std`),
    // so the rustup home is embedded too. Remap the workspace root, the cargo
    // home (registry + git checkouts) and the toolchain sysroot to fixed
    // names. This is necessary but not sufficient for a reproducible hash:
    // cargo also hashes each path crate's absolute path into its
    // `-C metadata` — the workspace crates and the `-Zbuild-std` crates under
    // the rustup home — which no flag can change, so the build must also run
    // with the workspace at `/build` and `RUSTUP_HOME=/rustup` (README,
    // "Reproducing a runtime wasm").
    for (from, to) in remap_prefixes() {
        builder = builder.append_to_rust_flags(format!("--remap-path-prefix={from}={to}"));
    }

    builder.build()
}

/// `(absolute prefix, replacement)` pairs, longest prefix first so a cargo
/// home nested inside the workspace (or vice versa) still remaps correctly.
#[cfg(feature = "std")]
fn remap_prefixes() -> Vec<(String, &'static str)> {
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let workspace = manifest_dir
        .ancestors()
        .nth(2) // runtime/vitreus -> runtime -> <workspace root>
        .expect("runtime/vitreus sits two levels below the workspace root")
        .to_path_buf();

    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")));

    // `rustc --print sysroot` of the compiler cargo handed us; wasm-builder
    // uses the same toolchain unless WASM_BUILD_TOOLCHAIN overrides it.
    let sysroot = std::env::var_os("RUSTC")
        .and_then(|rustc| {
            std::process::Command::new(rustc).args(["--print", "sysroot"]).output().ok()
        })
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let mut pairs = vec![(workspace.display().to_string(), "/power-plant")];
    if let Some(cargo_home) = cargo_home {
        pairs.push((cargo_home.display().to_string(), "/cargo"));
    }
    if let Some(sysroot) = sysroot {
        pairs.push((sysroot, "/rustc-sysroot"));
    }
    pairs.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    pairs
}

#[cfg(not(feature = "std"))]
fn main() {}
