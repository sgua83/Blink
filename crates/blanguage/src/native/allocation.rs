//! 按结构化 IR 活跃性着色；调用跨越值使用 ABI 保留寄存器，溢出槽按干涉复用。
use super::*;
use crate::ir::dataflow::Span;
use std::{collections::{BTreeMap, BTreeSet, btree_map, btree_set}, ffi::c_void, ptr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Location {
    Immediate(Bits),
    Gpr(u8),
    Xmm(u8),
    Stack(u32),
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct ColorLayout {
    end: u64,
    gpr: u32,
    xmm: u32,
}

#[repr(C)]
pub(super) struct Allocation {
    positions: ColorPositions,
    layout: ColorLayout,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ColorPositions {
    rows: Span,
    registers: usize,
}
pub(super) struct PacketAllocation {
    pub(super) positions: ColorPositions,
    layout: ColorLayout,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct SavedLayout {
    gpr_base: u64,
    xmm_base: u64,
    gpr: u32,
    xmm: u32,
}

#[repr(C)]
struct FrameSaveLayout {
    saved: SavedLayout,
    count: usize,
    end: u64,
}

#[repr(u32)]
#[allow(dead_code)] // The B layout selects the stream to visit.
enum TileFrameBank {
    Shared = 0,
    Private = 1,
}

#[repr(C)]
struct TileFrameSlot {
    key: *const c_void,
    value: u64,
}

#[repr(C)]
struct TileFrameHost {
    next: unsafe extern "C" fn(*mut c_void, TileFrameBank) -> TileFrameSlot,
    place: unsafe extern "C" fn(*mut c_void, TileFrameBank, *const c_void, u32),
}

struct TileFrameCursor<'a> {
    shared: btree_map::Iter<'a, SharedWord, (Scalar, usize, u32)>,
    private: btree_set::Iter<'a, PrivateSlot>,
    registers: &'a [Scalar],
    locals: &'a BTreeMap<usize, Ty>,
    frame: &'a mut Frame,
}

unsafe extern "C" fn next_tile_frame_slot(
    context: *mut c_void, bank: TileFrameBank,
) -> TileFrameSlot {
    let cursor = unsafe { &mut *context.cast::<TileFrameCursor<'_>>() };
    let slot = match bank {
        TileFrameBank::Shared => cursor.shared.next().map(|(key, &(_, slot, _))| TileFrameSlot {
            key: ptr::from_ref(key).cast(),
            value: slot as u64,
        }),
        TileFrameBank::Private => cursor.private.next().map(|key| TileFrameSlot {
            key: ptr::from_ref(key).cast(),
            value: u64::from(key.scalar(cursor.registers, cursor.locals).size()),
        }),
    };
    slot.unwrap_or(TileFrameSlot { key: ptr::null(), value: 0 })
}

unsafe extern "C" fn place_tile_frame_slot(
    context: *mut c_void, bank: TileFrameBank, key: *const c_void, offset: u32,
) {
    let cursor = unsafe { &mut *context.cast::<TileFrameCursor<'_>>() };
    // Both key collections outlive the synchronous layout.
    match bank {
        TileFrameBank::Shared => {
            cursor.frame.tile_shared.insert(unsafe { *key.cast::<SharedWord>() }, offset);
        }
        TileFrameBank::Private => {
            cursor.frame.tile_private.insert(unsafe { *key.cast::<PrivateSlot>() }, offset);
        }
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.frame_layout::frame_save_layout"]
    fn frame_save_layout(scalar: ColorLayout, packet: ColorLayout) -> FrameSaveLayout;
    #[link_name = "bootstrap.frame_layout::tile_frame"]
    fn tile_frame(data_end: u64, width: u64, steps: u32, context: *mut c_void, host: TileFrameHost) -> u64;
}

#[derive(Default)]
pub(super) struct Frame {
    pub saved: SavedLayout,
    pub saved_count: usize,
    pub end: u64,
    pub tile_shared: std::collections::BTreeMap<SharedWord, u32>,
    pub tile_private: std::collections::BTreeMap<PrivateSlot, u32>,
}
pub(super) struct Graph {
    private: BTreeSet<(usize, usize)>,
    pub(super) state: *mut c_void,
    pub(super) needs_r15: bool,
}
#[path = "allocation/graph.rs"]
mod graph;
impl Graph {
    fn together(&mut self, live: &BTreeSet<PrivateSlot>) { graph::together(self, live); }
}

impl Graph {
    pub(super) fn function_inputs(&mut self,inputs:&[Reg]) {
        // Entry values are simultaneous definitions supplied by the native caller.
        graph::inputs(self, inputs);
    }
    pub fn with_live(
        code: &[Inst],
        scalars: &[Scalar],
        locals: &std::collections::BTreeMap<usize, Ty>,
        mut live: BTreeSet<PrivateSlot>,
    ) -> Self {
        let private = private_spans(code);
        let (state, needs_r15) = graph::build(code, scalars, locals, &private, &mut live);
        Self { private, state, needs_r15 }
    }

    pub(super) fn tile_interference(&mut self, phases: &[TilePhase<'_>]) {
        // 前一阶段的出站保存槽属于后一阶段的入口活跃集合。
        for phase in phases {
            self.together(&phase.live_in);
        }
    }

}
impl Drop for Graph {
    fn drop(&mut self) {
        unsafe { graph_release(self.state); }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ColorPlace {
    value: u64,
    kind: u32,
    width: u8,
    floating: u8,
}
impl ColorPlace {
    const IMMEDIATE: u32 = 1;
    const GPR: u32 = 2;
    const XMM: u32 = 3;
    const STACK: u32 = 4;

    fn scalar(self) -> Location {
        match self.kind {
            Self::IMMEDIATE => Location::Immediate(self.value),
            Self::GPR => Location::Gpr(self.value as u8),
            Self::XMM => Location::Xmm(self.value as u8),
            Self::STACK => Location::Stack(self.value as u32),
            _ => unreachable!("unplaced scalar"),
        }
    }
}


#[repr(C)]
struct PrivateDeclaration {
    offsets: Span,
    begin: usize,
    end: usize,
    size: u32,
    alignment: u32,
}

struct PrivateDeclarations<'a> {
    spans: btree_set::Iter<'a, (usize, usize)>,
    locals: &'a std::collections::BTreeMap<usize, Ty>,
}

unsafe extern "C" fn next_private_declaration(context: *mut c_void) -> PrivateDeclaration {
    let state = unsafe { &mut *context.cast::<PrivateDeclarations<'_>>() };
    let Some(&(begin, end)) = state.spans.next() else {
        return PrivateDeclaration { offsets: Span::EMPTY, begin: 0, end: 0, size: 0, alignment: 0 };
    };
    let ty = &state.locals[&end];
    PrivateDeclaration {
        offsets: Span::field(&ty.components, |component| &component.1),
        begin,
        end,
        size: ty.size,
        alignment: ty.align,
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.coloring::color_places_drop"]
    fn color_places_drop(places: Span);
    #[link_name = "bootstrap.coloring::graph_release"]
    fn graph_release(graph: *mut c_void);
    #[link_name = "bootstrap.coloring::finish_scalar"]
    fn finish_scalar(graph: *mut c_void, mode: u32, context: *mut c_void,
        next: unsafe extern "C" fn(*mut c_void) -> PrivateDeclaration) -> Allocation;
}
impl Allocation {
    pub fn new(
        locals: &std::collections::BTreeMap<usize, Ty>, graph: &mut Graph, tile_scope: bool,
    ) -> Self {
        // All entry and tile interference has now been collected. Packet coloring
        // borrows these same ordered rows after scalar coloring returns.
        let mut private = PrivateDeclarations { spans: graph.private.iter(), locals };
        unsafe { finish_scalar(graph.state, u32::from(tile_scope),
            ptr::from_mut(&mut private).cast(), next_private_declaration) }
    }
    pub fn location(&self, slot: PrivateSlot) -> Location {
        let locations = unsafe { self.positions.rows.as_slice::<ColorPlace>() };
        locations[match slot {
            PrivateSlot::Register(r) => r,
            PrivateSlot::Local(s) => self.positions.registers + s,
        }].scalar()
    }
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe { color_places_drop(self.positions.rows) };
    }
}
impl PacketAllocation {
    // The result transfers the B color table and all its initialized rows.
    pub(super) unsafe fn from_b(registers: usize, locations: Span, layout: ColorLayout) -> Self {
        Self { positions: ColorPositions { rows: locations, registers }, layout }
    }
}
impl Drop for PacketAllocation {
    fn drop(&mut self) {
        unsafe { color_places_drop(self.positions.rows) };
    }
}
impl Frame {
    pub fn layout_tile(
        &mut self, registers: &[Scalar], locals: &BTreeMap<usize, Ty>,
        workgroup: u32, phases: &[TilePhase<'_>], shared: crate::ir::dataflow::TileLayout,
    ) -> u64 {
        // Multiple phase entries can restore the same physical private stream.
        let private: BTreeSet<_> = phases.iter().skip(1)
            .flat_map(|phase| phase.live_in.iter().copied()).collect();
        let data_end = self.end;
        let mut cursor = TileFrameCursor {
            shared: shared.iter(),
            private: private.iter(),
            registers,
            locals,
            frame: self,
        };
        let host = TileFrameHost { next: next_tile_frame_slot, place: place_tile_frame_slot };
        unsafe {
            tile_frame(data_end, u64::from(workgroup), u32::from(!phases.is_empty()),
                ptr::from_mut(&mut cursor).cast(), host)
        }
    }

    pub fn new(scalar: &Allocation, packet: Option<&PacketAllocation>) -> Self {
        let packet_layout = packet.map_or(ColorLayout::default(), |allocation| allocation.layout);
        let FrameSaveLayout { saved, count, end } = unsafe { frame_save_layout(scalar.layout, packet_layout) };
        Self {
            saved,
            saved_count: count,
            end,
            tile_shared: std::collections::BTreeMap::new(),
            tile_private: std::collections::BTreeMap::new(),
        }
    }
}
