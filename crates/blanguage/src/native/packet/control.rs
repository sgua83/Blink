//! Borrow the Rust IR during B packet analysis and emission.
use super::*;

#[repr(C)]
struct PacketStage { writes: u32, tile: u32, selected: u32 }
#[repr(C)]
struct Host {
    ir: crate::ir::dataflow::Host,
    stride: unsafe extern "C" fn(*const Context<'_>, usize) -> u32,
    written: unsafe extern "C" fn(*const Context<'_>, usize) -> u32,
    scalar: unsafe extern "C" fn(*const Context<'_>, Reg) -> ScalarEncoding,
}
struct Context<'a> {
    program: &'a Program,
    stage: &'a Stage,
    registers: &'a [Scalar],
}
#[repr(C)]
struct PreparedPacket {
    code: Span,
    places: Span,
    layout: ColorLayout,
}

unsafe extern "C" {
    #[link_name = "bootstrap.coloring::prepare_packet"]
    fn prepare_packet(graph: *const std::ffi::c_void, context: *const Context<'_>, code: Span,
        stage: PacketStage, output: *mut PreparedPacket, host: Host) -> bool;
}
pub(super) fn run<'a>(
    program: &Program, stage: &Stage, code: &'a [Inst], registers: &'a [Scalar], tile_scope: bool, graph: &Graph,
) -> Option<(Plan<'a>, PacketAllocation)> {
    let context = Context { program, stage, registers };
    let packet_stage = PacketStage {
        writes: u32::from(!stage.writes.is_empty()), tile: u32::from(tile_scope),
        selected: u32::from(stage.selection.is_some()),
    };
    let mut product = std::mem::MaybeUninit::<PreparedPacket>::uninit();
    if !unsafe { prepare_packet(graph.state, &context, Span::slice(code), packet_stage, product.as_mut_ptr(), PLAN_HOST) } {
        return None;
    }
    let PreparedPacket { code, places, layout } = unsafe { product.assume_init() };
    let allocation = unsafe { PacketAllocation::from_b(registers.len(), places, layout) };
    Some((Plan { code, _instructions: std::marker::PhantomData }, allocation))
}
unsafe extern "C" fn stride(context: *const Context<'_>, binding: usize) -> u32 {
    unsafe { &*context }.program.buffers[binding].stride
}

unsafe extern "C" fn written(context: *const Context<'_>, binding: usize) -> u32 {
    u32::from(unsafe { &*context }.stage.writes.contains(&binding))
}

unsafe extern "C" fn scalar(context: *const Context<'_>, register: Reg) -> ScalarEncoding {
    unsafe { &*context }.registers[register].into()
}

const PLAN_HOST: Host = Host { ir: crate::ir::dataflow::HOST, stride, written, scalar };
