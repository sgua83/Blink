//! 按结构化 IR 活跃性着色；调用跨越值使用 ABI 保留寄存器，溢出槽按干涉复用。
use super::*;
use crate::ir::dataflow::{Span, TileLayout};
use std::{collections::{BTreeMap, BTreeSet, btree_map}, ffi::c_void, ptr};

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

#[repr(C)]
struct TileFramePrivateSlot { key: PrivateSlot, bytes: u32, offset: u32 }

unsafe extern "C" {
    #[link_name = "bootstrap.frame_layout::frame_save_layout"]
    fn frame_save_layout(scalar: ColorLayout, packet: ColorLayout) -> FrameSaveLayout;
    #[link_name = "bootstrap.frame_layout::tile_frame"]
    fn tile_frame(data_end: u64, width: u64, steps: u32, shared: Span, private_slots: Span) -> u64;
}

#[derive(Default)]
pub(super) struct Frame {
    pub saved: SavedLayout,
    pub saved_count: usize,
    pub end: u64,
    tile_shared: TileLayout,
    tile_private: Vec<TileFramePrivateSlot>,
}
pub(super) struct Graph {
    pub(super) state: *mut c_void,
    pub(super) needs_r15: bool,
}
#[path = "allocation/graph.rs"]
mod graph;
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
        let (state, needs_r15) = graph::build(code, scalars, locals, &mut live);
        Self { state, needs_r15 }
    }

    pub(super) fn tile_interference(&mut self, phases: &[TilePhase<'_>]) {
        // 前一阶段的出站保存槽属于后一阶段的入口活跃集合。
        for phase in phases {
            graph::phase_interference(self, &phase.live_in);
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
struct LocalLayout {
    offsets: Span,
    size: u32,
    alignment: u32,
}

unsafe extern "C" fn local_layout(locals: *const c_void, end: usize) -> LocalLayout {
    let declarations = unsafe { &*locals.cast::<BTreeMap<usize, Ty>>() };
    let ty = &declarations[&end];
    LocalLayout {
        offsets: Span::field(&ty.components, |component| &component.1),
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
    fn finish_scalar(graph: *mut c_void, mode: u32, locals: *const c_void,
        local_layout: unsafe extern "C" fn(*const c_void, usize) -> LocalLayout) -> Allocation;
}
impl Allocation {
    pub fn new(
        locals: &std::collections::BTreeMap<usize, Ty>, graph: &mut Graph, tile_scope: bool,
    ) -> Self {
        // All entry and tile interference has now been collected. Packet coloring
        // borrows these same ordered rows after scalar coloring returns.
        unsafe { finish_scalar(graph.state, u32::from(tile_scope),
            ptr::from_ref(locals).cast(), local_layout) }
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
        workgroup: u32, phases: &[TilePhase<'_>], mut shared: TileLayout,
    ) -> u64 {
        // Multiple phase entries can restore the same physical private stream.
        let private: BTreeSet<_> = phases.iter().skip(1)
            .flat_map(|phase| phase.live_in.iter().copied()).collect();
        self.tile_private = private.into_iter().map(|key| TileFramePrivateSlot {
            key, bytes: key.scalar(registers, locals).size(), offset: 0,
        }).collect();
        let end = unsafe {
            tile_frame(self.end, u64::from(workgroup), u32::from(!phases.is_empty()),
                Span::mutable(&mut shared), Span::mutable(&mut self.tile_private))
        };
        self.tile_shared = shared;
        end
    }

    pub(super) fn shared_words(&self) -> Span {
        Span::slice(&self.tile_shared)
    }

    pub(super) fn private_offset(&self, key: PrivateSlot) -> u32 {
        let index = self.tile_private.binary_search_by_key(&key, |row| row.key).unwrap();
        self.tile_private[index].offset
    }

    pub fn new(scalar: &Allocation, packet: Option<&PacketAllocation>) -> Self {
        let packet_layout = packet.map_or(ColorLayout::default(), |allocation| allocation.layout);
        let FrameSaveLayout { saved, count, end } = unsafe { frame_save_layout(scalar.layout, packet_layout) };
        Self {
            saved,
            saved_count: count,
            end,
            tile_shared: Vec::new(),
            tile_private: Vec::new(),
        }
    }
}
