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
#[derive(Default)]
pub(super) struct ColorLayout {
    end: u64,
    gpr: u32,
    xmm: u32,
}

#[derive(Default)]
pub(super) struct Allocation {
    locations: Vec<ColorPlace>,
    registers: usize,
    layout: ColorLayout,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct PacketPositions {
    rows: Span,
    registers: usize,
}
pub(super) struct PacketAllocation {
    pub(super) positions: PacketPositions,
    layout: ColorLayout,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct SavedRegister {
    pub offset: u32,
    pub register: u16,
    pub bank: u16,
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
    #[link_name = "bootstrap.frame_layout::saved_registers"]
    fn saved_registers(data_end: u64, gpr: u32, xmm: u32, saves: *mut SavedRegister) -> u64;
    #[link_name = "bootstrap.frame_layout::tile_frame"]
    fn tile_frame(data_end: u64, width: u64, steps: u32, context: *mut c_void, host: TileFrameHost) -> u64;
}

#[derive(Default)]
pub(super) struct Frame {
    pub saved: Vec<SavedRegister>,
    pub end: u64,
    pub tile_shared: std::collections::BTreeMap<SharedWord, u32>,
    pub tile_private: std::collections::BTreeMap<PrivateSlot, u32>,
}
pub(super) struct Graph {
    locations: Vec<ColorPlace>,
    private: BTreeSet<(usize, usize)>,
    pub(super) state: *mut c_void,
    registers: usize,
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
        let registers = scalars.len();
        let slots = registers + locals.last_key_value().map_or(0, |(end, _)| *end);
        let private = private_spans(code);
        let mut locations: Vec<_> = scalars.iter()
            .chain(locals.values().flat_map(|ty| ty.components.iter().map(|(scalar, _)| scalar)))
            .map(|scalar| ColorPlace::new(scalar.size() as u8, scalar.is_float()))
            .collect();
        for &(begin, end) in &private {
            for location in &mut locations[registers + begin..registers + end] {
                location.width = 0;
            }
        }
        let empty = BTreeSet::new();
        let (state, needs_r15) = graph::build(&mut locations, slots, code, scalars, &mut live, &empty);
        Self {
            locations,
            private,
            state,
            registers,
            needs_r15,
        }
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
    const UNSEEN: u32 = 0;
    const IMMEDIATE: u32 = 1;
    const GPR: u32 = 2;
    const XMM: u32 = 3;
    const STACK: u32 = 4;

    fn new(width: u8, floating: bool) -> Self {
        Self { value: 0, kind: Self::UNSEEN, width, floating: u8::from(floating) }
    }
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
        return PrivateDeclaration { offsets: Span::EMPTY, begin: 0, size: 0, alignment: 0 };
    };
    let ty = &state.locals[&end];
    PrivateDeclaration {
        offsets: Span::field(&ty.components, |component| &component.1),
        begin,
        size: ty.size,
        alignment: ty.align,
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.coloring::packet_colors_drop"]
    fn packet_colors_drop(places: Span);
    #[link_name = "bootstrap.coloring::graph_seal"]
    fn graph_seal(graph: *mut c_void);
    #[link_name = "bootstrap.coloring::graph_release"]
    fn graph_release(graph: *mut c_void);
    #[link_name = "bootstrap.coloring::color"]
    fn color(graph: *const c_void, count: usize, places: *mut ColorPlace, mode: u32) -> ColorLayout;
    #[link_name = "bootstrap.coloring::place_private"]
    fn place_private(
        places: *mut ColorPlace, data_end: u64, registers: usize, context: *mut c_void,
        next: unsafe extern "C" fn(*mut c_void) -> PrivateDeclaration,
    ) -> u64;
}
impl Allocation {
    pub fn new(
        locals: &std::collections::BTreeMap<usize, Ty>, graph: &mut Graph, tile_scope: bool,
    ) -> Self {
        // All entry and tile interference has now been collected. Packet coloring
        // borrows these same ordered rows after scalar coloring returns.
        let mut locations = std::mem::take(&mut graph.locations);
        let mut layout = unsafe {
            graph_seal(graph.state);
            color(graph.state, locations.len(), locations.as_mut_ptr(), u32::from(tile_scope))
        };
        let mut private = PrivateDeclarations { spans: graph.private.iter(), locals };
        layout.end = unsafe {
            place_private(locations.as_mut_ptr(), layout.end, graph.registers,
                std::ptr::from_mut(&mut private).cast(), next_private_declaration)
        };
        Self {
            locations,
            registers: graph.registers,
            layout,
        }
    }
    pub fn location(&self, slot: PrivateSlot) -> Location {
        self.locations[match slot {
            PrivateSlot::Register(r) => r,
            PrivateSlot::Local(s) => self.registers + s,
        }].scalar()
    }
}
impl PacketAllocation {
    // The result transfers the B color table and all its initialized rows.
    pub(super) unsafe fn from_b(registers: usize, locations: Span, layout: ColorLayout) -> Self {
        Self { positions: PacketPositions { rows: locations, registers }, layout }
    }
}
impl Drop for PacketAllocation {
    fn drop(&mut self) {
        unsafe { packet_colors_drop(self.positions.rows) };
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
        let ColorLayout { end: mut data_end, mut gpr, mut xmm } = scalar.layout;
        if let Some(packet) = packet {
            data_end = data_end.max(packet.layout.end);
            gpr |= packet.layout.gpr;
            xmm |= packet.layout.xmm;
        }
        let count = (gpr.count_ones() + xmm.count_ones()) as usize;
        let mut saved = Vec::with_capacity(count);
        // B writes one row for each bit in these same masks.
        let end = unsafe {
            let end = saved_registers(data_end, gpr, xmm, saved.as_mut_ptr());
            saved.set_len(count);
            end
        };
        Self {
            saved,
            end,
            tile_shared: std::collections::BTreeMap::new(),
            tile_private: std::collections::BTreeMap::new(),
        }
    }
}
