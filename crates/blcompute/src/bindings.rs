//! 域关系随实际数据所有权移动；宿主取出可写存储就放弃关系。
use crate::{Result, error};
use blanguage::ir::{PrefixCount, PrefixRelation, Program};

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
    /// order 索引本次所有旧绑定与按顺序追加的新绑定；不允许产生可写别名。
    pub fn rebind(mut self, order: &[usize], additional: Vec<D>) -> Result<Self> {
        self.remap(order, additional)?;
        Ok(self)
    }
    /// 只转移独立所有权；关系双方分到不同部分时，原关系随之放弃。
    pub fn partition(mut self, groups: &[&[usize]]) -> Result<Vec<Self>> {
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
    pub(crate) fn remap(&mut self, order: &[usize], additional: Vec<D>) -> Result<Vec<D>> {
        let total = self.datasets.len() + additional.len();
        let mut positions = vec![None; total];
        for (new, &old) in order.iter().enumerate() {
            let slot = positions
                .get_mut(old)
                .ok_or_else(|| error("域绑定重排索引越界"))?;
            if slot.is_some() {
                return Err(error("域绑定重排不能重复同一存储"));
            }
            *slot = Some(new);
        }
        self.domains = self
            .domains
            .iter()
            .filter_map(|r| {
                Some(PrefixRelation {
                    domain: positions[r.domain]?,
                    count: PrefixCount {
                        binding: positions[r.count.binding]?,
                        word: r.count.word,
                    },
                })
            })
            .collect();
        self.datasets.extend(additional);
        let mut identities: Vec<_> = (0..total).collect();
        let mut locations = identities.clone();
        for (new, &old) in order.iter().enumerate() {
            let at = locations[old];
            locations[identities[new]] = at;
            locations[old] = new;
            self.datasets.swap(new, at);
            identities.swap(new, at);
        }
        Ok(self.datasets.split_off(order.len()))
    }
    pub(crate) fn require(&self, program: &Program) -> Result<()> {
        for relation in program.prefix_requirements() {
            if !self.domains.contains(relation) {
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
    pub(crate) fn completed(&mut self, program: &Program) {
        self.domains
            .retain(|r| !program.buffers()[r.count.binding].access.writes());
        self.domains.extend(program.prefix_guarantees());
        self.domains.sort();
        self.domains.dedup();
    }
}
