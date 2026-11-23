//! 记录按声明字段构造；不对异构分量作统一数值强转，不产生运行时对象。
use super::*;
impl Builder<'_> {
    pub(super) fn dataset_layout<'a>(&'a self,view:&DatasetView)->(u32,&'a Ty) {
        let mut context=DatasetLayoutContext {ty:&self.program.buffers[view.binding].ty,view};
        let offset=unsafe {dataset_layout(view.projection.len()as u64,std::ptr::from_mut(&mut context).cast(),dataset_projection_part)};
        (offset,context.ty)
    }
    fn dataset_projection<'a>(&'a self,expr:&Expr,usage:DatasetProjectionUse)->Result<(DatasetProjection,Option<DatasetView>,Option<&'a Ty>)> {
        let mut context=DatasetProjectionContext {builder:self,source:expr.source_view(),view:None,ty:None};
        let result=unsafe {dataset_projection(expr.signature_node(),usage,DatasetProjectionHost {context:std::ptr::from_mut(&mut context).cast(),root:dataset_projection_root,declared:dataset_declared_field,field:dataset_selected_field,select:dataset_projection_append})};
        let span=Span {start:result.span.start as usize,end:result.span.end as usize};
        match result.kind {DatasetProjectionKind::BadField=>return self.fail(span,"B141","dataset 字段不存在"),DatasetProjectionKind::BadRoot=>return self.fail(span,"B200","需要命名 dataset 或其字段"),_=>{}}
        Ok((result,context.view,context.ty))
    }
    pub(super) fn dataset_reference(&self,expr:&Expr)->Result<Option<DatasetView>> {
        Ok(self.dataset_projection(expr,DatasetProjectionUse::Reference)?.1)
    }
    pub(super) fn is_dataset_reference(&self,expr:&Expr)->Result<bool> {
        Ok(matches!(self.dataset_projection(expr,DatasetProjectionUse::Presence)?.0.kind,DatasetProjectionKind::Found))
    }
    pub(super) fn dataset_element(&self,view:&DatasetView,index:Reg,writable:bool)->Place {
        let (offset,ty)=self.dataset_layout(view);
        element_at(ty,view.binding,index,offset,view.access,writable)
    }
    pub(super) fn dataset_field(&self,expr:&Expr)->Result<(usize,u32,Ty)> {
        let (result,_,ty)=self.dataset_projection(expr,DatasetProjectionUse::Field)?;
        Ok((result.binding as usize,result.offset,ty.unwrap().clone()))
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


#[repr(u32)]enum DatasetProjectionUse {Reference,Field,Presence}
#[derive(Clone,Copy)]#[repr(u32)]enum DatasetProjectionKind {Missing,Found,BadField,BadRoot}
#[repr(C)]struct DatasetProjection {kind:DatasetProjectionKind,offset:u32,binding:u64,fields:u64,span:crate::syntax::wire::Span}
#[repr(C)]struct DatasetField {present:u32,offset:u32,fields:u64,name:SourceOperation}
#[repr(C)]struct DatasetProjectionHost {context:*mut std::ffi::c_void,root:unsafe extern "C" fn(*mut std::ffi::c_void,crate::syntax::wire::Text,u32)->DatasetProjection,declared:unsafe extern "C" fn(*mut std::ffi::c_void,u64)->DatasetField,field:unsafe extern "C" fn(*mut std::ffi::c_void,crate::syntax::wire::Text)->DatasetField,select:unsafe extern "C" fn(*mut std::ffi::c_void,u64,u32)}
struct DatasetProjectionContext<'a,'b,'c> {builder:&'a Builder<'b>,source:crate::syntax::SourceView<'c>,view:Option<DatasetView>,ty:Option<&'a Ty>}
struct DatasetLayoutContext<'a,'b> {ty:&'a Ty,view:&'b DatasetView}
pub(super) fn element_at(ty:&Ty,binding:usize,index:Reg,offset:u32,access:Access,writable:bool)->Place {
    let mut output=DatasetWords {ty,words:Vec::with_capacity(ty.components.len())};
    let writable=unsafe {dataset_element_words(offset,ty.components.len()as u64,access as u32,u32::from(writable),std::ptr::from_mut(&mut output).cast(),dataset_component,append_dataset_word)}!=0;
    Place::Element {ty:ty.clone(),binding,index,words:output.words,writable}
}
struct DatasetWords<'a> {ty:&'a Ty,words:Vec<u32>}
unsafe extern "C" fn dataset_projection_part(context:*mut std::ffi::c_void,index:u64)->u32 {
    let context=unsafe {&mut *context.cast::<DatasetLayoutContext<'_,'_>>()};let ty=context.ty;let (_,offset,field)=&ty.fields[context.view.projection[index as usize]];
    context.ty=field;*offset
}
unsafe extern "C" fn dataset_projection_root(context:*mut std::ffi::c_void,name:crate::syntax::wire::Text,collect:u32)->DatasetProjection {
    let context=unsafe {&mut *context.cast::<DatasetProjectionContext<'_,'_,'_>>()};
    match context.builder.env.get(context.source.text(name)) {
        Some(Binding::Data(view))=>{let (offset,ty)=context.builder.dataset_layout(view);context.ty=Some(ty);if collect!=0 {context.view=Some(view.clone());}DatasetProjection {kind:DatasetProjectionKind::Found,offset,binding:view.binding as u64,fields:ty.fields.len()as u64,span:Default::default()}},
        _=>DatasetProjection {kind:DatasetProjectionKind::Missing,offset:0,binding:0,fields:0,span:Default::default()},
    }
}
unsafe extern "C" fn dataset_declared_field(context:*mut std::ffi::c_void,index:u64)->DatasetField {
    let context=unsafe {&*context.cast::<DatasetProjectionContext<'_,'_,'_>>()};let (name,offset,field)=&context.ty.unwrap().fields[index as usize];
    DatasetField {present:1,offset:*offset,fields:field.fields.len()as u64,name:SourceOperation {data:name.as_ptr(),length:name.len()as u64}}
}
unsafe extern "C" fn dataset_selected_field(context:*mut std::ffi::c_void,name:crate::syntax::wire::Text)->DatasetField {
    let context=unsafe {&mut *context.cast::<DatasetProjectionContext<'_,'_,'_>>()};let ty=context.ty.unwrap();
    match ty.field(context.source.text(name)) {Some((offset,field))=>{context.ty=Some(field);DatasetField {present:1,offset,fields:field.fields.len()as u64,name:SourceOperation {data:std::ptr::null(),length:0}}},None=>DatasetField {present:0,offset:0,fields:0,name:SourceOperation {data:std::ptr::null(),length:0}}}
}
unsafe extern "C" fn dataset_projection_append(context:*mut std::ffi::c_void,index:u64,collect:u32) {
    let context=unsafe {&mut *context.cast::<DatasetProjectionContext<'_,'_,'_>>()};let ty=context.ty.unwrap();context.ty=Some(&ty.fields[index as usize].2);
    if collect!=0 {context.view.as_mut().unwrap().projection.push(index as usize);}
}
unsafe extern "C" fn dataset_component(context:*mut std::ffi::c_void,index:u64)->u32 {unsafe {&*context.cast::<DatasetWords<'_>>()}.ty.components[index as usize].1}
unsafe extern "C" fn append_dataset_word(context:*mut std::ffi::c_void,word:u32) {unsafe {&mut *context.cast::<DatasetWords<'_>>()}.words.push(word);}
unsafe extern "C" {
    #[link_name="bootstrap.function_body::dataset_layout"]fn dataset_layout(count:u64,context:*mut std::ffi::c_void,part:unsafe extern "C" fn(*mut std::ffi::c_void,u64)->u32)->u32;
    #[link_name="bootstrap.function_body::dataset_projection"]fn dataset_projection(source:crate::syntax::wire::SignatureNode,usage:DatasetProjectionUse,host:DatasetProjectionHost)->DatasetProjection;
    #[link_name="bootstrap.function_body::dataset_element_words"]fn dataset_element_words(base:u32,count:u64,access:u32,requested:u32,context:*mut std::ffi::c_void,component:unsafe extern "C" fn(*mut std::ffi::c_void,u64)->u32,append:unsafe extern "C" fn(*mut std::ffi::c_void,u32))->u32;
}
