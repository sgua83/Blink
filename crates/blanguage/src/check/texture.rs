//! 纹理按只读资源身份传递；采样没有普通值地址或可写别名。
use super::*;
use crate::ast::TextureType;
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
                parameter: None,
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
pub(super) async fn shape(
    world: &World,
    texture: TextureType<'_>,
    context: (&Source,&str,&[String]),
    span: Span,
) -> Result<Option<u32>> {
    let (source, module, imports) = context;
    let Some(length) = texture.length else {
        return Ok(None);
    };
    let count = unsafe {
        world.call(constant::borrowed_shape(world, Some(length), source, span, module, imports)).await
    }?.unwrap();
    if count == 0 {
        return source.fail(span, "B239", "纹理资源数组长度必须为正");
    }
    Ok(Some(count))
}
pub(super) async fn binding(
    world: &World,
    param: &Param,
    texture: TextureType<'_>,
    context: (&Source, &str, &[String]),
) -> Result<TextureBinding> {
    Ok(TextureBinding {
        name: param.name.clone(),
        count: unsafe { world.call(shape(world, texture, context, param.span)).await }?,
        dimension: texture.dimension,
        format: texture.format,
    })
}

pub(super) async fn compatible(
    world: &World,
    param: &Param,
    binding: &TextureBinding,
    count: Option<u32>,
    context: (&Source, &str, &[String]),
) -> Result<bool> {
    let Some(texture) = param.ty.texture_type() else {
        return Ok(false);
    };
    if texture.dimension != binding.dimension || texture.format.is_some_and(|format| Some(format) != binding.format) {
        return Ok(false);
    }
    Ok(unsafe { world.call(shape(world, texture, context, param.span)).await }? == count)
}

impl Builder<'_> {
    pub(super) async fn texture_parameter(&self, param: &Param, resource: &TextureReference, context:(&Source,&str,&[String]), span: Span) -> Result<()> {
        if param.access != Access::Read {
            return self.fail(span, "B239", "纹理只能只读借用");
        }
        let binding = &self.program.textures[resource.source.binding];
        if !unsafe {
            self.world.call(compatible(self.world, param, binding, resource.shape(self.program), context)).await
        }? {
            return self.fail(span, "B239", "纹理维度、格式或资源数组形状与借用参数不一致");
        }
        Ok(())
    }
    pub(super) async fn texture_argument(&mut self, expr: &Expr) -> Result<Option<TextureReference>> {
        let source=expr.source_view();let signature=expr.signature_node();
        let mut cursor=LexicalCursor{phase:0,flags:0,index:0,part:0};let mut facts=TextureArgumentFacts{present:0,array:0,bounded:0};
        let mut resource=None;let mut count=None;let mut value=None;
        loop{let step=unsafe{texture_argument_step(cursor,signature,TextureArgumentFacts{..facts})};cursor=step.cursor;match step.need{
            TextureArgumentNeed::Binding=>{resource=match self.env.get(source.text(step.name)){Some(Binding::Texture(resource))=>Some(resource.clone()),_=>None};facts.present=u32::from(resource.is_some());},
            TextureArgumentNeed::Resource=>return Ok(resource.take()),
            TextureArgumentNeed::Base=>{let base=expr.child(cursor.part);resource=unsafe{self.world.call(self.texture_argument(&base)).await}?;facts.present=u32::from(resource.is_some());},
            TextureArgumentNeed::Shape=>{count=resource.as_ref().unwrap().shape(self.program);facts.array=u32::from(count.is_some());},
            TextureArgumentNeed::BadShape=>return self.fail(expr.child::<crate::syntax::wire::ExprNode>(cursor.part).span(),"B239","纹理下标需要资源数组"),
            TextureArgumentNeed::Index=>{let index=expr.child(cursor.index);value=unsafe{self.world.call(self.expr(&index)).await}?;facts.present=u32::from(value.is_some());},
            TextureArgumentNeed::IndexType=>self.require_same(&Ty::scalar(Scalar::U32),&value.as_ref().unwrap().ty,expr.child::<crate::syntax::wire::ExprNode>(cursor.index).span())?,
            TextureArgumentNeed::Bounds=>facts.bounded=u32::from(self.index_in(value.as_ref().unwrap(),IndexDomain::Fixed(count.unwrap()),count.unwrap(),None)),
            TextureArgumentNeed::BadBounds=>return self.fail(expr.child::<crate::syntax::wire::ExprNode>(cursor.index).span(),"B137","纹理下标有效性无法静态证明；使用数组 length 或有界条件"),
            TextureArgumentNeed::Select=>{let mut resource=resource.take().unwrap();let value=value.take().unwrap();resource.source.element=Some(value.regs[0]);resource.uniform=value.uniform;return Ok(Some(resource));},
            TextureArgumentNeed::Empty=>return Ok(None),
            TextureArgumentNeed::BadArguments=>return self.fail(expr.span(),"B239","unsafe_read(纹理数组, u32 下标) 需要两个参数"),
            TextureArgumentNeed::BadSource=>return self.fail(expr.span(),"B239","需要已有的 read texture2d／texture3d 资源名称"),
        }}
    }
    pub(super) async fn texture_sample(&mut self, args: &Expressions, span: Span) -> Result<Option<Value>> {
        let mut cursor=SourceValueCursor{phase:0};let mut facts=TextureSampleFacts{count:args.len()as u64,present:0,array:0,texture_uniform:0,coordinate_uniform:0,lod_uniform:0};
        let mut texture=None;let mut coordinate=None;let mut lod=None;let mut binding=None;let mut proof=None;let mut result:Option<[Reg;4]>=None;
        loop{let step=unsafe{texture_sample_step(cursor,TextureSampleFacts{..facts})};cursor=step.cursor;match step.need{
            TextureSampleNeed::BadArguments=>return self.fail(span,"B239","texture_sample(texture, float2／float3 坐标, f32 lod) 需要三个参数"),
            TextureSampleNeed::Texture=>{texture=unsafe{self.world.call(self.texture_argument(&args.at(0))).await}?;facts.present=u32::from(texture.is_some());},
            TextureSampleNeed::Shape=>{let texture=texture.as_ref().unwrap();facts.array=u32::from(texture.shape(self.program).is_some());facts.texture_uniform=u32::from(texture.uniform);},
            TextureSampleNeed::BadArray=>return self.fail(args.at(0).span(),"B239","采样需要单张纹理；用下标选择资源数组元素"),
            TextureSampleNeed::Coordinate=>{coordinate=unsafe{self.world.call(self.expr(&args.at(1))).await}?;facts.present=u32::from(coordinate.is_some());facts.coordinate_uniform=coordinate.as_ref().map_or(0,|value|u32::from(value.uniform));},
            TextureSampleNeed::Lod=>{lod=unsafe{self.world.call(self.expr(&args.at(2))).await}?;facts.present=u32::from(lod.is_some());facts.lod_uniform=lod.as_ref().map_or(0,|value|u32::from(value.uniform));},
            TextureSampleNeed::Binding=>{let actual=&self.program.textures[texture.as_ref().unwrap().source.binding];binding=Some((actual.dimension,actual.format));},
            TextureSampleNeed::CoordinateType=>self.require_same(&Ty::vector(Scalar::F32,binding.unwrap().0.components()),&coordinate.as_ref().unwrap().ty,args.at(1).span())?,
            TextureSampleNeed::LodType=>self.require_same(&Ty::scalar(Scalar::F32),&lod.as_ref().unwrap().ty,args.at(2).span())?,
            TextureSampleNeed::Proof=>proof=Some(bounds::Proof::texture_sample(binding.unwrap().1,coordinate.as_ref().unwrap(),lod.as_ref().unwrap())),
            TextureSampleNeed::Registers=>result=Some(std::array::from_fn(|_|self.reg(Scalar::F32))),
            TextureSampleNeed::Emit=>self.emit(InstKind::TextureSample{result:result.unwrap(),texture:texture.as_ref().unwrap().source,coordinate:coordinate.take().unwrap().regs,lod:lod.as_ref().unwrap().regs[0]}),
            TextureSampleNeed::Value=>return Ok(Some(Value{proof:proof.take().unwrap(),literal:false,ty:Ty::vector(Scalar::F32,4),regs:result.unwrap().to_vec(),uniform:step.uniform!=0,constant:None})),
            TextureSampleNeed::Empty=>return Ok(None),
        }}
    }
}

#[repr(C)]struct TextureArgumentFacts{present:u32,array:u32,bounded:u32}
#[repr(u32)]enum TextureArgumentNeed{Binding,Resource,Base,Shape,BadShape,Index,IndexType,Bounds,BadBounds,Select,Empty,BadArguments,BadSource}
#[repr(C)]struct TextureArgumentStep{cursor:LexicalCursor,need:TextureArgumentNeed,name:crate::syntax::wire::Text}
#[repr(C)]struct TextureSampleFacts{count:u64,present:u32,array:u32,texture_uniform:u32,coordinate_uniform:u32,lod_uniform:u32}
#[repr(u32)]enum TextureSampleNeed{BadArguments,Texture,Shape,BadArray,Coordinate,Lod,Binding,CoordinateType,LodType,Proof,Registers,Emit,Value,Empty}
#[repr(C)]struct TextureSampleStep{cursor:SourceValueCursor,need:TextureSampleNeed,uniform:u32}
unsafe extern "C"{
 #[link_name="bootstrap.function_body::texture_argument_step"]fn texture_argument_step(cursor:LexicalCursor,source:crate::syntax::wire::SignatureNode,facts:TextureArgumentFacts)->TextureArgumentStep;
 #[link_name="bootstrap.function_body::texture_sample_step"]fn texture_sample_step(cursor:SourceValueCursor,facts:TextureSampleFacts)->TextureSampleStep;
}
