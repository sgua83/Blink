//! 域关系随实际数据所有权移动；宿主取出可写存储就放弃关系。
use crate::{Result, error};
use blanguage::ir::{ElementRange, ElementFact, ExecutionPlan, PrefixCount, ProgramInterface, Scalar};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
    sync::Arc,
};

/// 只读身份与绑定由存储所有者提供；原生执行入口仍只接受各自具体存储类型。
pub trait DomainStorage: Sized {
    #[doc(hidden)]
    fn read_identity(&self) -> Option<usize>;
    #[doc(hidden)]
    fn read_binding(&self) -> Option<Self>;
    #[doc(hidden)]
    fn element_count(&self) -> u32;
}
impl DomainStorage for crate::Dataset {
    fn element_count(&self) -> u32 { self.count() }
    fn read_identity(&self) -> Option<usize> {
        None
    }
    fn read_binding(&self) -> Option<Self> {
        None
    }
}

// 运行期引用原物理存储及其中的实际数据字段；IR中的domain仍是逻辑参数索引。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DomainRelation {
    Prefix {
    /// 当前原datasets中的实际槽索引；在Frame中就是原句柄。
    domain: usize,
    /// 完整记录内选中字段的字节偏移和声明类型字节数。
    offset: u32,
    bytes: u32,
    /// count.binding同样索引原datasets，count.word是记录内字节偏移。
    count: PrefixCount,
    },
    Element(ElementRange),
}
impl DomainRelation {
    pub fn domain(&self) -> usize {
        match self { Self::Prefix { domain, .. } => *domain, Self::Element(range) => range.binding }
    }
    pub(crate) fn references(&self, binding: usize) -> bool {
        self.binding_indices().any(|index| index == binding)
    }
    pub(crate) fn binding_indices(&self) -> impl Iterator<Item = usize> {
        let count = match self {
            Self::Prefix { count, .. } => Some(count.binding),
            Self::Element(_) => None,
        };
        std::iter::once(self.domain()).chain(count)
    }
    pub(crate) fn mapped(&self, binding: impl Fn(usize) -> Option<usize>) -> Option<Self> {
        Some(match self {
            Self::Prefix { domain, offset, bytes, count } => Self::Prefix {
                domain: binding(*domain)?, offset: *offset, bytes: *bytes,
                count: PrefixCount { binding: binding(count.binding)?, word: count.word },
            },
            Self::Element(range) => Self::Element(ElementRange { binding: binding(range.binding)?, ..range.clone() }),
        })
    }
    /// 已移出的关系仅重绑索引；原区间存储随同一关系移动。
    pub(crate) fn into_mapped(self, binding: impl Fn(usize) -> Option<usize>) -> Option<Self> {
        Some(match self {
            Self::Prefix { domain, offset, bytes, count } => Self::Prefix {
                domain: binding(domain)?, offset, bytes,
                count: PrefixCount { binding: binding(count.binding)?, word: count.word },
            },
            Self::Element(range) => Self::Element(ElementRange { binding: binding(range.binding)?, ..range }),
        })
    }
    pub(crate) fn written(&self, binding: usize, start: u64, end: u64) -> bool {
        let (owner, offset, bytes) = match self {
            Self::Prefix { count, .. } => (count.binding, count.word, 4),
            Self::Element(range) => (range.binding, range.offset, range.scalar.size()),
        };
        owner == binding && u64::from(offset) < end && start < u64::from(offset) + u64::from(bytes)
    }
    pub(crate) fn address_written(&self, binding: usize, offset: u64, bytes: u32, stride: u64, count: u32, record_stride: u32, records: u32) -> bool {
        let (owner, field, width, step, rows) = match self {
            Self::Prefix { count, .. } => (count.binding, count.word, 4, 0, 1),
            Self::Element(range) => (range.binding, range.offset, range.scalar.size(), record_stride, records),
        };
        if owner != binding || count == 0 || rows == 0 || bytes == 0 { return false; }
        let field = u128::from(field);
        let offset = u128::from(offset);
        let field_end = field + u128::from(rows - 1) * u128::from(step) + u128::from(width);
        let write_end = offset + u128::from(count - 1) * u128::from(stride) + u128::from(bytes);
        if field >= write_end || offset >= field_end { return false; }
        let (mut a, mut b) = (u128::from(step), u128::from(stride));
        while b != 0 { (a, b) = (b, a % b); }
        if a == 0 { return true; }
        let delta = (field % a + a - offset % a) % a;
        delta < u128::from(bytes) || a - delta < u128::from(width)
    }
}
pub struct DomainBindings<D> {
    pub(crate) datasets: Vec<D>,
    pub(crate) domains: DomainRelations,
}

/// 范围借用同阶段的输入事实，发生本地变更时取得自有关系容器。
pub(crate) enum DomainRelations {
    Owned(Vec<DomainRelation>),
    Shared(Arc<Vec<DomainRelation>>),
}

impl Default for DomainRelations {
    fn default() -> Self { Self::Owned(Vec::new()) }
}

impl Deref for DomainRelations {
    type Target = Vec<DomainRelation>;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(domains) => domains,
            Self::Shared(domains) => domains,
        }
    }
}

impl DerefMut for DomainRelations {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if let Self::Shared(domains) = self && Arc::get_mut(domains).is_none() {
            *self = Self::Owned((**domains).clone());
        }
        match self {
            Self::Owned(domains) => domains,
            Self::Shared(domains) => Arc::make_mut(domains),
        }
    }
}

impl Extend<DomainRelation> for DomainRelations {
    fn extend<T: IntoIterator<Item = DomainRelation>>(&mut self, items: T) {
        let mut items = items.into_iter().peekable();
        if items.peek().is_some() {
            self.deref_mut().extend(items);
        }
    }
}

impl DomainRelations {
    pub(crate) fn share(&mut self) -> Option<&Arc<Vec<DomainRelation>>> {
        if let Self::Owned(domains) = self && !domains.is_empty() {
            *self = Self::Shared(Arc::new(std::mem::take(domains)));
        }
        match self {
            Self::Shared(domains) if !domains.is_empty() => Some(domains),
            _ => None,
        }
    }

    pub(crate) fn into_vec(self) -> Vec<DomainRelation> {
        match self {
            Self::Owned(domains) => domains,
            Self::Shared(domains) => Arc::unwrap_or_clone(domains),
        }
    }

    pub(crate) fn retain(&mut self, keep: impl Fn(&DomainRelation) -> bool) {
        match self {
            Self::Owned(domains) => domains.retain(keep),
            Self::Shared(shared) => {
                if let Some(domains) = Arc::get_mut(shared) {
                    domains.retain(keep);
                } else if !shared.iter().all(&keep) {
                    *self = Self::Owned(shared.iter().filter(|relation| keep(relation)).cloned().collect());
                }
            }
        }
    }

    pub(crate) fn extract_matching<'a>(
        &'a mut self, matches: impl Fn(&DomainRelation) -> bool + 'a,
    ) -> impl Iterator<Item = DomainRelation> + 'a {
        let any = match self {
            Self::Owned(_) => true,
            Self::Shared(domains) => domains.iter().any(&matches),
        };
        let extracted = any.then(|| self.deref_mut().extract_if(.., move |r| matches(r)));
        extracted.into_iter().flatten()
    }

    pub(crate) fn sort_unique(&mut self) {
        if matches!(self, Self::Shared(_)) && self.windows(2).all(|pair| pair[0] < pair[1]) {
            return;
        }
        let domains = self.deref_mut();
        domains.sort();
        domains.dedup();
    }
}

struct RelationTargets<'a, D, T> {
    datasets: &'a [D],
    positions: Vec<Vec<T>>,
    shared: BTreeMap<usize, usize>,
}

impl<'a, D: DomainStorage, T> RelationTargets<'a, D, T> {
    fn new(datasets: &'a [D], targets: impl Iterator<Item = (usize, T)>) -> Self {
        let mut positions: Vec<_> = datasets.iter().map(|_| Vec::new()).collect();
        let mut shared = BTreeMap::new();
        for (old, target) in targets {
            let source = datasets[old].read_identity()
                .map_or(old, |identity| *shared.entry(identity).or_insert(old));
            positions[source].push(target);
        }
        Self { datasets, positions, shared }
    }

    fn source(&self, old: usize) -> Option<usize> {
        let source = match self.datasets[old].read_identity() {
            Some(identity) => *self.shared.get(&identity)?,
            None => old,
        };
        (!self.positions[source].is_empty()).then_some(source)
    }
}

struct ArgumentField {
    binding: usize,
    start: u32,
    end: u64,
}

impl ArgumentField {
    fn project(&self, offset: u32, width: u32) -> Option<(usize, u32)> {
        (offset >= self.start && u64::from(offset) + u64::from(width) <= self.end)
            .then(|| (self.binding, offset - self.start))
    }
}

impl<D> From<Vec<D>> for DomainBindings<D> {
    fn from(datasets: Vec<D>) -> Self {
        Self {
            datasets,
            domains: DomainRelations::default(),
        }
    }
}
impl<D> DomainBindings<D> {
    /// 合并独立存储与其域关系，不复制记录或签发新的计数关系。
    pub fn join(parts: impl IntoIterator<Item = Self>) -> Self {
        let mut joined = Vec::new().into();
        for part in parts {
            Self::append(&mut joined, part);
        }
        joined
    }
    pub(crate) fn append(&mut self, other: Self) {
        let offset = self.datasets.len();
        self.datasets.extend(other.datasets);
        self.domains.extend(other.domains.into_vec().into_iter().map(|r| r.into_mapped(|binding| Some(binding + offset)).unwrap()));
    }
    /// 共享借用原数据槽Vec，len包含已初始化空槽，capacity以D槽数计。
    /// 槽容量不包含元素内部拥有的载荷、分配器开销或OS页；零尺寸D不占槽字节。
    pub fn datasets(&self) -> &Vec<D> {
        &self.datasets
    }
    /// 借用原有效域关系Vec；len是当前关系数，capacity是可用关系槽数。
    /// 预留容量不产生关系或数据所有权；查询不复制、发布或重建关系。
    pub fn domain_relations(&self) -> &Vec<DomainRelation> {
        &self.domains
    }
    /// 取出存储后，宿主可任意改写；已有有效域不再随之交付。
    pub fn into_datasets(self) -> Vec<D> {
        self.datasets
    }
    pub(crate) fn freeze_with(
        mut self,
        bindings: &[usize],
        mut freeze: impl FnMut(D) -> Result<D>,
    ) -> Result<Self> {
        let len = self.datasets.len();
        if bindings.iter().any(|&binding| binding >= len) {
            return Err(error("冻结域绑定索引越界"));
        }
        let mut bindings = Cow::Borrowed(bindings);
        if !bindings.is_sorted() {
            bindings.to_mut().sort_unstable();
        }
        for selected in bindings.chunk_by(|a, b| a == b) {
            let binding = selected[0];
            let data = self.datasets.swap_remove(binding);
            self.datasets.push(freeze(data)?);
            // 归还临时移到本槽的尾项，保持绑定与域关系的索引。
            self.datasets.swap(binding, len - 1);
        }
        Ok(self)
    }
    /// order 索引本次所有旧绑定与按顺序追加的新绑定；不允许产生可写别名。
    pub fn rebind(mut self, order: &[usize], additional: Vec<D>) -> Result<Self>
    where
        D: DomainStorage,
    {
        self.remap(order, additional)?;
        Ok(self)
    }
    /// 唯一存储只移动一次，冻结存储可共享；每个部分只持有自己实际绑定的关系。
    pub fn partition(mut self, groups: &[&[usize]]) -> Result<Vec<Self>>
    where
        D: DomainStorage,
    {
        let order: Vec<_> = groups.iter().flat_map(|g| g.iter().copied()).collect();
        let mut parts: Vec<Self> = groups.iter().map(|_| Vec::new().into()).collect();
        self.remap_into(&order, Vec::new(), groups.iter().map(|group| group.len()),
            |part, relation| parts[part].domains.push(relation))?;
        let mut data = self.datasets.into_iter();
        for (part, group) in parts.iter_mut().zip(groups) {
            part.datasets = data.by_ref().take(group.len()).collect();
            part.domains.sort_unstable();
        }
        Ok(parts)
    }
    pub(crate) fn remap(&mut self, order: &[usize], additional: Vec<D>) -> Result<()>
    where
        D: DomainStorage,
    {
        let mut domains = Vec::new();
        self.remap_into(order, additional, std::iter::once(order.len()),
            |_, relation| domains.push(relation))?;
        domains.sort_unstable();
        self.domains = DomainRelations::Owned(domains);
        Ok(())
    }
    fn remap_into(
        &mut self,
        order: &[usize],
        additional: Vec<D>,
        groups: impl Iterator<Item = usize>,
        mut emit: impl FnMut(usize, DomainRelation),
    ) -> Result<()>
    where
        D: DomainStorage,
    {
        let total = self.datasets.len() + additional.len();
        let mut remaining = vec![0; total];
        for &old in order {
            let count = remaining
                .get_mut(old)
                .ok_or_else(|| error("域绑定重排索引越界"))?;
            *count += 1;
        }
        self.datasets.extend(additional);
        let relations = std::mem::take(&mut self.domains).into_vec();
        if !relations.is_empty() && !order.is_empty() {
            let targets = groups.enumerate().flat_map(|(part, len)| (0..len).map(move |slot| (part, slot)));
            let targets = RelationTargets::new(&self.datasets, order.iter().copied().zip(targets));
            // Resolve aliases before expanding relations into the receiving groups.
            let mut relations: Vec<_> = relations.into_iter()
                .filter_map(|relation| relation.into_mapped(|old| targets.source(old))).collect();
            relations.sort_unstable();
            relations.dedup();
            for relation in relations {
                match relation {
                    DomainRelation::Prefix { domain, offset, bytes, count } => {
                        let mut counts = targets.positions[count.binding].as_slice();
                        for targets in targets.positions[domain].chunk_by(|a,b| a.0 == b.0) {
                            let part = targets[0].0;
                            let start = counts.partition_point(|&(group, _)| group < part);
                            let end = counts.partition_point(|&(group, _)| group <= part);
                            for &(_, domain) in targets {
                                for &(_, binding) in &counts[start..end] {
                                    emit(part, DomainRelation::Prefix {
                                        domain, offset, bytes, count: PrefixCount { binding, word: count.word },
                                    });
                                }
                            }
                            counts = &counts[end..];
                        }
                    },
                    DomainRelation::Element(range) => for &(part, binding) in &targets.positions[range.binding] {
                        emit(part, DomainRelation::Element(ElementRange { binding, ..range.clone() }));
                    },
                }
            }
        }
        let mut slots: Vec<_> = std::mem::take(&mut self.datasets)
            .into_iter()
            .map(Some)
            .collect();
        let mut datasets = Vec::with_capacity(order.len());
        for &old in order {
            let data = if remaining[old] == 1 {
                slots[old].take().unwrap()
            } else {
                slots[old]
                    .as_ref()
                    .unwrap()
                    .read_binding()
                    .ok_or_else(|| error("域绑定重排不能重复可写存储"))?
            };
            datasets.push(data);
            remaining[old] -= 1;
        }
        self.datasets = datasets;
        Ok(())
    }
    pub(crate) fn completed(
        &mut self,
        program: &ExecutionPlan,
        binding: impl Fn(usize) -> (usize, u32),
    ) where D: DomainStorage {
        invalidate(&mut self.domains, &program.write_regions, &binding);
        self.produced(program, binding);
    }
    pub(crate) fn produced(
        &mut self,
        program: &dyn ProgramInterface,
        binding: impl Fn(usize) -> (usize, u32),
    ) where D: DomainStorage {
        self.domains
            .extend(program.prefix_guarantees().iter().map(|r| DomainRelation::Prefix {
                domain: binding(r.domain).0,
                offset: binding(r.domain).1+r.domain_layout(program.buffers()).0,
                bytes: r.domain_layout(program.buffers()).1,
                count: PrefixCount {
                    binding: binding(r.count.binding).0,
                    word: r.count.word + binding(r.count.binding).1,
                },
            }));
        for range in produced_ranges(program.element_guarantees(), &binding,
            |target| self.datasets[binding(target).0].element_count()) {
            produced_element(&mut self.domains, range);
        }
        self.domains.sort_unique();
    }
    /// Borrow only facts contained in the actual logical argument fields.
    pub(crate) fn projected_relations(
        &self,
        program: &ExecutionPlan,
        physical: impl Fn(usize) -> (usize, u32),
    ) -> Vec<DomainRelation>
    where
        D: DomainStorage,
    {
        if self.domains.is_empty() {
            return Vec::new();
        }
        let fields = program.buffers().iter().enumerate().map(|(binding, buffer)| {
            let (source, start) = physical(binding);
            (source, ArgumentField { binding, start, end: u64::from(start) + u64::from(buffer.ty.size) })
        });
        let targets = RelationTargets::new(&self.datasets, fields);
        let mut prefixes = BTreeSet::new();
        let mut elements = BTreeSet::new();
        let mut counts = Vec::new();
        let mut result = Vec::new();
        for relation in self.domains.iter() {
            let Some(source) = targets.source(relation.domain()) else { continue; };
            match relation {
                DomainRelation::Prefix { offset, bytes, count, .. } => {
                    let Some(count_source) = targets.source(count.binding) else { continue; };
                    if !prefixes.insert((source, *offset, *bytes, count_source, count.word)) {
                        continue;
                    }
                    counts.clear();
                    counts.extend(targets.positions[count_source].iter()
                        .filter_map(|field| field.project(count.word, 4)));
                    let domains = targets.positions[source].iter()
                        .filter_map(|field| field.project(*offset, *bytes));
                    for (domain, offset) in domains {
                        for &(binding, word) in &counts {
                            result.push(DomainRelation::Prefix {
                                domain, offset, bytes: *bytes, count: PrefixCount { binding, word },
                            });
                        }
                    }
                },
                DomainRelation::Element(range) => {
                    // Compare the original interval storage before cloning a receiving field's fact.
                    let key = (source, range.offset, range.scalar, range.intervals.as_slice(), range.known_bits);
                    if !elements.insert(key) {
                        continue;
                    }
                    let fields = targets.positions[source].iter()
                        .filter_map(|field| field.project(range.offset, range.scalar.size()));
                    for (binding, offset) in fields {
                        result.push(DomainRelation::Element(ElementRange { binding, offset, ..range.clone() }));
                    }
                }
            }
        }
        result.sort_unstable();
        result
    }
    /// 将真实生产者的共同字段保证并入本组原关系，不读取或修改载荷。
    ///
    /// # Safety
    /// binding是本组当前实际存储，offset／Scalar对应其真实标量字段。
    /// 全部元素已经初始化，值同时满足所声明的有序闭区间并集及原始固定位；非空存储不能交空集。
    /// 区间须为该Scalar的有效有序数值编码，端点及区间次序保持其原规范；固定位仅用于整数，value只含mask内该Scalar有效位。
    /// 保证来自编译器证明或调用者信任的真实实现，不能仅从任意字节推定。
    /// 实际写已完成或由原存储执行依赖覆盖后续消费者；本调用不提交或等待。
    /// 原存储及最后消费者的寿命／访问约束继续成立，外部别名不得绕过原写入撤回。
    pub unsafe fn produced_element(&mut self, range: ElementRange) {
        produced_element(&mut self.domains, range);
    }
}

pub(crate) fn produced_ranges(
    facts: &[ElementFact],
    binding: impl Fn(usize) -> (usize, u32),
    count: impl Fn(usize) -> u32,
) -> impl Iterator<Item = ElementRange> {
    facts.iter().map(move |fact| {
        let range = match fact {
            ElementFact::Range(range) => range.clone(),
            ElementFact::Index { binding, offset, target } => ElementRange {
                binding: *binding, offset: *offset, scalar: Scalar::U32, known_bits:(0,0),
                intervals: count(*target).checked_sub(1).map(|last| (0, u64::from(last))).into_iter().collect(),
            },
        };
        let (source, offset) = binding(range.binding);
        ElementRange { binding: source, offset: range.offset + offset, ..range }
    })
}
pub(crate) fn produced_element(domains: &mut Vec<DomainRelation>, range: ElementRange) {
    if let Some(known) = domains.iter_mut().find_map(|relation| match relation {
        DomainRelation::Element(known) if known.binding == range.binding && known.offset == range.offset
            && known.scalar == range.scalar => Some(known),
        _ => None,
    }) {
        known.intersect(&range);
    } else {
        domains.push(DomainRelation::Element(range));
    }
}

fn same_storage<D:DomainStorage>(datasets:&[D],a:usize,b:usize)->bool {
    a==b || datasets[a].read_identity().is_some_and(|identity|
        datasets.get(b).and_then(D::read_identity)==Some(identity))
}
pub(crate) fn require<D: DomainStorage>(
    datasets: &[D],
    domains: &[DomainRelation],
    program: &dyn ProgramInterface,
    binding: impl Fn(usize) -> (usize, u32),
) -> Result<()> {
    let same = |a: usize, b: usize| same_storage(datasets,a,b);
    for relation in program.prefix_requirements() {
        if !domains.iter().any(|known| {
            matches!(known, DomainRelation::Prefix { domain, offset, bytes, count }
                if same(*domain, binding(relation.domain).0)
                && u64::from(*offset) <= u64::from(binding(relation.domain).1)+u64::from(relation.domain_layout(program.buffers()).0)
                && u64::from(binding(relation.domain).1)+u64::from(relation.domain_layout(program.buffers()).0)+u64::from(relation.domain_layout(program.buffers()).1)<=u64::from(*offset)+u64::from(*bytes)
                && u64::from(count.word) == u64::from(relation.count.word) + u64::from(binding(relation.count.binding).1)
                && same(count.binding, binding(relation.count.binding).0))
        }) {
            return Err(error(format!(
                "程序 {} 缺少绑定 {} 与 {} 的有效域所有权",
                program.name(),
                program.buffers()[relation.domain].name,
                program.buffers()[relation.count.binding].name
            )));
        }
    }
    for requirement in program.element_requirements() {
        let (logical, word, scalar) = match requirement {
            ElementFact::Range(range) => (range.binding, range.offset, range.scalar),
            ElementFact::Index { binding, offset, .. } => (*binding, *offset, Scalar::U32),
        };
        let (source, offset) = binding(logical);
        if datasets[source].element_count() == 0 { continue; }
        let holds = domains.iter().any(|known| {
            let DomainRelation::Element(range) = known else { return false; };
            same(range.binding, source) && u64::from(range.offset) == u64::from(offset) + u64::from(word)
                && range.scalar == scalar && match requirement {
                    ElementFact::Range(required) => range.within(required),
                    ElementFact::Index { target, .. } => {
                        let allowed=datasets[binding(*target).0].element_count().checked_sub(1).map(|last|(0,u64::from(last)));
                        range.intervals.iter().all(|&actual|scalar.covers_range(actual,range.known_bits,allowed.as_slice(),(0,0)))
                    },
                }
        });
        if !holds { return Err(error(format!("程序 {} 缺少绑定 {} 字段 {} 的已生产数值范围", program.name(), program.buffers()[logical].name, word))); }
    }
    Ok(())
}
pub(crate) fn invalidate(
    domains: &mut DomainRelations,
    write_regions: &BTreeMap<usize, BTreeSet<(u32,u32)>>,
    binding: impl Fn(usize) -> (usize, u32),
) {
    domains.retain(|r| !write_regions.iter().any(|(&logical, written)| {
        let (physical, offset) = binding(logical);
        written.iter().any(|&(start, end)| r.written(physical, u64::from(offset) + u64::from(start), u64::from(offset) + u64::from(end)))
    }));
}
