//! 域关系随实际数据所有权移动；宿主取出可写存储就放弃关系。
use crate::{Result, error};
use blanguage::ir::{ElementRange, ElementFact, ExecutionPlan, PrefixCount, ProgramInterface, Scalar};
use std::collections::{BTreeMap, BTreeSet};

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
        match self { Self::Prefix { domain, count, .. } => *domain == binding || count.binding == binding,
            Self::Element(range) => range.binding == binding }
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
    pub(crate) domains: Vec<DomainRelation>,
}
impl<D> From<Vec<D>> for DomainBindings<D> {
    fn from(datasets: Vec<D>) -> Self {
        Self {
            datasets,
            domains: Vec::new(),
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
        self.domains.extend(other.domains.into_iter().map(|r| r.mapped(|binding| Some(binding + offset)).unwrap()));
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
        for &binding in bindings {
            if binding >= self.datasets.len() {
                return Err(error("冻结域绑定索引越界"));
            }
        }
        self.datasets = self
            .datasets
            .into_iter()
            .enumerate()
            .map(|(binding, data)| {
                if bindings.contains(&binding) {
                    freeze(data)
                } else {
                    Ok(data)
                }
            })
            .collect::<Result<_>>()?;
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
        self.remap(&order, Vec::new())?;
        let mut data = self.datasets.into_iter();
        let mut parts = Vec::new();
        let mut positions = Vec::new();
        for (part, group) in groups.iter().enumerate() {
            parts.push(Self {
                datasets: data.by_ref().take(group.len()).collect(),
                domains: Vec::new(),
            });
            positions.extend((0..group.len()).map(|index| (part, index)));
        }
        for r in self.domains {
            let part = positions[r.domain()].0;
            if let Some(relation) = r.mapped(|binding| {
                let (owner, local) = positions[binding]; (owner == part).then_some(local)
            }) {
                parts[part].domains.push(relation);
            }
        }
        Ok(parts)
    }
    pub(crate) fn remap(&mut self, order: &[usize], additional: Vec<D>) -> Result<Vec<D>>
    where
        D: DomainStorage,
    {
        let total = self.datasets.len() + additional.len();
        let mut positions = vec![Vec::new(); total];
        let mut remaining = vec![0; total];
        for (new, &old) in order.iter().enumerate() {
            let slot = positions
                .get_mut(old)
                .ok_or_else(|| error("域绑定重排索引越界"))?;
            slot.push(new);
            remaining[old] += 1;
        }
        self.datasets.extend(additional);
        let mut shared: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (new, &old) in order.iter().enumerate() {
            if let Some(identity) = self.datasets[old].read_identity() {
                shared.entry(identity).or_default().push(new);
            }
        }
        for (old, data) in self.datasets.iter().enumerate() {
            if let Some(identity) = data.read_identity() {
                positions[old] = shared.get(&identity).cloned().unwrap_or_default();
            }
        }
        let mut domains = Vec::new();
        for relation in &self.domains {
            match relation {
                DomainRelation::Prefix { domain, offset, bytes, count } => for &domain in &positions[*domain] {
                    for &binding in &positions[count.binding] { domains.push(DomainRelation::Prefix {
                        domain, offset: *offset, bytes: *bytes, count: PrefixCount { binding, word: count.word },
                    }); }
                },
                DomainRelation::Element(range) => for &binding in &positions[range.binding] {
                    domains.push(DomainRelation::Element(ElementRange { binding, ..range.clone() }));
                },
            }
        }
        domains.sort();
        domains.dedup();
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
        self.domains = domains;
        Ok(slots.into_iter().flatten().collect())
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
        self.domains.sort();
        self.domains.dedup();
    }
    /// Borrow only facts contained in the actual logical argument fields.
    pub(crate) fn projected_relations(&self, program:&ExecutionPlan, physical:impl Fn(usize)->(usize,u32))->Vec<DomainRelation>
    where D:DomainStorage {
        let mut result=Vec::new();
        for (logical,buffer) in program.buffers().iter().enumerate() {
            let (source,start)=physical(logical);
            let end=u64::from(start)+u64::from(buffer.ty.size);
            for relation in &self.domains {
                if !same_storage(&self.datasets,source,relation.domain()) { continue; }
                match relation {
                    DomainRelation::Prefix{offset,bytes,count,..}
                        if *offset>=start && u64::from(*offset)+u64::from(*bytes)<=end => {
                        let offset=*offset-start;
                        for (target,buffer) in program.buffers().iter().enumerate() {
                            let (source,start)=physical(target);
                            if same_storage(&self.datasets,source,count.binding) && count.word>=start
                                && u64::from(count.word)+4<=u64::from(start)+u64::from(buffer.ty.size) {
                                result.push(DomainRelation::Prefix{domain:logical,offset,bytes:*bytes,
                                    count:PrefixCount{binding:target,word:count.word-start}});
                            }
                        }
                    },
                    DomainRelation::Element(range)
                        if range.offset>=start && u64::from(range.offset)+u64::from(range.scalar.size())<=end => {
                        result.push(DomainRelation::Element(ElementRange{binding:logical,offset:range.offset-start,..range.clone()}));
                    },
                    _=>{},
                }
            }
        }
        result.sort();result.dedup();result
    }
    /// 将真实生产者的共同字段保证并入本组原关系，不读取或修改载荷。
    ///
    /// # Safety
    /// binding是本组当前实际存储，offset／Scalar对应其真实标量字段。
    /// 全部元素已经初始化，值位于所声明的有序闭区间并集；非空存储不能交空集。
    /// 区间须为该Scalar的有效有序数值编码，端点及区间次序保持其原规范。
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
                binding: *binding, offset: *offset, scalar: Scalar::U32,
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
        known.intervals = known.intervals.iter().flat_map(|&(a, b)| range.intervals.iter().filter_map(move |&(c, d)| {
            let (start, end) = (a.max(c), b.min(d)); (start <= end).then_some((start, end))
        })).collect();
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
                    ElementFact::Range(required) => range.intervals.iter().all(|&(a, b)| required.intervals.iter().any(|&(c, d)| c <= a && b <= d)),
                    ElementFact::Index { target, .. } => range.intervals.iter().all(|&(_, b)| b < u64::from(datasets[binding(*target).0].element_count())),
                }
        });
        if !holds { return Err(error(format!("程序 {} 缺少绑定 {} 字段 {} 的已生产数值范围", program.name(), program.buffers()[logical].name, word))); }
    }
    Ok(())
}
pub(crate) fn invalidate(
    domains: &mut Vec<DomainRelation>,
    write_regions: &BTreeMap<usize, BTreeSet<(u32,u32)>>,
    binding: impl Fn(usize) -> (usize, u32),
) {
    domains.retain(|r| !write_regions.iter().any(|(&logical, written)| {
        let (physical, offset) = binding(logical);
        written.iter().any(|&(start, end)| r.written(physical, u64::from(offset) + u64::from(start), u64::from(offset) + u64::from(end)))
    }));
}
