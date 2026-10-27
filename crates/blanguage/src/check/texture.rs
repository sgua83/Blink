//! 纹理按只读资源身份传递；采样没有普通值地址或可写别名。
use super::*;
#[derive(Clone)]
pub(super) struct TextureReference {
    pub source: TextureSource,
    pub uniform: bool,
}
impl TextureReference {
    pub fn binding(binding: usize) -> Self {
        Self {
            source: TextureSource {
                binding,
                element: None,
            },
            uniform: true,
        }
    }
    pub fn shape(&self, program: &Program) -> Option<u32> {
        if self.source.element.is_some() {
            None
        } else {
            program.textures[self.source.binding].count
        }
    }
}
pub(super) fn shape(
    world: &World,
    ty: &TypeName,
    d: &Definition,
    span: Span,
) -> Result<Option<u32>> {
    let Some(length) = ty.texture_array_shape() else {
        return Ok(None);
    };
    let count = world
        .shape(&Some(length), &d.source, span, &d.module, &d.imports)?
        .unwrap();
    if count == 0 {
        return d.source.fail(span, "B239", "纹理资源数组长度必须为正");
    }
    Ok(Some(count))
}
impl Builder<'_> {
    pub(super) fn texture_argument(&mut self, expr: &Expr) -> Result<Option<TextureReference>> {
        if let ExprKind::Name(name) = &expr.kind
            && let Some(Binding::Texture(resource)) = self.env.get(name)
        {
            return Ok(Some(resource.clone()));
        }
        let selection = match &expr.kind {
            ExprKind::Index(base, index) => Some((&**base, &**index, true)),
            ExprKind::Call(name, args) if name == "unsafe_read" => {
                if args.len() != 2 {
                    return self.fail(
                        expr.span,
                        "B239",
                        "unsafe_read(纹理数组, u32 下标) 需要两个参数",
                    );
                }
                Some((&args[0], &args[1], false))
            }
            _ => None,
        };
        if let Some((base, index, checked)) = selection {
            let Some(mut resource) = self.texture_argument(base)? else {
                return Ok(None);
            };
            let Some(count) = resource.shape(self.program) else {
                return self.fail(base.span, "B239", "纹理下标需要资源数组");
            };
            let Some(value) = self.expr(index)? else {
                return Ok(None);
            };
            self.require_same(&Ty::scalar(Scalar::U32), &value.ty, index.span)?;
            if checked && !self.index_in(&value, IndexDomain::Fixed(count), count) {
                return self.fail(
                    index.span,
                    "B137",
                    "纹理下标有效性无法静态证明；使用数组 length 或有界条件",
                );
            }
            resource.source.element = Some(value.regs[0]);
            resource.uniform = value.uniform;
            return Ok(Some(resource));
        }
        self.fail(expr.span, "B239", "需要已有的 read texture2d 资源名称")
    }
    pub(super) fn texture_sample(&mut self, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if args.len() != 3 {
            return self.fail(
                span,
                "B239",
                "texture_sample(texture, float2 坐标, f32 lod) 需要三个参数",
            );
        }
        let Some(texture) = self.texture_argument(&args[0])? else {
            return Ok(None);
        };
        if texture.shape(self.program).is_some() {
            return self.fail(
                args[0].span,
                "B239",
                "采样需要单张纹理；用下标选择资源数组元素",
            );
        }
        let uniform = texture.uniform;
        let Some(coordinate) = self.expr(&args[1])? else {
            return Ok(None);
        };
        let Some(lod) = self.expr(&args[2])? else {
            return Ok(None);
        };
        self.require_same(&Ty::vector(Scalar::F32, 2), &coordinate.ty, args[1].span)?;
        self.require_same(&Ty::scalar(Scalar::F32), &lod.ty, args[2].span)?;
        let result = std::array::from_fn(|_| self.reg(Scalar::F32));
        self.code.push(Inst::TextureSample {
            result,
            texture: texture.source,
            coordinate: [coordinate.regs[0], coordinate.regs[1]],
            lod: lod.regs[0],
        });
        Ok(Some(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::vector(Scalar::F32, 4),
            regs: result.to_vec(),
            uniform: uniform && coordinate.uniform && lod.uniform,
            constant: None,
        }))
    }
}
