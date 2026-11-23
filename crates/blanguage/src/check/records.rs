//! 记录按声明字段构造；不对异构分量作统一数值强转，不产生运行时对象。
use super::*;
impl Builder<'_> {
    pub(super) fn dataset_layout<'a>(&'a self, view: &DatasetView) -> (u32, &'a Ty) {
        let mut ty = &self.program.buffers[view.binding].ty;
        let mut offset = 0;
        for index in &view.projection {
            let (_, at, field) = &ty.fields[*index];
            offset += at;
            ty = field;
        }
        (offset, ty)
    }
    pub(super) fn dataset_reference(&self, expr: &Expr) -> Result<Option<DatasetView>> {
        match expr.kind() {
            ExprKind::Name(name) => Ok(match self.env.get(name) {
                Some(Binding::Data(view)) => Some(view.clone()),
                _ => None,
            }),
            ExprKind::Member(base, field) => {
                let Some(mut view) = self.dataset_reference(&base)? else { return Ok(None); };
                let (_, ty) = self.dataset_layout(&view);
                let Some(index) = ty.fields.iter().position(|(name, _, _)| name == field) else {
                    return self.fail(expr.span(), "B141", "dataset 字段不存在");
                };
                view.projection.push(index);
                Ok(Some(view))
            }
            _ => Ok(None),
        }
    }
    pub(super) fn dataset_element(&self, view: &DatasetView, index: Reg, writable: bool) -> Place {
        let (offset, ty) = self.dataset_layout(view);
        Place::Element {
            ty: ty.clone(), binding: view.binding, index,
            words: ty.components.iter().map(|(_, word)| offset + word).collect(),
            writable: writable && view.access.writes(),
        }
    }
    pub(super) fn dataset_field(&self, expr: &Expr) -> Result<(usize, u32, Ty)> {
        match expr.kind() {
            ExprKind::Name(name) => {
                let Some(Binding::Data(view)) = self.env.get(name) else {
                    return self.fail(expr.span(), "B200", "需要命名 dataset 或其字段");
                };
                let (offset, ty) = self.dataset_layout(view);
                Ok((view.binding, offset, ty.clone()))
            }
            ExprKind::Member(base, name) => {
                let (binding, word, ty) = self.dataset_field(&base)?;
                let Some((offset, field)) = ty.field(name) else {
                    return self.fail(expr.span(), "B141", "dataset 字段不存在");
                };
                Ok((binding, word + offset, field.clone()))
            }
            _ => self.fail(expr.span(), "B200", "需要命名 dataset 或其字段"),
        }
    }
    #[inline(never)]
    pub(super) async fn record_expression(
        &mut self,
        name: &TypeName,
        args: &Expressions,
        span: Span,
    ) -> Result<Option<Value>> {
        let ty = unsafe { self.world.call(self.world.ty(
            name,
            self.scope.source(),
            span,
            self.scope.module(),
            self.scope.imports(),
        )).await }?;
        unsafe { self.world.call(self.record_values(ty,args.len(),span,async |builder,index,expected|
            unsafe { builder.world.call(builder.expr_expected(&args.at(index),expected)).await })).await }
    }
    pub(super) async fn record_values(&mut self,ty:Ty,count:usize,span:Span,
        mut argument:impl std::ops::AsyncFnMut(&mut Self,usize,Option<&Ty>)->Result<Option<Value>>)->Result<Option<Value>> {
        let mut cursor=LexicalCursor {phase:0,flags:0,index:0,part:0};
        let mut facts=RecordConstructFacts {arguments:count as u64,held:0,fields:ty.fields.len()as u64,elements:ty.array.as_ref().map_or(0,|_|u64::from(ty.values[0])),owns:u32::from(ty.owns_resources()),variants:u32::from(ty.has_variants()),enumeration:u32::from(ty.enumeration),matrix:u32::from(ty.matrix_shape().is_some()),c_function:u32::from(ty.is_c_function()),numeric:u32::from(ty.is_numeric()),array:u32::from(ty.array.is_some()),empty:u32::from(ty.components.is_empty()),present:0,borrows:0,same:0,uniform:0,constant:0};
        let mut values:Vec<Value>=Vec::new();let mut held=Vec::new();let mut released:Option<std::vec::IntoIter<String>>=None;let mut value:Option<Value>=None;
        let mut proof=bounds::Proof::default();let mut regs=Vec::new();let mut constant:Option<Vec<Bits>>=Some(Vec::new());
        loop {let step=unsafe {record_construct_step(cursor,RecordConstructFacts {..facts})};cursor=step.cursor;match step.need {
            RecordConstructNeed::Argument=>{
                let expected=match step.expected {1=>ty.array.as_deref(),2=>Some(ty.fields[cursor.index as usize].2.as_ref()),_=>None};
                value=argument(self,cursor.index as usize,expected).await?;facts.present=u32::from(value.is_some());facts.borrows=value.as_ref().map_or(0,|v|u32::from(v.ty.contains_borrows()));
            },
            RecordConstructNeed::Hold=>{held.push(self.hold_binding(Binding::Value(value.as_ref().unwrap().clone())));facts.held=held.len()as u64;},
            RecordConstructNeed::Push=>values.push(value.take().unwrap()),
            RecordConstructNeed::ReleaseBegin=>released=Some(std::mem::take(&mut held).into_iter()),
            RecordConstructNeed::Release=>{let name=released.as_mut().unwrap().next().unwrap();self.env.remove(&name);},
            RecordConstructNeed::Owned=>return self.fail(span,"B224","拥有资源的记录通过移动构造，不能复制为普通值"),
            RecordConstructNeed::Variant=>return self.fail(span,"B269","结果通过 Ok／Err 构造，不能直接构造原始存储"),
            RecordConstructNeed::BadEnum=>return self.fail(span,"B268","编号枚举构造需要一个同类型值或其声明的整数表示"),
            RecordConstructNeed::EnumValue=>{value=values.pop();facts.same=u32::from(value.as_ref().unwrap().ty==ty);},
            RecordConstructNeed::EnumType=>self.require_same(&Ty::scalar(ty.components[0].0),&value.as_ref().unwrap().ty,span)?,
            RecordConstructNeed::EnumReturn=>{let mut value=value.take().unwrap();value.ty=ty;value.literal=false;return Ok(Some(value));},
            RecordConstructNeed::Matrix=>return self.matrix_construct(ty,values,span).map(Some),
            RecordConstructNeed::Numeric=>return self.value_constructor(ty,values,span).map(Some),
            RecordConstructNeed::SameArray=>facts.same=u32::from(values[0].ty==ty),
            RecordConstructNeed::Single=>return Ok(values.pop()),
            RecordConstructNeed::ArrayType=>{let value=&values[cursor.index as usize];self.require_same(ty.array.as_deref().unwrap(),&value.ty,span)?;facts.uniform=u32::from(value.uniform);facts.constant=u32::from(value.constant.is_some());},
            RecordConstructNeed::BadArray=>return self.fail(span,"B157","array 构造需要全部元素，或一个填充值"),
            RecordConstructNeed::EmptyArray=>return Ok(Some(Value {proof,literal:false,ty,regs,uniform:cursor.flags&1!=0,constant:if cursor.flags&2!=0 {constant}else{None}})),
            RecordConstructNeed::ArrayBegin=>{if cursor.flags&2==0 {constant=None;}},
            RecordConstructNeed::ArrayElement=>{let value=&values[cursor.index as usize];proof.append(value);regs.extend(value.regs.iter().copied());if let Some(bits)=&mut constant {bits.extend(value.constant.as_ref().unwrap().iter().copied());}},
            RecordConstructNeed::ArrayValue=>return Ok(Some(Value {proof,literal:false,ty,regs,uniform:cursor.flags&1!=0,constant})),
            RecordConstructNeed::Record=>return self.pack_record(ty,values,span).map(Some),
            RecordConstructNeed::Empty=>return Ok(None),
        }}
    }
    pub(super) fn pack_record(&mut self,ty:Ty,values:Vec<Value>,span:Span)->Result<Value> {
        let mut cursor=LexicalCursor {phase:0,flags:0,index:0,part:0};
        let mut facts=RecordPackFacts {fields:ty.aggregate_len()as u64,values:values.len()as u64,uniform:0,constant:0};
        let mut items=values.into_iter();let mut pending:Option<Value>=None;let mut regs=Vec::new();let mut proof=bounds::Proof::default();let mut constant:Option<Vec<Bits>>=Some(Vec::new());
        loop {let step=unsafe {record_pack_step(cursor,RecordPackFacts {..facts})};cursor=step.cursor;match step.need {
            RecordPackNeed::BadCount=>return self.fail(span,"B157","记录构造需要按声明顺序提供每个字段"),
            RecordPackNeed::Field=>{let value=items.next().unwrap();let (_,field)=ty.aggregate_part(cursor.index as usize);self.require_same(field,&value.ty,span)?;facts.uniform=u32::from(value.uniform);facts.constant=u32::from(value.constant.is_some());pending=Some(value);},
            RecordPackNeed::Append=>{let value=pending.take().unwrap();proof.append(&value);regs.extend(value.regs);if cursor.flags&2!=0 {constant.as_mut().unwrap().extend(value.constant.unwrap());}else{constant=None;}},
            RecordPackNeed::Value=>return Ok(Value {proof,literal:false,ty,regs,uniform:cursor.flags&1!=0,constant}),
        }}
    }
}

#[repr(C)]struct RecordConstructFacts {arguments:u64,held:u64,fields:u64,elements:u64,owns:u32,variants:u32,enumeration:u32,matrix:u32,c_function:u32,numeric:u32,array:u32,empty:u32,present:u32,borrows:u32,same:u32,uniform:u32,constant:u32}
#[repr(u32)]enum RecordConstructNeed {Argument,Hold,Push,ReleaseBegin,Release,Owned,Variant,BadEnum,EnumValue,EnumType,EnumReturn,Matrix,Numeric,SameArray,Single,ArrayType,BadArray,EmptyArray,ArrayBegin,ArrayElement,ArrayValue,Record,Empty}
#[repr(C)]struct RecordConstructStep {cursor:LexicalCursor,need:RecordConstructNeed,expected:u32}
#[repr(C)]struct RecordPackFacts {fields:u64,values:u64,uniform:u32,constant:u32}
#[repr(u32)]enum RecordPackNeed {BadCount,Field,Append,Value}
#[repr(C)]struct RecordPackStep {cursor:LexicalCursor,need:RecordPackNeed}
unsafe extern "C" {
    #[link_name="bootstrap.function_body::record_construct_step"]fn record_construct_step(cursor:LexicalCursor,facts:RecordConstructFacts)->RecordConstructStep;
    #[link_name="bootstrap.function_body::record_pack_step"]fn record_pack_step(cursor:LexicalCursor,facts:RecordPackFacts)->RecordPackStep;
}
