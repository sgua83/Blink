//! 显式传输闭合资源图；一次操作内保留共享，不解析源平台地址或维护跨操作缓存。
use super::*;
use std::collections::BTreeMap;

impl Device {
    pub(super) fn upload_graph(
        &self,
        data: &Dataset,
        memo: &mut BTreeMap<usize, ReadDataset>,
    ) -> Result<GpuDataset> {
        let mut sources = Vec::with_capacity(data.views.sources.len());
        for source in &data.views.sources {
            let key = source.identity();
            let target = if let Some(target) = memo.get(&key) {
                target.clone()
            } else {
                let target = self.upload_graph(source, memo)?.freeze();
                memo.insert(key, target.clone());
                target
            };
            sources.push(target);
        }
        let mut target =
            GpuDataset::new(self.inner.clone(), data.ty.clone(), data.stride, data.count)?;
        target.storage.views = Views {
            sources,
            bindings: data.views.bindings.clone(),
        };
        self.upload_payload(&mut target, &data.words)?;
        Ok(target)
    }

    pub(super) fn upload_payload(&self, target: &mut GpuDataset, words: &[u32]) -> Result<()> {
        if words.is_empty() {
            return Ok(());
        }
        let mut commands = Commands::new(self.inner.clone())?;
        let mut staging = Vec::new();
        self.stage_payload(target, words, &mut commands, &mut staging)?;
        target.last_use = Some(self.submit_transfer(commands, staging)?);
        Ok(())
    }
    pub(super) fn stage_payload(
        &self,
        target: &GpuDataset,
        words: &[u32],
        commands: &mut Commands,
        staging: &mut Vec<Allocation>,
    ) -> Result<()> {
        let words_per_page = target.page_records() as usize * target.stride as usize / 4;
        let fields: Vec<_> = target
            .ty
            .view_fields()
            .iter()
            .map(|(offset, _)| *offset)
            .collect();
        let headers: Vec<_> = target
            .views
            .sources
            .iter()
            .map(|source| {
                let address = source.0.allocation.address();
                [address as u32, (address >> 32) as u32, source.0.count]
            })
            .collect();
        for (index, (page, words)) in target
            .pages
            .iter()
            .zip(words.chunks(words_per_page))
            .enumerate()
        {
            self.stage_page(
                page,
                words,
                |mapped| {
                    target.views.encode(
                        &fields,
                        target.stride as usize / 4,
                        index * target.page_records() as usize,
                        mapped,
                        &headers,
                    );
                },
                commands,
                staging,
            )?;
        }
        Ok(())
    }
    pub(super) fn stage_page(
        &self,
        target: &Allocation,
        words: &[u32],
        encode: impl FnOnce(&mut [u32]),
        commands: &mut Commands,
        staging: &mut Vec<Allocation>,
    ) -> Result<()> {
        if words.is_empty() {
            return Ok(());
        }
        let source = Allocation::new(self.inner.clone(), words.len() as u64 * 4, 1, 6)?;
        source.write_with(words, encode)?;
        let copy = VkBufferCopy {
            srcOffset: 0,
            dstOffset: 0,
            size: words.len() as u64 * 4,
        };
        unsafe {
            (self.inner.functions.vkCmdCopyBuffer)(
                commands.command,
                source.buffer,
                target.buffer,
                1,
                &copy,
            );
        }
        staging.push(source);
        Ok(())
    }
    pub(super) fn submit_transfer(
        &self,
        mut commands: Commands,
        staging: Vec<Allocation>,
    ) -> Result<Rc<Execution>> {
        commands.submit()?;
        Ok(Execution::new(
            ExecutionResources::Transfer {
                commands,
                _staging: staging,
            },
            Vec::new(),
        ))
    }

    pub(super) fn download_graph(
        &self,
        data: &GpuDataset,
        memo: &mut BTreeMap<usize, crate::ReadDataset>,
    ) -> Result<Dataset> {
        let mut sources = Vec::with_capacity(data.views.sources.len());
        for source in &data.views.sources {
            let key = Rc::as_ptr(&source.0) as usize;
            let target = if let Some(target) = memo.get(&key) {
                target.clone()
            } else {
                let target = self.download_graph(&source.0, memo)?.freeze()?;
                memo.insert(key, target.clone());
                target
            };
            sources.push(target);
        }
        let mut target = Dataset {
            ty: data.ty.clone(),
            stride: data.stride,
            count: data.count,
            words: self.read_payload(data)?,
            views: Views::default(),
        };
        // 原生实例尾部不是可移植数据；主机只接收 B 声明的矩阵。
        if data.instances.is_some() {
            for record in target.words.chunks_exact_mut(16) {
                record[12..16].fill(0);
            }
        }
        target.attach_views(Views {
            sources,
            bindings: data.views.bindings.clone(),
        });
        Ok(target)
    }
}
