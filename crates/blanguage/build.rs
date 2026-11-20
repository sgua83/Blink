#[path = "src/build/bootstrap_jobs.rs"]
mod bootstrap_jobs;
#[path = "src/build/compiler_identity.rs"]
mod compiler_identity;

use bootstrap_jobs::Job;
use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

fn compile(name: &'static str, source: &str, include: &str, output: &Path) -> Job {
    Job {
        name,
        compiler: true,
        arguments: [
            "-c".into(),
            source.into(),
            "-I".into(),
            include.into(),
            "--module".into(),
            "-o".into(),
            output.as_os_str().into(),
        ]
        .into(),
        inputs: Vec::new(),
    }
}

fn archive(
    name: &'static str,
    members: impl Iterator<Item = PathBuf>,
    output: &Path,
    inputs: Vec<usize>,
) -> Job {
    let mut arguments: Vec<OsString> = vec!["--host-archive".into()];
    arguments.extend(members.map(PathBuf::into_os_string));
    arguments.extend(["-o".into(), output.as_os_str().into()]);
    Job {
        name,
        compiler: false,
        arguments,
        inputs,
    }
}

fn native_modules() -> Result<(), Box<dyn std::error::Error>> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    // Cargo already owns one job for this build script and exposes the remaining budget.
    let cargo = unsafe { jobserver::Client::from_env() }
        .ok_or_else(|| io::Error::other("缺少Cargo的B引导构建工作许可"))?;
    let cpu_budget: std::num::NonZeroUsize = env::var("NUM_JOBS")?.parse()?;
    println!("cargo:rerun-if-env-changed=BLINK_BOOTSTRAP_SDK");
    let seed = match env::var_os("BLINK_BOOTSTRAP_SDK") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env::var_os("ProgramFiles").ok_or("缺少ProgramFiles")?).join("Blink"),
    };
    // The inputs stay installed until their final compiler and archiver have exited.
    let _seed_use = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(seed.join("VERSION"))?;
    for name in ["VERSION", "bin/bc.exe", "bin/blink.exe"] {
        println!("cargo:rerun-if-changed={}", seed.join(name).display());
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?);
    let value_object = out.join("blink_value_format.obj");
    let value_archive = out.join("blink_value_format.lib");
    let compiler_archive = out.join("blink_compiler_bootstrap.lib");
    let object = |name: &str| out.join(format!("blink_compiler_{name}.obj"));
    let coff_source = "../../library/blink/windows/coff";
    let coff_object = object("coff_read");
    for extension in ["b", "h"] {
        println!("cargo:rerun-if-changed=src/codec/value_format.{extension}");
        println!("cargo:rerun-if-changed={coff_source}.{extension}");
    }
    let mut jobs = vec![
        compile("value_format", "src/codec/value_format.b", "src", &value_object),
        compile("coff_read", &format!("{coff_source}.b"), "../../library", &coff_object),
    ];
    let compiler_start = jobs.len();
    let mut compiler_members = Vec::new();
    let compiler = env::var_os("CARGO_FEATURE_COMPILER").is_some();
    if compiler {
        println!("cargo:rustc-env=BLINK_BUILT_SHARED_OBJECTS={}", out.display());
        println!("cargo:rerun-if-changed=src/bootstrap");
        for (name, source) in [
            ("unicode_scalar", "../../library/blink/unicode_scalar"),
            ("path_core", "../../library/blink/windows/path_core"),
            ("process_core", "../../library/blink/windows/process_core"),
            ("text_core", "../../library/blink/text_core"),
        ] {
            for extension in ["b", "h"] {
                println!("cargo:rerun-if-changed={source}.{extension}");
            }
            let object = object(name);
            jobs.push(compile(
                name,
                &format!("{source}.b"),
                "../../library",
                &object,
            ));
            compiler_members.push(object);
        }
        for header in [
            "directory",
            "error",
            "file",
            "path",
            "process",
            "process_extended",
            "system",
            "thread",
            "time",
        ] {
            println!("cargo:rerun-if-changed=../../library/blink/windows/{header}.h");
        }
        for &name in MODULES {
            let output = object(name);
            let mut job = compile(name, &format!("src/bootstrap/{name}.b"), "src", &output);
            let inputs: &[&str] = match name {
                "contract_codec" => &["value_format"],
                "host_sdk" => {
                    job.arguments.extend(["-I".into(), "../../library".into()]);
                    &["path_core", "ast_storage"]
                }
                "metadata_read" | "object_read" => &["coff_read"],
                "pe" => &["coff_read", "combine"],
                "parser" => &["unicode_scalar"],
                _ => &[],
            };
            for &input in inputs {
                let index = jobs
                    .iter()
                    .position(|producer| producer.name == input)
                    .ok_or_else(|| {
                        io::Error::other(format!("B引导模块{name}缺少对象生产者{input}"))
                    })?;
                job.inputs.push(index);
                let file = if index == 0 {
                    value_object.clone()
                } else {
                    object(input)
                };
                job.arguments
                    .extend(["--import".into(), file.into_os_string()]);
            }
            if name == "names" {
                job.arguments.extend(
                    [
                        "--instance",
                        "sort_rows<u64>",
                        "--instance",
                        "sort_rows<u32>",
                        "--instance",
                        "sort_rows<array<u64,3>>",
                    ]
                    .map(OsString::from),
                );
            }
            jobs.push(job);
            compiler_members.push(output);
        }
    }
    if !compiler && let Some(directory) = env::var_os("BLINK_DEVELOPMENT_SHARED_OBJECTS") {
        let directory = Path::new(&directory);
        for output in [&value_object, &coff_object] {
            if output.try_exists()? {
                continue;
            }
            let source = directory.join(output.file_name().unwrap());
            let mut source = match fs::File::open(source) {
                Ok(source) => source,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            // Publish a complete object for bc to validate against its actual inputs.
            let staged = output.with_extension("seed.obj");
            io::copy(&mut source, &mut fs::File::create(&staged)?)?;
            fs::rename(staged, output)?;
        }
    }
    let compiler_inputs = (compiler_start..jobs.len()).collect();
    jobs.push(archive(
        "blink_value_format.lib",
        [value_object, coff_object].into_iter(),
        &value_archive,
        (0..compiler_start).collect(),
    ));
    if compiler {
        jobs.push(archive(
            "blink_compiler_bootstrap.lib",
            compiler_members.into_iter(),
            &compiler_archive,
            compiler_inputs,
        ));
    }
    bootstrap_jobs::run(&seed, &jobs, cargo, cpu_budget.get())?;
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=blink_value_format");
    if compiler {
        println!("cargo:rustc-link-lib=static=blink_compiler_bootstrap");
        let mut identity = compiler_identity::Identity::new();
        for archive in [value_archive, compiler_archive] {
            identity.library(&mut fs::File::open(archive)?, io::sink())?;
        }
        compiler_identity::emit(identity)?;
    }
    Ok(())
}

const MODULES: &[&str] = &[
    "lexer",
    "parser",
    "names",
    "signatures",
    "type_families",
    "type_layout",
    "scalar_fold",
    "constant_evaluation",
    "source_call",
    "call_aliases",
    "call_arguments",
    "call_execution",
    "function_body",
    "return_exit",
    "written_output",
    "archive",
    "coff",
    "metadata",
    "bindings",
    "metadata_read",
    "object_read",
    "ast_storage",
    "combine",
    "pe",
    "native_import",
    "native_resources",
    "instance_domain",
    "unwind",
    "frame_layout",
    "x64_instruction",
    "coloring",
    "call_layout",
    "spirv_envelope",
    "static_uses",
    "native_select",
    "host_sdk",
    "module_load",
    "ir_dataflow",
    "stage_fusion",
    "source_bind",
    "type_reference",
    "c_function",
    "type_value",
    "generic_instance",
    "participation",
    "cli",
    "numeric_literal",
    "build_graph",
    "build_schedule",
    "build_digest",
    "view_liveness",
    "return_scope",
    "operator_types",
    "contract_condition",
    "ownership_position",
    "ownership_presence",
    "ownership_path",
    "ownership_projection",
    "ownership_storage",
    "ownership_release",
    "ownership_cleanup",
    "dataset_drop",
    "task_completion",
    "task_callbacks",
    "task_lower",
    "native_model",
    "native_canonical",
    "function_inputs",
    "task_parameters",
    "ownership_payload",
    "ownership_place",
    "contract_codec",
    "condition_graph",
    "stack_report",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/build/bootstrap_jobs.rs");
    native_modules()
}
