//! 按结构化 IR 活跃性着色；调用跨越值使用 ABI 保留寄存器，溢出槽按干涉复用。
use super::*;
use std::{collections::BTreeSet, ffi::c_void};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Location {
    Immediate(Bits),
    Gpr(u8),
    Xmm(u8),
    Stack(u32),
}
#[derive(Default)]
pub(super) struct Allocation {
    locations: Vec<ColorPlace>,
    registers: usize,
    data_end: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PacketPlace {
    Register(u8),
    Stack(u32),
}
pub(super) struct PacketAllocation {
    locations: Vec<ColorPlace>,
    registers: usize,
    data_end: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct SavedRegister {
    pub offset: u32,
    pub register: u16,
    pub bank: u16,
}

unsafe extern "C" {
    #[link_name = "bootstrap.frame_layout::saved_registers"]
    fn saved_registers(data_end: u64, gpr: u32, xmm: u32, saves: *mut SavedRegister) -> u64;
    #[link_name = "bootstrap.frame_layout::tile_frame"]
    fn tile_frame(data_end: u64, width: u64, steps: u32, shared: *mut u64,
        shared_count: usize, private: *mut u64, private_count: usize) -> u64;
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
    private: Vec<std::ops::Range<usize>>,
    addressable: BTreeSet<usize>,
    edges: Vec<BTreeSet<usize>>,
    affinities: Vec<BTreeSet<usize>>,
    weights: Vec<u64>,
    across_call: BTreeSet<usize>,
    outgoing_bytes: u64,
    registers: usize,
}
impl Graph {
    fn id(&self, slot: PrivateSlot) -> usize {
        match slot {
            PrivateSlot::Register(r) => r,
            PrivateSlot::Local(s) => self.registers + s,
        }
    }
    fn edge(&mut self, a: usize, b: usize) {
        if a != b && !self.addressable.contains(&a) && !self.addressable.contains(&b) {
            self.edges[a].insert(b);
            self.edges[b].insert(a);
        }
    }
    fn together(&mut self, live: &BTreeSet<PrivateSlot>) {
        let allocated: Vec<_> = live
            .iter()
            .map(|slot| self.id(*slot))
            .filter(|id| !self.addressable.contains(id))
            .collect();
        for (i, &a) in allocated.iter().enumerate() {
            for &b in &allocated[..i] {
                self.edge(a, b);
            }
        }
    }
    fn block(
        &mut self,
        code: &[Inst],
        registers: &[Scalar],
        mut live: BTreeSet<PrivateSlot>,
        break_live: &BTreeSet<PrivateSlot>,
        continue_live: &BTreeSet<PrivateSlot>,
        exit_live: &BTreeSet<PrivateSlot>,
        weight: u64,
    ) -> BTreeSet<PrivateSlot> {
        use PrivateSlot::{Local, Register};
        for inst in code.iter().rev() {
            match &(inst).kind {
                InstKind::If { condition, yes, no } => {
                    let a = self.block(
                        yes,
                        registers,
                        live.clone(),
                        break_live,
                        continue_live,
                        exit_live,
                        weight,
                    );
                    live = self.block(
                        no,
                        registers,
                        live,
                        break_live,
                        continue_live,
                        exit_live,
                        weight,
                    );
                    live.extend(a);
                    live.insert(Register(*condition));
                    self.weights[*condition] = self.weights[*condition].saturating_add(weight);
                }
                InstKind::Loop {
                    condition,
                    test,
                    body,
                } => {
                    let header = loop_live(
                        condition,
                        *test,
                        body,
                        &live,
                        break_live,
                        continue_live,
                        exit_live,
                    );
                    let nested = weight.saturating_mul(8);
                    let mut before_body = self.block(
                        body,
                        registers,
                        header.clone(),
                        &live,
                        &header,
                        exit_live,
                        nested,
                    );
                    before_body.extend(live);
                    before_body.insert(Register(*test));
                    self.weights[*test] = self.weights[*test].saturating_add(nested);
                    live = self.block(
                        condition,
                        registers,
                        before_body,
                        break_live,
                        continue_live,
                        exit_live,
                        nested,
                    );
                }
                InstKind::Scope(body) => {
                    live = self.block(
                        body,
                        registers,
                        live.clone(),
                        break_live,
                        continue_live,
                        &live,
                        weight,
                    )
                }
                InstKind::Raise(error) => {live.clear();live.insert(Register(*error));self.weights[*error]=self.weights[*error].saturating_add(weight);}
                InstKind::Exit => live = exit_live.clone(),
                InstKind::Break => live = break_live.clone(),
                InstKind::Continue => live = continue_live.clone(),
                _ => {
                    let transfer = match &(inst).kind {
                        InstKind::LoadLocal(dst, slot) => Some((Register(*dst), Local(*slot))),
                        InstKind::StoreLocal(slot, src) => Some((Local(*slot), Register(*src))),
                        InstKind::Cast { dst, src }
                            if operands::bit_copy(registers[*src], registers[*dst]) =>
                        {
                            Some((Register(*dst), Register(*src)))
                        }
                        _ => None,
                    }
                    .filter(|(dst, src)| {
                        !self.addressable.contains(&self.id(*dst))
                            && !self.addressable.contains(&self.id(*src))
                    });
                    let preferred = transfer.or_else(|| match &(inst).kind {
                        InstKind::Unary { dst, a, .. } => Some((Register(*dst), Register(*a))),
                        InstKind::Binary { dst, op, a, b } => {
                            let source = if let BinaryOp::Compare(comparison) = op {
                                let reverse = if registers[*a].is_float() {
                                    matches!(
                                        comparison,
                                        Comparison::Greater | Comparison::GreaterEqual
                                    )
                                } else {
                                    matches!(
                                        comparison,
                                        Comparison::Less | Comparison::GreaterEqual
                                    )
                                };
                                if reverse { *b } else { *a }
                            } else {
                                *a
                            };
                            Some((Register(*dst), Register(source)))
                        }
                        InstKind::Select { dst, b, .. } => Some((Register(*dst), Register(*b))),
                        InstKind::Math { dst, op, args }
                            if matches!(
                                op,
                                MathOp::Abs | MathOp::Min | MathOp::Max | MathOp::Sqrt
                            ) =>
                        {
                            Some((Register(*dst), Register(args[0])))
                        }
                        _ => None,
                    });
                    if let Some((dst, src)) = preferred {
                        let (dst, src) = (self.id(dst), self.id(src));
                        self.affinities[dst].insert(src);
                        self.affinities[src].insert(dst);
                    }
                    let literal = match &inst.kind {
                        InstKind::Constant(_, _, bits) => Some(*bits),
                        _ => None,
                    };
                    let mut definitions = Vec::new();
                    for dst in inst.results() {
                        unsafe {
                            record_definition(&mut self.locations[dst], u32::from(literal.is_some()),
                                literal.unwrap_or_default());
                        }
                        definitions.push(Register(dst));
                    }
                    if let InstKind::StoreLocal(slot, _) = &(inst).kind {
                        definitions.push(Local(*slot));
                    }
                    for dst in &definitions {
                        let id = self.id(*dst);
                        if !self.addressable.contains(&id) {
                            for other in &live {
                                // 复制后的两份值相同；其他定义和写入仍建立真实干涉。
                                if transfer != Some((*dst, *other)) {
                                    self.edge(id, self.id(*other));
                                }
                            }
                        }
                        self.weights[id] = self.weights[id].saturating_add(weight);
                    }
                    for dst in &definitions {
                        live.remove(dst);
                    }
                    if matches!(
                        &(inst).kind,
                        InstKind::Print { .. }
                            | InstKind::Call(_)
                            | InstKind::Math {
                                op: MathOp::Sin
                                    | MathOp::Cos
                                    | MathOp::Floor
                                    | MathOp::Exp
                                    | MathOp::Exp2
                                    | MathOp::Log
                                    | MathOp::Log2
                                    | MathOp::Pow,
                                ..
                            }
                    ) {
                        self.outgoing_bytes =
                            self.outgoing_bytes
                                .max(if let InstKind::Call(call) = &(inst).kind {
                                    super::external::outgoing_bytes(call)
                                } else {
                                    32
                                });
                        let preserved: Vec<_> = live.iter().map(|slot| self.id(*slot)).collect();
                        self.across_call.extend(preserved);
                    }
                    inst.all_operands(&mut |reg| {
                        let slot = Register(reg);
                        let id = self.id(slot);
                        self.weights[id] = self.weights[id].saturating_add(weight);
                        live.insert(slot);
                        true
                    });
                    for slot in inst.local_reads().into_iter().map(Local) {
                        let id = self.id(slot);
                        self.weights[id] = self.weights[id].saturating_add(weight);
                        live.insert(slot);
                    }
                }
            }
        }
        live
    }
}
impl Graph {
    pub(super) fn function_inputs(&mut self,inputs:&[Reg]) {
        // Entry values are simultaneous definitions supplied by the native caller.
        self.together(&inputs.iter().copied().map(PrivateSlot::Register).collect());
    }
    pub fn new(stage: &Stage) -> (Self, Vec<TilePhase<'_>>) {
        Self::with_live(stage, BTreeSet::new())
    }
    pub fn with_live(stage: &Stage, live: BTreeSet<PrivateSlot>) -> (Self, Vec<TilePhase<'_>>) {
        let slots = stage.registers.len() + stage.locals.last_key_value().map_or(0, |(end, _)| *end);
        let private = private_spans(&stage.code);
        let addressable: BTreeSet<_> = private.iter().flat_map(|span| span.clone())
            .map(|slot| stage.registers.len() + slot).collect();
        let locations = stage.registers.iter()
            .chain(stage.locals.values().flat_map(|ty| ty.components.iter().map(|(scalar, _)| scalar)))
            .enumerate()
            .map(|(id, scalar)| {
                let width = if addressable.contains(&id) { 0 } else { scalar.size() as u8 };
                ColorPlace::new(width, scalar.is_float())
            })
            .collect();
        let mut graph = Graph {
            locations,
            addressable,
            private,
            edges: vec![BTreeSet::new(); slots],
            affinities: vec![BTreeSet::new(); slots],
            weights: vec![0; slots],
            across_call: BTreeSet::new(),
            outgoing_bytes: 0,
            registers: stage.registers.len(),
        };
        let empty = BTreeSet::new();
        graph.together(&live);
        graph.block(
            &stage.code,
            &stage.registers,
            live,
            &empty,
            &empty,
            &empty,
            1,
        );
        let phases = if stage.has_tile_steps() { stage.tile_phases() } else { Vec::new() };
        // 实际阶段入口会恢复整组私有值，出口则保存整组；它们必须同时可取。
        for phase in &phases {
            graph.together(&phase.live_in);
            graph.together(&phase.preserved_out());
        }
        (graph, phases)
    }

}
#[repr(C)]
struct ColorNode {
    weight: u64,
    degree: u64,
    across_call: u64,
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
    fn packet(self) -> PacketPlace {
        match self.kind {
            Self::XMM => PacketPlace::Register(self.value as u8),
            Self::STACK => PacketPlace::Stack(self.value as u32),
            _ => unreachable!("unplaced packet"),
        }
    }
}


unsafe extern "C" {
    #[link_name = "bootstrap.coloring::color"]
    fn color(
        graph: *const c_void, count: u64, places: *mut ColorPlace, order: *mut [u64; 3],
        mode: u32, base: u64,
        node: unsafe extern "C" fn(*const c_void, u64) -> ColorNode,
        next: unsafe extern "C" fn(*const c_void, u64, bool, u64) -> u64,
    ) -> u64;
    #[link_name = "bootstrap.coloring::record_definition"]
    fn record_definition(place: *mut ColorPlace, literal: u32, bits: u64);
    #[link_name = "bootstrap.coloring::place_private"]
    fn place_private(places: *mut ColorPlace, count: usize, data_end: u64, bytes: u64,
        alignment: u64) -> u64;
}
unsafe extern "C" fn color_node(graph: *const c_void, id: u64) -> ColorNode {
    let graph = unsafe { &*graph.cast::<Graph>() };
    let id = id as usize;
    ColorNode {
        weight: graph.weights[id],
        degree: graph.edges[id].len() as u64,
        across_call: u64::from(graph.across_call.contains(&id)),
    }
}
unsafe extern "C" fn color_next(graph: *const c_void, id: u64, affinity: bool, after: u64) -> u64 {
    use std::ops::Bound::{Excluded, Unbounded};
    let graph = unsafe { &*graph.cast::<Graph>() };
    let set = if affinity { &graph.affinities[id as usize] } else { &graph.edges[id as usize] };
    let next = if after == graph.edges.len() as u64 { set.first() }
        else { set.range((Excluded(after as usize), Unbounded)).next() };
    next.map_or(graph.edges.len() as u64, |id| *id as u64)
}
fn color_locations(graph: &Graph, places: &mut [ColorPlace], mode: u32, base: u64) -> u64 {
    let mut order = Vec::<[u64; 3]>::with_capacity(places.len());
    // B borrows the graph and writes the final locations. The order rows are
    // scratch through coloring and byte layout and are never read by Rust.
    unsafe {
        color((graph as *const Graph).cast(), places.len() as u64, places.as_mut_ptr(),
            order.as_mut_ptr(), mode, base, color_node, color_next)
    }
}

impl Allocation {
    pub fn new(stage: &Stage, graph: &mut Graph) -> Self {
        let mut locations = std::mem::take(&mut graph.locations);
        let mut data_end = color_locations(
            graph, &mut locations, u32::from(stage.has_tile_scope()), graph.outgoing_bytes,
        );
        for span in &graph.private {
            let ty = &stage.locals[&span.end];
            let first = stage.registers.len() + span.start;
            let count = span.len().min(ty.components.len());
            let places = &mut locations[first..first + count];
            for (place, (_, offset)) in places.iter_mut().zip(&ty.components) {
                place.value = u64::from(*offset);
            }
            // B lays out only this declaration's borrowed location slice.
            data_end = unsafe {
                place_private(places.as_mut_ptr(), places.len(), data_end,
                    u64::from(ty.size), u64::from(ty.align.max(16)))
            };
        }
        Self {
            locations,
            registers: stage.registers.len(),
            data_end,
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
    pub fn new(stage: &Stage, graph: &mut Graph, address_only: &BTreeSet<Reg>) -> Self {
        // 原生范围游标已承担这些寻址，不生产没有值消费者的索引向量。
        for r in address_only {
            for n in std::mem::take(&mut graph.edges[*r]) {
                graph.edges[n].remove(r);
            }
            graph.weights[*r] = 0;
        }
        let mut locations = vec![ColorPlace::new(16, false); graph.edges.len()];
        let data_end = color_locations(graph, &mut locations, 2, 0);
        Self {
            locations,
            registers: stage.registers.len(),
            data_end,
        }
    }
    pub fn location(&self, slot: PrivateSlot) -> PacketPlace {
        self.locations[match slot {
            PrivateSlot::Register(r) => r,
            PrivateSlot::Local(s) => self.registers + s,
        }].packet()
    }
}
impl Frame {
    pub fn layout_tile(&mut self, stage: &Stage, phases: &[TilePhase<'_>]) -> u64 {
        let shared = stage.tile_layout(false);
        let private: BTreeSet<_> = phases.iter().flat_map(|phase| phase.live_out.iter().copied()).collect();
        let mut shared_positions: Vec<_> = shared.values().map(|&(_, slot, _)| slot as u64).collect();
        let mut private_positions: Vec<_> = private.iter()
            .map(|&slot| u64::from(stage.private_scalar(slot).size())).collect();
        // The frame owns the final maps. B borrows the two position arrays only
        // while replacing each entry with its stack offset.
        let end = unsafe {
            tile_frame(self.end, u64::from(stage.workgroup_width()), u32::from(!phases.is_empty()),
                shared_positions.as_mut_ptr(), shared_positions.len(),
                private_positions.as_mut_ptr(), private_positions.len())
        };
        self.tile_shared = shared.into_keys().zip(shared_positions.into_iter().map(|offset| offset as u32)).collect();
        self.tile_private = private.into_iter().zip(private_positions.into_iter().map(|offset| offset as u32)).collect();
        end
    }

    pub fn new(scalar: &Allocation, packet: Option<&PacketAllocation>) -> Self {
        let mut gpr = 0u32;
        let mut xmm = 0u32;
        for &location in &scalar.locations {
            match location.scalar() {
                // The allocator assigns only RBP, RSI, and RDI from the nonvolatile GPRs.
                Location::Gpr(register @ 5..=7) => gpr |= 1 << register,
                Location::Xmm(register) => xmm |= 1 << register,
                _ => {}
            }
        }
        let mut data_end = scalar.data_end;
        if let Some(packet) = packet {
            data_end = data_end.max(packet.data_end);
            for &place in &packet.locations {
                if let PacketPlace::Register(register) = place.packet() {
                    xmm |= 1 << register;
                }
            }
        }
        xmm &= 0xffc0; // XMM6–15 are nonvolatile.
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
