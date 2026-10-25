//! Shared contract checks. The prototype must compile, then match Rust's
//! string with the retained formatter physically absent from the tested view.

use introspect_fmt::Format;
use std::{fs, path::PathBuf, process::Command};

pub fn assert_rendered(view: &Format<'_>, expected: &str) {
    // Passing None prevents even an accidental fallback to retained formatting.
    let independent = Format::new(view.template(), view.args(), None);
    let actual = independent
        .render()
        .unwrap_or_else(|error| panic!("cannot reconstruct {:?}: {error}", view.template()));
    assert_eq!(actual, expected, "template: {:?}", view.template());
}

// Use only expressions without side effects here: Rust and the prototype each
// evaluate their own inputs. Direct calls cover implicit-capture macro hygiene.
macro_rules! parity {
    ($($input:tt)*) => {{
        let expected = std::fmt::format(format_args!($($input)*));
        introspect_fmt::inspect_format!(|view| {
            crate::support::assert_rendered(&view, &expected);
        }, $($input)*);
    }};
}

// Compile unsupported syntax in a subprocess, so one compile gap does not stop
// the remaining output tests from running. Both sides must actually compile;
// a rejection by the prototype is a normal test failure, not an expected pass.
pub fn assert_compiles_and_renders(name: &str, source: &str, dependency_name: &str) {
    assert_compiler_fixture(name, source, dependency_name, false);
}

pub fn assert_reexported_compiles_and_renders(name: &str, source: &str) {
    assert_compiler_fixture(name, source, "facade_fmt", true);
}

fn assert_compiler_fixture(name: &str, source: &str, dependency_name: &str, reexport: bool) {
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let scratch =
        Scratch(std::env::temp_dir().join(format!("introspect-fmt-{}-{name}", std::process::id())));
    fs::create_dir_all(&scratch.0).unwrap();
    let executable = std::env::current_exe().unwrap();
    let deps = executable.parent().unwrap();
    // Use the library Cargo just built, rather than launching nested Cargo and
    // contending with its target lock. Honor custom target directories/profiles.
    let library = fs::read_dir(deps)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libintrospect_fmt-")
                && path
                    .extension()
                    .is_some_and(|extension| extension == "rlib")
        })
        .max_by_key(|path| path.metadata().unwrap().modified().unwrap())
        .expect("Cargo-built introspect_fmt library");
    let library = if reexport {
        // The client receives only this facade, with no direct runtime dependency.
        // Runtime paths in the expanded macro must still resolve to its owner.
        let facade_source = scratch.0.join("facade.rs");
        fs::write(&facade_source, "// Reexport the public API without exposing a runtime crate name.\npub use private_runtime::{inspect_format, Format};\n").unwrap();
        let facade_library = scratch.0.join("libintrospect_fmt_facade.rlib");
        let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .env_remove("RUSTFLAGS")
            .args([
                "--edition=2021",
                "--crate-name",
                "introspect_fmt_facade",
                "--crate-type",
                "rlib",
            ])
            .arg(&facade_source)
            .arg("--extern")
            .arg(format!("private_runtime={}", library.display()))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("-o")
            .arg(&facade_library)
            .output()
            .expect("compile facade");
        assert!(
            output.status.success(),
            "cannot compile facade:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        facade_library
    } else {
        library
    };
    let mut oracle_stdout = None;
    for prototype in [false, true] {
        let replacement = if prototype {
            format!("{dependency_name}::inspect_format!(|view| {{
                let independent = {dependency_name}::Format::new(view.template(), view.args(), None);
                println!(\"{{}}\", independent.render().expect(\"independent reconstruction\"));
            }}, ")
        } else {
            "println!(".to_owned()
        };
        let path = scratch.0.join(if prototype {
            "prototype.rs"
        } else {
            "oracle.rs"
        });
        fs::write(&path, source.replace("CASE!(", &replacement)).unwrap();
        let binary = scratch
            .0
            .join(if prototype { "prototype" } else { "oracle" });
        let mut rustc = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        rustc
            .env_remove("RUSTFLAGS")
            .arg("--edition=2021")
            .arg(&path)
            .arg("-o")
            .arg(&binary);
        if prototype {
            rustc
                .arg("--extern")
                .arg(format!("{dependency_name}={}", library.display()))
                .arg("-L")
                .arg(format!("dependency={}", deps.display()));
        }
        let output = rustc.output().expect("run rustc");
        assert!(
            output.status.success(),
            "{} rejected {name}:\n{}",
            if prototype {
                "prototype"
            } else {
                "Rust oracle"
            },
            String::from_utf8_lossy(&output.stderr)
        );
        let result = Command::new(&binary)
            .output()
            .expect("run compiled fixture");
        assert!(
            result.status.success(),
            "{name} failed to reconstruct:\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if prototype {
            assert_eq!(
                Some(result.stdout),
                oracle_stdout,
                "{name}: reconstructed output differs"
            );
        } else {
            oracle_stdout = Some(result.stdout);
        }
    }
}
