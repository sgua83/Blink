//! Borrows emitted images for the B command module and writes its selected files.
use blanguage::{
    Compiler,
    ast::Target,
    ir::{Operation, Program, interface_layout_json},
    native::{self, ContinuationCode, NativeCode, StageCode},
    spirv::{self, ShaderCode, TerminalScan},
};
use std::{borrow::Cow, ffi::c_void, path::Path, ptr, slice};

type Error = Box<dyn std::error::Error>;
type GpuImage<'a> = spirv::ProgramImage<&'a Program>;
type CpuImage<'a> = native::ProgramImage<&'a Program>;

#[repr(C)]
#[derive(Clone, Copy)]
struct Span {
    data: *const c_void,
    length: usize,
}

impl Span {
    fn new<T>(items: &[T]) -> Self {
        Self {
            data: items.as_ptr().cast(),
            length: items.len(),
        }
    }

    fn absent() -> Self {
        Self {
            data: ptr::null(),
            length: 0,
        }
    }

    fn words(items: Option<&[u32]>) -> Self {
        items.map_or_else(Self::absent, Self::new)
    }

    // The item type and lifetime are those of the original borrowed slice.
    unsafe fn as_slice<'a, T>(self) -> &'a [T] {
        unsafe { slice::from_raw_parts(self.data.cast(), self.length) }
    }
}

#[repr(C)]
struct GpuStage {
    main: Span,
    tail: Span,
    scan_add: Span,
    dispatch: Span,
    selector: Span,
    finish_name: Span,
    finish: Span,
}

#[repr(C)]
struct GpuAdditional {
    sort: Span,
    segmented_single_group: Span,
    terminal_root: Span,
    terminal_tail: Span,
}

#[repr(u32)]
enum CpuContinuation {
    None,
    ReductionTail,
    ScanAdd,
    CompactScatter,
    SegmentedFinish,
    Scan,
}

#[repr(C)]
struct CpuStage {
    main: *const NativeCode,
    continuation: CpuContinuation,
    first: *const NativeCode,
    second: *const NativeCode,
    custom_scan: bool,
    initialize: Span,
    histogram: *const NativeCode,
    scatter: *const NativeCode,
    gather: *const NativeCode,
}

// B chooses the file type, name, order and pairing. Each case supplies just
// the requested bytes for the original synchronous file write.
#[repr(u32)]
#[expect(dead_code, reason = "the B command module supplies the ABI discriminants")]
enum FileContent {
    Spirv,
    SpirvJson,
    Native,
    NativeJson,
    Relocations,
    Layout,
    LayoutJson,
}

#[repr(C)]
struct ExportHost {
    context: *mut c_void,
    gpu: unsafe extern "C" fn(*mut c_void, Span) -> bool,
    cpu: unsafe extern "C" fn(*mut c_void, Span) -> bool,
    gpu_stage: unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> GpuStage,
    gpu_additional: unsafe extern "C" fn(*const c_void, usize) -> GpuAdditional,
    gpu_sort_shader: unsafe extern "C" fn(Span, usize) -> Span,
    cpu_stage: unsafe extern "C" fn(*const c_void, usize) -> CpuStage,
    cpu_initializer: unsafe extern "C" fn(Span, usize) -> *const NativeCode,
    prepare_spirv: unsafe extern "C" fn(*mut c_void, Span) -> bool,
    write: unsafe extern "C" fn(*mut c_void, Span, FileContent, Span) -> bool,
}

unsafe extern "C" {
    #[link_name = "bootstrap.cli::export_program"]
    fn export_program(
        target: Target,
        name: Span,
        ordinal: usize,
        output: *mut u8,
        host: ExportHost,
    );
    #[link_name = "bootstrap.cli::export_gpu_image"]
    fn export_gpu_image(host: ExportHost, stem: Span, image: *const c_void, count: usize) -> bool;
    #[link_name = "bootstrap.cli::export_cpu_image"]
    fn export_cpu_image(host: ExportHost, stem: Span, image: *const c_void, count: usize) -> bool;
}

struct Export<'a> {
    compiler: &'a Compiler,
    program: &'a Program,
    out: &'a Path,
    layout: String,
    spirv_json: String,
    error: Option<Error>,
}

pub fn program(
    compiler: &Compiler,
    program: &Program,
    out: &Path,
    ordinal: usize,
) -> Result<(), Error> {
    let name = program.name().as_bytes();
    // The longest suffix includes the instance, stage, key, and file extension.
    let mut output =
        Vec::<u8>::with_capacity(name.len() + ".instance-..key-.spv.json".len() + 3 * 20);
    let mut export = Export {
        compiler,
        program,
        out,
        layout: program.describe(),
        spirv_json: String::new(),
        error: None,
    };
    // B only borrows these objects synchronously. The image outlives every
    // shader and machine code borrow and is released before the next target.
    unsafe {
        export_program(
            program.target(),
            Span::new(name),
            ordinal,
            output.as_mut_ptr(),
            host(ptr::from_mut(&mut export).cast()),
        );
    }
    export.error.map_or(Ok(()), Err)
}

fn host(context: *mut c_void) -> ExportHost {
    ExportHost {
        context,
        gpu,
        cpu,
        gpu_stage,
        gpu_additional,
        gpu_sort_shader,
        cpu_stage,
        cpu_initializer,
        prepare_spirv,
        write,
    }
}

unsafe extern "C" fn gpu(context: *mut c_void, stem: Span) -> bool {
    let export = unsafe { &mut *context.cast::<Export>() };
    match export.compiler.spirv(export.program) {
        Ok(image) => unsafe {
            export_gpu_image(
                host(context),
                stem,
                ptr::from_ref(&image).cast(),
                image.stages().len(),
            )
        },
        Err(error) => {
            export.error = Some(std::io::Error::other(error).into());
            false
        }
    }
}

unsafe extern "C" fn cpu(context: *mut c_void, stem: Span) -> bool {
    let export = unsafe { &mut *context.cast::<Export>() };
    match export.compiler.native(export.program) {
        Ok(image) => unsafe {
            export_cpu_image(
                host(context),
                stem,
                ptr::from_ref(&image).cast(),
                image.stages().len(),
            )
        },
        Err(error) => {
            export.error = Some(std::io::Error::other(error).into());
            false
        }
    }
}

unsafe extern "C" fn gpu_stage(
    context: *mut c_void,
    image: *const c_void,
    index: usize,
) -> GpuStage {
    let export = unsafe { &mut *context.cast::<Export>() };
    let image = unsafe { &*image.cast::<GpuImage>() };
    let stage = &image.stages()[index];
    export.layout.push_str(&format!(
        "gpu_stage={index}, shared_bytes={}\n",
        image.program().stages()[index].shared_bytes()
    ));
    let (finish_name, finish) = match &stage.finish {
        Some((name, words)) => (Span::new(name.as_bytes()), Span::new(words)),
        None => (Span::absent(), Span::absent()),
    };
    GpuStage {
        main: Span::new(&stage.main),
        tail: Span::words(stage.tail.as_deref()),
        scan_add: Span::words(stage.scan_add.as_deref()),
        dispatch: Span::words(stage.dispatch.as_deref()),
        selector: Span::words(stage.selector.as_deref()),
        finish_name,
        finish,
    }
}

unsafe extern "C" fn gpu_additional(image: *const c_void, index: usize) -> GpuAdditional {
    let image = unsafe { &*image.cast::<GpuImage>() };
    let plan = image.program();
    let shaders = image.stages()[index].additional_shaders(plan, &plan.stages()[index]);
    let [terminal_root, terminal_tail] = shaders.terminal_scans.map(|entry| match entry {
        Some(TerminalScan::Alternate(shader)) => Span::new(&shader.words),
        _ => Span::absent(),
    });
    GpuAdditional {
        sort: Span::new(shaders.sort),
        segmented_single_group: Span::words(
            shaders
                .segmented_single_group
                .map(|shader| shader.words.as_slice()),
        ),
        terminal_root,
        terminal_tail,
    }
}

unsafe extern "C" fn gpu_sort_shader(shaders: Span, index: usize) -> Span {
    let shaders = unsafe { shaders.as_slice::<ShaderCode>() };
    Span::new(&shaders[index].words)
}

unsafe extern "C" fn cpu_stage(image: *const c_void, index: usize) -> CpuStage {
    let image = unsafe { &*image.cast::<CpuImage>() };
    match &image.stages()[index] {
        StageCode::Element {
            main, continuation, ..
        } => {
            use CpuContinuation as Kind;
            let (continuation, first, second) = match continuation {
                None => (Kind::None, ptr::null(), ptr::null()),
                Some(ContinuationCode::ReductionTail(code)) => {
                    (Kind::ReductionTail, ptr::from_ref(code), ptr::null())
                }
                Some(ContinuationCode::ScanAdd(code)) => {
                    (Kind::ScanAdd, ptr::from_ref(code), ptr::null())
                }
                Some(ContinuationCode::CompactScatter(code)) => {
                    (Kind::CompactScatter, ptr::from_ref(code), ptr::null())
                }
                Some(ContinuationCode::SegmentedFinish(code)) => {
                    (Kind::SegmentedFinish, ptr::from_ref(code), ptr::null())
                }
                Some(ContinuationCode::Scan { prefix, apply, .. }) => {
                    (Kind::Scan, ptr::from_ref(prefix), ptr::from_ref(apply))
                }
            };
            CpuStage {
                main,
                continuation,
                first,
                second,
                custom_scan: matches!(
                    image.program().stages()[index].operation,
                    Operation::CustomScan(_)
                ),
                initialize: Span::absent(),
                histogram: ptr::null(),
                scatter: ptr::null(),
                gather: ptr::null(),
            }
        }
        StageCode::Sort(sort) => CpuStage {
            main: ptr::null(),
            continuation: CpuContinuation::None,
            first: ptr::null(),
            second: ptr::null(),
            custom_scan: false,
            initialize: Span::new(&sort.initialize),
            histogram: &sort.histogram,
            scatter: &sort.scatter,
            gather: &sort.gather,
        },
    }
}

unsafe extern "C" fn cpu_initializer(codes: Span, index: usize) -> *const NativeCode {
    let codes = unsafe { codes.as_slice::<NativeCode>() };
    &codes[index]
}

unsafe extern "C" fn prepare_spirv(context: *mut c_void, words: Span) -> bool {
    let export = unsafe { &mut *context.cast::<Export>() };
    match spirv::module_json(unsafe { words.as_slice() }) {
        Ok(json) => {
            export.spirv_json = json;
            true
        }
        Err(error) => {
            export.error = Some(error.into());
            false
        }
    }
}

unsafe extern "C" fn write(
    context: *mut c_void,
    name: Span,
    content: FileContent,
    data: Span,
) -> bool {
    let export = unsafe { &mut *context.cast::<Export>() };
    // The name consists of the UTF-8 program name and B's ASCII components.
    let name = unsafe { std::str::from_utf8_unchecked(name.as_slice()) };
    let code = data.data.cast::<NativeCode>();
    let result: Result<(), Error> = (|| {
        let bytes: Cow<'_, [u8]> = match content {
            FileContent::Spirv => {
                let words = unsafe { data.as_slice::<u32>() };
                Cow::Owned(words.iter().flat_map(|word| word.to_le_bytes()).collect())
            }
            FileContent::SpirvJson => {
                Cow::Owned(std::mem::take(&mut export.spirv_json).into_bytes())
            }
            FileContent::Native => Cow::Borrowed(&unsafe { &*code }.bytes),
            FileContent::NativeJson => Cow::Owned(unsafe { &*code }.code_json(0)?.into_bytes()),
            FileContent::Relocations => {
                let text = unsafe { &*code }
                    .relocations
                    .iter()
                    .map(|r| format!("{} {:?}\n", r.offset, r.target))
                    .collect::<String>();
                Cow::Owned(text.into_bytes())
            }
            FileContent::Layout => Cow::Borrowed(export.layout.as_bytes()),
            FileContent::LayoutJson => {
                Cow::Owned(interface_layout_json(export.program).into_bytes())
            }
        };
        std::fs::write(export.out.join(name), bytes)?;
        Ok(())
    })();
    match result {
        Ok(()) => true,
        Err(error) => {
            export.error = Some(error);
            false
        }
    }
}
