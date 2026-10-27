//! 域关系随实际数据所有权移动；宿主取出可写存储就放弃关系。
use crate::{Result, error};
use blanguage::ir::{PrefixCount, PrefixRelation, ProgramInterface};
use std::collections::BTreeMap;

/// 只读身份与绑定由存储所有者提供；原生执行入口仍只接受各自具体存储类型。
pub trait DomainStorage: Sized {
    #[doc(hidden)]
    fn read_identity(&self) -> Option<usize>;
    #[doc(hidden)]
    fn read_binding(&self) -> Option<Self>;
}
impl DomainStorage for crate::Dataset {
    fn read_identity(&self) -> Option<usize> {
        None
    }
    fn read_binding(&self) -> Option<Self> {
        None
    }
}

pub struct DomainBindings<D> {
    pub(crate) datasets: Vec<D>,
    pub(crate) domains: Vec<PrefixRelation>,
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
        self.domains
            .extend(other.domains.into_iter().map(|r| PrefixRelation {
                domain: r.domain + offset,
                count: PrefixCount {
                    binding: r.count.binding + offset,
                    word: r.count.word,
                },
            }));
    }
    pub fn datasets(&self) -> &[D] {
        &self.datasets
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
            let (domain_part, domain) = positions[r.domain];
            let (count_part, binding) = positions[r.count.binding];
            if domain_part == count_part {
                parts[domain_part].domains.push(PrefixRelation {
                    domain,
                    count: PrefixCount {
                        binding,
                        word: r.count.word,
                    },
                });
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
            for &domain in &positions[relation.domain] {
                for &binding in &positions[relation.count.binding] {
                    domains.push(PrefixRelation {
                        domain,
                        count: PrefixCount {
                            binding,
                            word: relation.count.word,
                        },
                    });
                }
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
    pub(crate) fn require(
        &self,
        program: &dyn ProgramInterface,
        binding: impl Fn(usize) -> usize,
    ) -> Result<()>
    where
        D: DomainStorage,
    {
        let same = |a: usize, b: usize| {
            a == b
                || self.datasets[a].read_identity().is_some_and(|identity| {
                    self.datasets.get(b).and_then(D::read_identity) == Some(identity)
                })
        };
        for relation in program.prefix_requirements() {
            if !self.domains.iter().any(|known| {
                same(known.domain, binding(relation.domain))
                    && known.count.word == relation.count.word
                    && same(known.count.binding, binding(relation.count.binding))
            }) {
                return Err(error(format!(
                    "程序 {} 缺少绑定 {} 与 {} 的有效域所有权",
                    program.name(),
                    program.buffers()[relation.domain].name,
                    program.buffers()[relation.count.binding].name
                )));
            }
        }
        Ok(())
    }
    pub(crate) fn completed(
        &mut self,
        program: &dyn ProgramInterface,
        binding: impl Fn(usize) -> usize,
    ) {
        self.domains.retain(|r| {
            !program
                .buffers()
                .iter()
                .enumerate()
                .any(|(i, b)| binding(i) == r.count.binding && b.access.writes())
        });
        self.domains
            .extend(program.prefix_guarantees().iter().map(|r| PrefixRelation {
                domain: binding(r.domain),
                count: PrefixCount {
                    binding: binding(r.count.binding),
                    word: r.count.word,
                },
            }));
        self.domains.sort();
        self.domains.dedup();
    }
}
