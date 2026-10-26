//! 按结构化 IR 活跃性着色；调用跨越值使用 ABI 保留寄存器，溢出槽按干涉复用。
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Location {
    Gpr(u8),
    Xmm(u8),
    Stack(u32),
}
#[derive(Default)]
pub(super) struct Allocation {
    locations: Vec<Location>,
    registers: usize,
    data_end: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PacketPlace {
    Register(u8),
    Stack(u32),
}
pub(super) struct PacketAllocation {
    locations: Vec<PacketPlace>,
    registers: usize,
    data_end: u64,
}
#[derive(Default)]
pub(super) struct Frame {
    pub saved_gpr: Vec<(u8, u32)>,
    pub saved_xmm: Vec<(u8, u32)>,
    pub end: u64,
}
pub(super) struct Graph {
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
            match inst {
                Inst::If { condition, yes, no } => {
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
                Inst::Loop {
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
                Inst::Scope(body) => {
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
                Inst::Exit => live = exit_live.clone(),
                Inst::Break => live = break_live.clone(),
                Inst::Continue => live = continue_live.clone(),
                _ => {
                    let transfer = match inst {
                        Inst::LoadLocal(dst, slot) => Some((Register(*dst), Local(*slot))),
                        Inst::StoreLocal(slot, src) => Some((Local(*slot), Register(*src))),
                        Inst::Cast { dst, src }
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
                    let preferred = transfer.or_else(|| match inst {
                        Inst::Unary { dst, a, .. } => Some((Register(*dst), Register(*a))),
                        Inst::Binary { dst, op, a, .. } if !matches!(op, BinaryOp::Compare(_)) => {
                            Some((Register(*dst), Register(*a)))
                        }
                        Inst::Math { dst, op, args }
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
                    let mut definitions: Vec<_> =
                        inst.results().into_iter().map(Register).collect();
                    if let Inst::StoreLocal(slot, _) = inst {
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
                        inst,
                        Inst::Print { .. }
                            | Inst::ExternalCall(_)
                            | Inst::Math {
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
                                .max(if let Inst::ExternalCall(call) = inst {
                                    super::external::Layout::new(call).bytes
                                } else {
                                    32
                                });
                        let preserved: Vec<_> = live.iter().map(|slot| self.id(*slot)).collect();
                        self.across_call.extend(preserved);
                    }
                    let mut uses: Vec<_> = inst.operands().into_iter().map(Register).collect();
                    uses.extend(inst.local_reads().into_iter().map(Local));
                    for slot in uses {
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
    pub fn new(stage: &Stage) -> Self {
        Self::with_live(stage, BTreeSet::new())
    }
    pub fn with_live(stage: &Stage, live: BTreeSet<PrivateSlot>) -> Self {
        let mut graph = Graph {
            addressable: private_spans(&stage.code)
                .into_iter()
                .flat_map(|r| r.map(|s| stage.registers.len() + s))
                .collect(),
            edges: vec![BTreeSet::new(); stage.registers.len() + stage.locals.len()],
            affinities: vec![BTreeSet::new(); stage.registers.len() + stage.locals.len()],
            weights: vec![0; stage.registers.len() + stage.locals.len()],
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
        if stage.has_tile_steps() {
            // 实际阶段入口会恢复整组私有值，出口则保存整组；它们必须同时可取。
            for phase in stage.tile_phases() {
                graph.together(&phase.live_in);
                graph.together(&phase.preserved_out());
            }
        }
        graph
    }
    fn allocate<P: Copy + Ord>(
        &self,
        candidates: impl Fn(usize, bool) -> Vec<P>,
        stack: impl Fn(u32) -> P,
    ) -> (Vec<P>, u32) {
        let mut order: Vec<_> = (0..self.edges.len()).collect();
        order.sort_by_key(|id| {
            (
                std::cmp::Reverse(self.weights[*id]),
                std::cmp::Reverse(self.edges[*id].len()),
                *id,
            )
        });
        let mut locations = vec![None; self.edges.len()];
        let mut spills = 0u32;
        for id in order {
            let used: BTreeSet<_> = self.edges[id]
                .iter()
                .filter_map(|n| locations[*n])
                .collect();
            let candidates = candidates(id, self.across_call.contains(&id));
            let preferred = self.affinities[id]
                .iter()
                .filter_map(|n| locations[*n])
                .find(|r| candidates.contains(r) && !used.contains(r));
            let location = if let Some(register) =
                preferred.or_else(|| candidates.into_iter().find(|r| !used.contains(r)))
            {
                register
            } else {
                let slot = (0..).find(|s| !used.contains(&stack(*s))).unwrap();
                spills = spills.max(slot + 1);
                stack(slot)
            };
            locations[id] = Some(location);
        }
        (locations.into_iter().map(Option::unwrap).collect(), spills)
    }
}
impl Allocation {
    pub fn new(stage: &Stage, graph: &Graph) -> Self {
        let scalars: Vec<_> = stage
            .registers
            .iter()
            .chain(&stage.locals)
            .copied()
            .collect();
        let base = graph.outgoing_bytes;
        let (mut locations, spills) = graph.allocate(
            |id, across_call| {
                if scalars[id] == Scalar::F32 {
                    (2..16)
                        .filter(|r| !across_call || *r >= 6)
                        .map(Location::Xmm)
                        .collect()
                } else {
                    [8, 9, 6, 7, 5]
                        .into_iter()
                        .filter(|r| !across_call || *r < 8)
                        .map(Location::Gpr)
                        .collect()
                }
            },
            |slot| Location::Stack((base + slot as u64 * 4) as u32),
        );
        let mut data_end = base as u64 + spills as u64 * 4;
        for span in private_spans(&stage.code) {
            data_end = data_end.div_ceil(16) * 16;
            for slot in span.clone() {
                locations[stage.registers.len() + slot] =
                    Location::Stack((data_end + (slot - span.start) as u64 * 4) as u32);
            }
            data_end += span.len() as u64 * 4;
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
        }]
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
        let (locations, spills) = graph.allocate(
            |_, _| (4..16).map(PacketPlace::Register).collect(),
            |slot| PacketPlace::Stack(slot * 16),
        );
        Self {
            locations,
            registers: stage.registers.len(),
            data_end: spills as u64 * 16,
        }
    }
    pub fn location(&self, slot: PrivateSlot) -> PacketPlace {
        self.locations[match slot {
            PrivateSlot::Register(r) => r,
            PrivateSlot::Local(s) => self.registers + s,
        }]
    }
}
impl Frame {
    pub fn new(scalar: &Allocation, packet: Option<&PacketAllocation>) -> Self {
        let mut saved_gpr = Vec::new();
        let mut saved_xmm = Vec::new();
        let mut used: BTreeSet<_> = scalar.locations.iter().copied().collect();
        let mut end = scalar.data_end;
        if let Some(packet) = packet {
            end = end.max(packet.data_end);
            used.extend(packet.locations.iter().filter_map(|place| match place {
                PacketPlace::Register(r) => Some(Location::Xmm(*r)),
                PacketPlace::Stack(_) => None,
            }));
        }
        end = end.div_ceil(8) * 8;
        for location in &used {
            if let Location::Gpr(r @ 5..=7) = location {
                saved_gpr.push((*r, end as u32));
                end += 8;
            }
        }
        end = end.div_ceil(16) * 16;
        for location in used {
            if let Location::Xmm(r @ 6..=15) = location {
                saved_xmm.push((r, end as u32));
                end += 16;
            }
        }
        Self {
            saved_gpr,
            saved_xmm,
            end,
        }
    }
}
