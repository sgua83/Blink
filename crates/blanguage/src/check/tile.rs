//! 输入捕获与中间发布共享逻辑块；块内索引能力不能变成全局可写所有权。
use super::*;
impl Builder<'_> {
    pub(super) fn tile_inputs(&mut self, body: &[Stmt]) -> Result<usize> {
        let mut consumed = 0;
        for statement in body {
            let StmtKind::Let(
                annotation,
                name,
                Expr {
                    kind: ExprKind::Call(call, args),
                    ..
                },
            ) = &statement.kind
            else {
                break;
            };
            if call != "tile_input" {
                break;
            }
            if annotation.is_some() || args.len() != 2 {
                return self.fail(
                    statement.span,
                    "B198",
                    "使用 let name = tile_input(dataset 或字段, 正整数宽度)",
                );
            }
            if self.env.contains_key(name) {
                return self.fail(statement.span, "B125", "tile 输入名称不能覆盖现有绑定");
            }
            let width = self.tile_width(&args[1])?;
            let (source, word, ty) = self.dataset_field(&args[0])?;
            if !self.view_access[&source].reads()
                || self.view_access[&source] == Access::Atomic
                || ty.contains_views()
                || ty.components.is_empty()
            {
                return self.fail(
                    args[0].span,
                    "B200",
                    "tile 输入需要普通可读值，不能捕获 write／atomic 数据或资源 view",
                );
            }
            if self.program.buffers[source]
                .count
                .zip(self.program.buffers[self.domain].count)
                .is_some_and(|(a, b)| a < b)
            {
                return self.fail(args[0].span, "B200", "tile 源容量不能小于阶段域容量");
            }
            let tile = if let Some(index) = self.tiles.iter().position(|t| {
                t.source
                    == TileOrigin::Bound(TileSource::Input {
                        binding: source,
                        word,
                    })
                    && t.ty == ty
            }) {
                index
            } else {
                let index = self.tiles.len();
                self.tiles.push(CheckedTile {
                    source: TileOrigin::Bound(TileSource::Input {
                        binding: source,
                        word,
                    }),
                    ty,
                    width: TileWidth::Fixed(width),
                    access: Access::Value,
                });
                index
            };
            self.bind_tile(name, tile);
            consumed += 1;
        }
        Ok(consumed)
    }
    pub(super) fn tile_publish(
        &mut self,
        annotated: bool,
        name: &str,
        args: &[Expr],
        span: Span,
        atomic: bool,
    ) -> Result<()> {
        if annotated || !self.is_cooperative_root() || !matches!(args.len(), 1 | 2) {
            return self.fail(
                span,
                "B203",
                "使用协作根块的 let name = tile_publish／tile_atomic(值[, 正整数宽度])",
            );
        }
        let width = if args.len() == 2 {
            if self.cooperative_root.is_some() {
                return self.fail(
                    span,
                    "B209",
                    "cooperative 函数继承借用块范围，发布不能指定固定宽度",
                );
            }
            TileWidth::Fixed(self.tile_width(&args[1])?)
        } else {
            self.tiles
                .first()
                .map(|t| t.width)
                .ok_or_else(|| Diagnostic {
                    source: self.definition.source.clone(),
                    span,
                    code: "B209",
                    message: "省略发布宽度需要既有 tile 范围".into(),
                })?
        };
        let value = self.expr(&args[0])?;
        if value.ty.contains_views() || value.ty.components.is_empty() {
            return self.fail(
                args[0].span,
                "B204",
                "tile 发布需要普通值，不能发布资源 view 或能力句柄",
            );
        }
        let tile = self.tiles.len();
        let values = value
            .ty
            .components
            .iter()
            .zip(&value.regs)
            .map(|((_, byte), r)| (byte / 4, *r))
            .collect();
        self.tiles.push(CheckedTile {
            source: TileOrigin::Bound(TileSource::Published),
            ty: value.ty,
            width,
            access: if atomic {
                Access::Atomic
            } else {
                Access::Value
            },
        });
        self.code
            .push(Inst::TilePublish(Publication { tile, values }));
        self.bind_tile(name, tile);
        Ok(())
    }
    fn tile_width(&mut self, expr: &Expr) -> Result<u32> {
        let value = self.expr(expr)?;
        let Some(bits) = value.constant else {
            return self.fail(expr.span, "B199", "tile 宽度需要编译期正 u32 值");
        };
        let width = bits[0];
        if value.ty != Ty::scalar(Scalar::U32)
            || width == 0
            || self
                .tiles
                .first()
                .is_some_and(|t| t.width != TileWidth::Fixed(width))
        {
            return self.fail(
                expr.span,
                "B199",
                "一个阶段的 tile 必须使用相同的非零 u32 宽度",
            );
        }
        Ok(width)
    }
    pub(super) fn tile_collective(
        &mut self,
        op: TileOp,
        args: &[Expr],
        span: Span,
    ) -> Result<Value> {
        if !self.is_cooperative_root() || args.len() != 1 {
            return self.fail(
                span,
                "B205",
                "tile 集合运算需要协作根块的 let／值类型声明及一个 tile 参数",
            );
        }
        let TileView {
            tile, ty, words, ..
        } = self.tile_view(&args[0])?;
        if self.tiles[tile].access == Access::Atomic {
            return self.fail(
                args[0].span,
                "B214",
                "atomic tile 需要先显式原子读取并发布不可变值，再参与集合运算",
            );
        }
        if let TileOrigin::Bound(TileSource::Input { binding, .. }) = self.tiles[tile].source {
            self.reads.insert(binding);
            self.cross_access.insert(binding);
        }
        if !ty.is_numeric() || !op.operator().accepts(ty.components[0].0) {
            return self.fail(
                args[0].span,
                "B206",
                "tile 集合运算需要数值标量／向量或其字段，按位操作只接受 u32/i32；不接受 bool、记录或资源",
            );
        }
        let regs: Vec<_> = ty.components.iter().map(|(s, _)| self.reg(*s)).collect();
        self.code.push(Inst::TileCollective(TileCollective {
            op,
            tile,
            results: words.into_iter().zip(regs.iter().copied()).collect(),
        }));
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs,
            uniform: false,
            constant: None,
        })
    }
    pub(super) fn tile_view(&self, expr: &Expr) -> Result<TileView> {
        match &expr.kind {
            ExprKind::Name(name) => match self.env.get(name) {
                Some(Binding::Tile(view)) => Ok(view.clone()),
                _ => self.fail(expr.span, "B206", "需要输入／发布的 tile 或其字段借用"),
            },
            ExprKind::Member(base, field) => {
                let view = self.tile_view(base)?;
                let (ty, indices) = self.member(&view.ty, field, expr.span)?;
                Ok(TileView {
                    tile: view.tile,
                    ty,
                    words: indices.iter().map(|i| view.words[*i]).collect(),
                    length: view.length,
                })
            }
            _ => self.fail(expr.span, "B206", "需要输入／发布的 tile 或其字段借用"),
        }
    }
    pub(super) fn bind_tile(&mut self, name: &str, tile: usize) {
        let ty = self.tiles[tile].ty.clone();
        let words = ty.components.iter().map(|(_, byte)| byte / 4).collect();
        let length = bounds::Proof::unsigned(1, self.tile_width_bound())
            .with_domains(Domains::from([IndexDomain::TileLength]))
            .with_identity(ValueIdentity::TileLength);
        self.env.insert(
            name.into(),
            Binding::Tile(TileView {
                tile,
                ty,
                words,
                length,
            }),
        );
    }
    pub(super) fn is_cooperative_root(&self) -> bool {
        if self.in_function {
            self.cooperative_root == Some(self.block_depth)
        } else {
            self.block_depth == 1
        }
    }
    pub(super) fn tile_barrier(&mut self, args: &[Expr], span: Span) -> Result<()> {
        if !args.is_empty() || !self.is_cooperative_root() || self.tiles.is_empty() {
            return self.fail(
                span,
                "B219",
                "tile_barrier() 需要既有 tile 范围，只能位于 parallel／cooperative 根块",
            );
        }
        self.code.push(Inst::TileBarrier);
        Ok(())
    }
    pub(super) fn lower_tiles(&mut self, span: Span) -> Result<Vec<Tile>> {
        std::mem::take(&mut self.tiles)
            .into_iter()
            .map(|t| match (t.source, t.width) {
                (TileOrigin::Bound(source), TileWidth::Fixed(width)) => Ok(Tile {
                    source,
                    ty: t.ty,
                    width,
                }),
                _ => self.fail(
                    span,
                    "B207",
                    "program 的 tile 必须具有实际来源与具体逻辑块宽度",
                ),
            })
            .collect()
    }
    pub(super) fn tile_length(&mut self, view: &TileView) -> Value {
        let r = self.reg(Scalar::U32);
        self.code.push(Inst::TileLength(r));
        Value {
            proof: view.length.clone(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![r],
            uniform: false,
            constant: None,
        }
    }
    pub(super) fn tile_lane(&mut self) -> Value {
        let r = self.reg(Scalar::U32);
        self.code.push(Inst::TileLane(r));
        Value {
            proof: self
                .tile_index_proof()
                .with_domains(Domains::from([IndexDomain::Tile])),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![r],
            uniform: false,
            constant: None,
        }
    }
    fn tile_width_bound(&self) -> u32 {
        match self.tiles.first().map(|t| t.width) {
            Some(TileWidth::Fixed(width)) => width,
            _ => u32::MAX,
        }
    }
    pub(super) fn tile_index_proof(&self) -> bounds::Proof {
        bounds::Proof::unsigned(0, self.tile_width_bound() - 1)
    }
    pub(super) fn tile_place(&mut self, view: TileView, index: &Expr) -> Result<Place> {
        let value = self.expr(index)?;
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, index.span)?;
        let minimum = match self.tiles[view.tile].width {
            TileWidth::Fixed(width) if self.prefix.is_none() => {
                self.program.buffers[self.domain].count.map_or(1, |n| {
                    let tail = n % width;
                    if tail == 0 { width } else { tail }
                })
            }
            _ => 1,
        };
        let bounded = self.index_in(&value, IndexDomain::Tile, minimum);
        if !bounded {
            return self.fail(
                index.span,
                "B201",
                "tile 索引需要本块有界游标、lane、有效长度条件或在所有尾块内有效的常量",
            );
        }
        Ok(Place::TileElement {
            ty: view.ty,
            tile: view.tile,
            index: value.regs[0],
            words: view.words,
        })
    }
}
