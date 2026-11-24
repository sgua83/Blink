//! B controls tile syntax and progression; the original owners retain types, proofs and IR.
use super::*;
use std::ffi::c_void;
use crate::syntax::wire::{SignatureNode,StorageView,Slice,Text,ExprNode};
type Address = *mut c_void;
#[derive(Clone,Copy,Default)]#[repr(C)]struct TileCursor {index:u64,phase:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct TileInputFacts {name_exists:u32,access:u32,views:u32,nonempty:u32,source_known:u32,source_size:u32,domain_known:u32,domain_size:u32,found:u32}
#[allow(dead_code)]#[repr(u32)]enum TileInputNeed {Name,Width,Field,Capacity,ElementDomain,Find,Push,Bind,Complete,BadSyntax,Duplicate,BadType,BadCapacity}
#[repr(C)]struct TileInputStep {cursor:TileCursor,need:TileInputNeed,name:Text,field:u64,width:u64}
#[repr(C)]struct TileCandidate {binding:u64,word:u32,input:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct TileRange {present:u32,fixed:u32,width:u32}
#[derive(Clone,Copy)]#[repr(C)]struct TileWidthFacts {first:TileRange,constant:u32,scalar:u32,width:u32}
#[allow(dead_code)]#[repr(u32)]enum TileWidthNeed {Read,Complete,ConstantError,WidthError}
#[repr(C)]struct TileWidthStep {cursor:SourceValueCursor,need:TileWidthNeed}
#[derive(Clone,Copy,Default)]#[repr(C)]struct TilePublishFacts {arguments:u64,annotated:u32,root:u32,cooperative:u32,has_tile:u32,present:u32,views:u32,nonempty:u32,atomic:u32}
#[allow(dead_code)]#[repr(u32)]enum TilePublishNeed {FixedWidth,InheritedWidth,Value,Publish,Bind,Complete,BadRoot,BadFixed,MissingWidth,BadValue}
#[repr(C)]struct TilePublishStep {cursor:SourceValueCursor,need:TilePublishNeed,access:Access}
#[derive(Clone,Copy,Default)]#[repr(C)]struct TileCollectiveFacts {arguments:u64,root:u32,atomic:u32,input:u32,numeric:u32,integer:u32,sum:u32,proof:u32}
#[allow(dead_code)]#[repr(u32)]enum TileCollectiveNeed {View,Read,Type,CommonProof,SumProof,Publish,BadRoot,BadAtomic,BadType}
#[repr(C)]struct TileCollectiveStep {cursor:SourceValueCursor,need:TileCollectiveNeed}
#[repr(C)]struct TileProjectionHost {root:unsafe extern "C" fn(Address,u64,Text)->u32,member:unsafe extern "C" fn(Address,u64,Text)->u32,invalid:unsafe extern "C" fn(Address,u64)->u32}
#[repr(C)]struct TilePair {word:u32,reg:u64}
#[repr(C)]struct TileComponent {scalar:u32,word:u32}
#[repr(C)]struct TileResultHost {get:unsafe extern "C" fn(Address,u64)->TileComponent,allocate:unsafe extern "C" fn(Address,u32)->u64,push:unsafe extern "C" fn(Address,TilePair)}
#[derive(Clone,Copy,Default)]#[repr(C)]struct TileLowerFacts {present:u32,actual:u32,fixed:u32}
#[allow(dead_code)]#[repr(u32)]enum TileLowerNeed {Next,Publish,Complete,Invalid}
#[repr(C)]struct TileLowerStep {cursor:SourceValueCursor,need:TileLowerNeed}
#[repr(C)]struct TileLanePlan {maximum:u32,mark:u32,width:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct TilePlaceFacts {present:u32,bounded:u32,fixed:u32,prefix:u32,known:u32}
#[allow(dead_code)]#[repr(u32)]enum TilePlaceNeed {Type,Range,Count,Minimum,Bound,Publish,Complete,Invalid}
#[repr(C)]struct TilePlaceStep {cursor:SourceValueCursor,need:TilePlaceNeed}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::tile_input_step"]fn tile_input_step(cursor:TileCursor,source:StorageView,body:Slice,facts:TileInputFacts)->TileInputStep;
 #[link_name="bootstrap.function_body::tile_find_input"]fn tile_find_input(count:u64,binding:u64,word:u32,context:Address,candidate:unsafe extern "C" fn(Address,u64)->TileCandidate,same:unsafe extern "C" fn(Address,u64)->u32)->u64;
 #[link_name="bootstrap.function_body::tile_width_step"]fn tile_width_step(cursor:SourceValueCursor,facts:TileWidthFacts)->TileWidthStep;
 #[link_name="bootstrap.function_body::tile_publish_step"]fn tile_publish_step(cursor:SourceValueCursor,facts:TilePublishFacts)->TilePublishStep;
 #[link_name="bootstrap.function_body::tile_collective_step"]fn tile_collective_step(operation:CollectiveOperator,cursor:SourceValueCursor,facts:TileCollectiveFacts)->TileCollectiveStep;
 #[link_name="bootstrap.function_body::tile_project"]fn tile_project(source:SignatureNode,context:Address,host:TileProjectionHost)->u32;
 #[link_name="bootstrap.function_body::tile_words"]fn tile_words(count:u64,context:Address,get:unsafe extern "C" fn(Address,u64)->u32,push:unsafe extern "C" fn(Address,u32));
 #[link_name="bootstrap.function_body::tile_publication"]fn tile_publication(count:u64,context:Address,get:unsafe extern "C" fn(Address,u64)->TilePair,push:unsafe extern "C" fn(Address,TilePair));
 #[link_name="bootstrap.function_body::tile_results"]fn tile_results(count:u64,context:Address,host:TileResultHost);
 #[link_name="bootstrap.function_body::tile_root"]fn tile_root(function:u32,cooperative:u32,root:u64,depth:u64)->u32;
 #[link_name="bootstrap.function_body::tile_barrier_allowed"]fn tile_barrier_allowed(arguments:u64,root:u32,tiles:u64)->u32;
 #[link_name="bootstrap.function_body::tile_lower_step"]fn tile_lower_step(cursor:SourceValueCursor,facts:TileLowerFacts)->TileLowerStep;
 #[link_name="bootstrap.function_body::tile_width_bound"]fn tile_width_bound(range:TileRange)->u32;
 #[link_name="bootstrap.function_body::tile_lane_plan"]fn tile_lane_plan(range:TileRange)->TileLanePlan;
 #[link_name="bootstrap.function_body::tile_index_minimum"]fn tile_index_minimum(width:u32,count:u32)->u32;
 #[link_name="bootstrap.function_body::tile_place_step"]fn tile_place_step(cursor:SourceValueCursor,facts:TilePlaceFacts)->TilePlaceStep;
}
fn width_range(width:Option<TileWidth>)->TileRange {
 match width {Some(TileWidth::Fixed(width))=>TileRange{present:1,fixed:1,width},Some(TileWidth::Parameter)=>TileRange{present:1,..Default::default()},None=>TileRange::default()}
}
struct TileSearch<'a> {tiles:&'a [CheckedTile],ty:&'a Ty}
unsafe extern "C" fn tile_candidate(context:Address,index:u64)->TileCandidate {
 let context=unsafe{&*context.cast::<TileSearch<'_>>()};
 match &context.tiles[index as usize].source {TileOrigin::Input{binding,word,..}=>TileCandidate{binding:*binding as u64,word:*word,input:1},_=>TileCandidate{binding:0,word:0,input:0}}
}
unsafe extern "C" fn tile_same_type(context:Address,index:u64)->u32 {let context=unsafe{&*context.cast::<TileSearch<'_>>()};u32::from(context.tiles[index as usize].ty()==context.ty)}
struct TypeWords<'a> {ty:&'a Ty,words:Vec<u32>}
unsafe extern "C" fn type_word(context:Address,index:u64)->u32 {unsafe{&*context.cast::<TypeWords<'_>>()}.ty.components[index as usize].1}
unsafe extern "C" fn push_type_word(context:Address,word:u32) {unsafe{&mut *context.cast::<TypeWords<'_>>()}.words.push(word);}
struct ProjectWords<'a> {words:&'a [u32],indices:&'a [usize],output:Vec<u32>}
unsafe extern "C" fn project_word(context:Address,index:u64)->u32 {let context=unsafe{&*context.cast::<ProjectWords<'_>>()};context.words[context.indices[index as usize]]}
unsafe extern "C" fn push_project_word(context:Address,word:u32) {unsafe{&mut *context.cast::<ProjectWords<'_>>()}.output.push(word);}
struct PublishWords<'a> {value:&'a Value,values:Vec<(u32,Reg)>}
unsafe extern "C" fn publish_word(context:Address,index:u64)->TilePair {let context=unsafe{&*context.cast::<PublishWords<'_>>()};TilePair{word:context.value.ty.components[index as usize].1,reg:context.value.regs[index as usize] as u64}}
unsafe extern "C" fn push_publish_word(context:Address,pair:TilePair) {unsafe{&mut *context.cast::<PublishWords<'_>>()}.values.push((pair.word,pair.reg as Reg));}
struct TileResults<'a,'w> {builder:&'a mut Builder<'w>,ty:&'a Ty,words:&'a [u32],regs:Vec<Reg>,results:Vec<(u32,Reg)>}
unsafe extern "C" fn tile_component(context:Address,index:u64)->TileComponent {let context=unsafe{&*context.cast::<TileResults<'_, '_>>()};TileComponent{scalar:context.ty.components[index as usize].0 as u32,word:context.words[index as usize]}}
unsafe extern "C" fn tile_register(context:Address,scalar:u32)->u64 {unsafe{&mut *context.cast::<TileResults<'_, '_>>()}.builder.reg(operators::SCALARS[scalar as usize]) as u64}
unsafe extern "C" fn push_tile_result(context:Address,pair:TilePair) {let context=unsafe{&mut *context.cast::<TileResults<'_, '_>>()};context.regs.push(pair.reg as Reg);context.results.push((pair.word,pair.reg as Reg));}
struct TileProjection<'a,'w> {builder:&'a Builder<'w>,expression:&'a Expr,value:Option<Result<TileView>>}
unsafe extern "C" fn tile_projection_invalid(context:Address,offset:u64)->u32 {
 let context=unsafe{&mut *context.cast::<TileProjection<'_, '_>>()};let expression=context.expression.child::<ExprNode>(offset);
 context.value=Some(context.builder.fail(expression.span(),"B206","需要输入／发布的 tile 或其字段借用"));0
}
unsafe extern "C" fn tile_projection_root(context:Address,offset:u64,name:Text)->u32 {
 let projection=unsafe{&mut *context.cast::<TileProjection<'_, '_>>()};
 match projection.builder.env.get(projection.expression.source_view().text(name)) {
  Some(Binding::Tile(view))=>{projection.value=Some(Ok(view.clone()));1},
  _=>unsafe{tile_projection_invalid(context,offset)},
 }
}
unsafe extern "C" fn tile_projection_member(context:Address,offset:u64,field:Text)->u32 {
 let context=unsafe{&mut *context.cast::<TileProjection<'_, '_>>()};
 let view=context.value.take().unwrap().unwrap();let expression=context.expression.child::<ExprNode>(offset);
 let result=member_type(&view.ty,context.expression.source_view().text(field),context.builder.scope.source(),expression.span()).map(|(ty,indices)| {
  let mut words=ProjectWords{words:&view.words,indices:&indices,output:Vec::new()};
  unsafe{tile_words(indices.len() as u64,(&mut words as *mut ProjectWords<'_>).cast(),project_word,push_project_word)};
  TileView{tile:view.tile,ty,words:words.output,length:view.length}
 });
 let present=u32::from(result.is_ok());context.value=Some(result);present
}
impl Builder<'_> {
 pub(super) async fn tile_inputs(&mut self,body:&Block)->Result<usize> {
  let source=body.source_view();let mut cursor=TileCursor::default();let mut facts=TileInputFacts::default();
  let mut syntax:Option<TileInputStep>=None;let mut field=None;let mut width=0;let mut tile=0;
  loop {
   let step=unsafe{tile_input_step(cursor,source.storage(),body.source_slice(),facts)};cursor=step.cursor;
   match step.need {
    TileInputNeed::Name=>{facts.name_exists=u32::from(self.env.contains_key(source.text(step.name)));syntax=Some(step);},
    TileInputNeed::Width=>{let expr=source.node::<ExprNode>(syntax.as_ref().unwrap().width);width=unsafe{self.world.call(self.tile_width(&expr)).await}?;},
    TileInputNeed::Field=>{let expr=source.node::<ExprNode>(syntax.as_ref().unwrap().field);let value=self.dataset_field(&expr)?;facts.access=self.view_access[&value.0] as u32;facts.views=u32::from(value.2.contains_views());facts.nonempty=u32::from(!value.2.components.is_empty());field=Some(value);},
    TileInputNeed::Capacity=>{let size=self.program.buffers[field.as_ref().unwrap().0].count;let domain=self.program.buffers[self.domain].count;facts.source_known=u32::from(size.is_some());facts.source_size=size.unwrap_or(0);facts.domain_known=u32::from(domain.is_some());facts.domain_size=domain.unwrap_or(0);},
    TileInputNeed::ElementDomain=>{self.element_domains.insert(field.as_ref().unwrap().0);},
    TileInputNeed::Find=>{let (binding,word,ty)=field.as_ref().unwrap();let mut context=TileSearch{tiles:&self.tiles,ty};tile=unsafe{tile_find_input(self.tiles.len() as u64,*binding as u64,*word,(&mut context as *mut TileSearch<'_>).cast(),tile_candidate,tile_same_type)} as usize;facts.found=u32::from(tile!=self.tiles.len());},
    TileInputNeed::Push=>{let (binding,word,ty)=field.take().unwrap();self.tiles.push(CheckedTile{source:TileOrigin::Input{binding,word,ty},width:TileWidth::Fixed(width),access:Access::Value});},
    TileInputNeed::Bind=>{self.bind_tile(source.text(syntax.as_ref().unwrap().name),tile);},
    TileInputNeed::Complete=>return Ok(cursor.index as usize),
    TileInputNeed::BadSyntax=>return self.fail(body.at(cursor.index as usize).span(),"B198","使用 let name = tile_input(dataset 或字段, 正整数宽度)"),
    TileInputNeed::Duplicate=>return self.fail(body.at(cursor.index as usize).span(),"B125","tile 输入名称不能覆盖现有绑定"),
    TileInputNeed::BadType=>return self.fail(source.node::<ExprNode>(syntax.as_ref().unwrap().field).span(),"B200","tile 输入需要普通可读值，不能捕获 write／atomic 数据或资源 view"),
    TileInputNeed::BadCapacity=>return self.fail(source.node::<ExprNode>(syntax.as_ref().unwrap().field).span(),"B200","tile 源容量不能小于阶段域容量"),
   }
  }
 }
 pub(super) async fn tile_publish(&mut self,annotated:bool,name:&str,args:&Expressions,span:Span,atomic:bool)->Result<()> {
  let mut cursor=SourceValueCursor{phase:0};let mut facts=TilePublishFacts{arguments:args.len() as u64,annotated:u32::from(annotated),root:u32::from(self.is_cooperative_root()),cooperative:u32::from(self.cooperative_root.is_some()),has_tile:u32::from(!self.tiles.is_empty()),atomic:u32::from(atomic),..Default::default()};
  let mut width=None;let mut value=None;let mut tile=0;
  loop {
   let step=unsafe{tile_publish_step(cursor,facts)};cursor=step.cursor;
   match step.need {
    TilePublishNeed::FixedWidth=>{width=Some(TileWidth::Fixed(unsafe{self.world.call(self.tile_width(&args.at(1))).await}?));},
    TilePublishNeed::InheritedWidth=>{width=Some(self.tiles.first().unwrap().width);},
    TilePublishNeed::Value=>{value=unsafe{self.world.call(self.expr(&args.at(0))).await}?;facts.present=u32::from(value.is_some());if let Some(value)=&value{facts.views=u32::from(value.ty.contains_views());facts.nonempty=u32::from(!value.ty.components.is_empty());}},
    TilePublishNeed::Publish=>{
     let mut value=value.take().unwrap();value.proof.under_path(&value.ty,&self.path);tile=self.tiles.len();
     let mut context=PublishWords{value:&value,values:Vec::new()};
     unsafe{tile_publication(value.ty.components.len() as u64,(&mut context as *mut PublishWords<'_>).cast(),publish_word,push_publish_word)};
     let values=context.values;self.tiles.push(CheckedTile{source:TileOrigin::Published(value),width:width.take().unwrap(),access:step.access});
     self.emit(InstKind::TilePublish(Publication{tile,values}));
    },
    TilePublishNeed::Bind=>self.bind_tile(name,tile),
    TilePublishNeed::Complete=>return Ok(()),
    TilePublishNeed::BadRoot=>return self.fail(span,"B203","使用协作根块的 let name = tile_publish／tile_atomic(值[, 正整数宽度])"),
    TilePublishNeed::BadFixed=>return self.fail(span,"B209","cooperative 函数继承借用块范围，发布不能指定固定宽度"),
    TilePublishNeed::MissingWidth=>return self.fail(span,"B209","省略发布宽度需要既有 tile 范围"),
    TilePublishNeed::BadValue=>return self.fail(args.at(0).span(),"B204","tile 发布需要普通值，不能发布资源 view 或能力句柄"),
   }
  }
 }
 async fn tile_width(&mut self,expr:&Expr)->Result<u32> {
  let result=unsafe{self.world.call(self.expr(expr)).await}?;let value=self.value_result(result,expr.span())?;
  let mut cursor=SourceValueCursor{phase:0};let mut facts=TileWidthFacts{first:width_range(self.tiles.first().map(|tile|tile.width)),constant:u32::from(value.constant.is_some()),scalar:u32::from(value.ty==Ty::scalar(Scalar::U32)),width:0};
  loop {let step=unsafe{tile_width_step(cursor,facts)};cursor=step.cursor;match step.need {
   TileWidthNeed::Read=>{facts.width=value.constant.as_ref().unwrap()[0] as u32;},
   TileWidthNeed::Complete=>return Ok(facts.width),
   TileWidthNeed::ConstantError=>return self.fail(expr.span(),"B199","tile 宽度需要编译期正 u32 值"),
   TileWidthNeed::WidthError=>return self.fail(expr.span(),"B199","一个阶段的 tile 必须使用相同的非零 u32 宽度"),
  }}
 }
 pub(super) fn tile_collective(&mut self,op:TileOp,args:&Expressions,span:Span)->Result<Value> {
  let mut cursor=SourceValueCursor{phase:0};let mut facts=TileCollectiveFacts{arguments:args.len() as u64,root:u32::from(self.is_cooperative_root()),sum:u32::from(op==TileOp::ReduceSum),..Default::default()};
  let mut view:Option<TileView>=None;let mut binding=None;let mut input=None;let mut proof=bounds::Proof::default();
  loop {let step=unsafe{tile_collective_step(op.operator(),cursor,facts)};cursor=step.cursor;match step.need {
   TileCollectiveNeed::View=>{let value=self.tile_view(&args.at(0))?;facts.atomic=u32::from(self.tiles[value.tile].access==Access::Atomic);binding=match &self.tiles[value.tile].source{TileOrigin::Input{binding,..}=>Some(*binding),_=>None};facts.input=u32::from(binding.is_some());view=Some(value);},
   TileCollectiveNeed::Read=>{self.reads.insert(binding.unwrap());self.cross_access.insert(binding.unwrap());},
   TileCollectiveNeed::Type=>{let ty=&view.as_ref().unwrap().ty;let numeric=ty.is_numeric();facts.numeric=u32::from(numeric);facts.integer=u32::from(numeric&&ty.components[0].0.is_integer());},
   TileCollectiveNeed::CommonProof=>{let view=view.as_ref().unwrap();input=self.common_tile_proof(view.tile,&view.words);facts.proof=u32::from(input.is_some());},
   TileCollectiveNeed::SumProof=>{let view=view.as_ref().unwrap();proof=self.tile_sum(view.tile,&view.words,&input.take().unwrap(),&view.ty,&view.length);},
   TileCollectiveNeed::Publish=>{
    let view=view.take().unwrap();let mut context=TileResults{builder:self,ty:&view.ty,words:&view.words,regs:Vec::new(),results:Vec::new()};
    unsafe{tile_results(view.ty.components.len() as u64,(&mut context as *mut TileResults<'_, '_>).cast(),TileResultHost{get:tile_component,allocate:tile_register,push:push_tile_result})};
    let regs=context.regs;let results=context.results;self.emit(InstKind::TileCollective(TileCollective{op,tile:view.tile,results}));
    return Ok(Value{proof,literal:false,ty:view.ty,regs,uniform:false,constant:None});
   },
   TileCollectiveNeed::BadRoot=>return self.fail(span,"B205","tile 集合运算需要协作根块的 let／值类型声明及一个 tile 参数"),
   TileCollectiveNeed::BadAtomic=>return self.fail(args.at(0).span(),"B214","atomic tile 需要先显式原子读取并发布不可变值，再参与集合运算"),
   TileCollectiveNeed::BadType=>return self.fail(args.at(0).span(),"B206","tile 集合运算需要数值标量／向量或其字段，按位操作只接受整数；不接受 bool、记录或资源"),
  }}
 }
 pub(super) fn tile_view(&self,expr:&Expr)->Result<TileView> {
  let mut context=TileProjection{builder:self,expression:expr,value:None};
  unsafe{tile_project(expr.signature_node(),(&mut context as *mut TileProjection<'_, '_>).cast(),TileProjectionHost{root:tile_projection_root,member:tile_projection_member,invalid:tile_projection_invalid})};
  context.value.unwrap()
 }
 pub(super) fn bind_tile(&mut self,name:&str,tile:usize) {
  let ty=self.tiles[tile].ty().clone();let mut context=TypeWords{ty:&ty,words:Vec::new()};
  unsafe{tile_words(ty.components.len() as u64,(&mut context as *mut TypeWords<'_>).cast(),type_word,push_type_word)};
  let words=context.words;let length=bounds::Proof::unsigned(1,self.tile_width_bound()).with_domains(Domains::from([IndexDomain::TileLength])).with_identity(ValueIdentity::TileLength);
  self.env.insert(name.into(),Binding::Tile(TileView{tile,ty,words,length}));
 }
 pub(super) fn is_cooperative_root(&self)->bool {unsafe{tile_root(u32::from(self.in_function),u32::from(self.cooperative_root.is_some()),self.cooperative_root.unwrap_or(0) as u64,self.block_depth as u64)!=0}}
 pub(super) fn tile_barrier(&mut self,args:&Expressions,span:Span)->Result<()> {
  if unsafe{tile_barrier_allowed(args.len() as u64,u32::from(self.is_cooperative_root()),self.tiles.len() as u64)}==0{return self.fail(span,"B219","tile_barrier() 需要既有 tile 范围，只能位于 parallel／cooperative 根块");}
  self.emit(InstKind::TileBarrier);Ok(())
 }
 pub(super) fn lower_tiles(&mut self,span:Span)->Result<Vec<Tile>> {
  let mut source=std::mem::take(&mut self.tiles).into_iter();let mut cursor=SourceValueCursor{phase:0};let mut facts=TileLowerFacts::default();let mut value=None;let mut width=None;let mut output=Vec::new();
  loop {let step=unsafe{tile_lower_step(cursor,facts)};cursor=step.cursor;match step.need {
   TileLowerNeed::Next=>{
    let next=source.next();facts.present=u32::from(next.is_some());
    if let Some(tile)=next {
     value=match tile.source{TileOrigin::Input{binding,word,ty}=>Some((TileSource::Input{binding,word},ty)),TileOrigin::Published(value)=>Some((TileSource::Published,value.ty)),TileOrigin::Parameter(_)=>None};
     width=match tile.width{TileWidth::Fixed(width)=>Some(width),TileWidth::Parameter=>None};facts.actual=u32::from(value.is_some());facts.fixed=u32::from(width.is_some());
    }
   },
   TileLowerNeed::Publish=>{let (source,ty)=value.take().unwrap();output.push(Tile{source,ty,width:width.take().unwrap()});},
   TileLowerNeed::Complete=>return Ok(output),
   TileLowerNeed::Invalid=>return self.fail(span,"B207","program 的 tile 必须具有实际来源与具体逻辑块宽度"),
  }}
 }
 pub(super) fn tile_length(&mut self,length:bounds::Proof)->Value {
  let r=self.reg(Scalar::U32);self.emit(InstKind::TileLength(r));Value{proof:length,literal:false,ty:Ty::scalar(Scalar::U32),regs:vec![r],uniform:false,constant:None}
 }
 pub(super) fn tile_lane(&mut self)->Value {
  let r=self.reg(Scalar::U32);self.emit(InstKind::TileLane(r));let plan=unsafe{tile_lane_plan(width_range(self.tiles.first().map(|tile|tile.width)))};
  let mut proof=bounds::Proof::unsigned(0,plan.maximum).with_domains(Domains::from([IndexDomain::Tile]));
  if plan.mark!=0{proof.mark_domain(0,IndexDomain::Computed(bounds::IntegerExpression::Binary(bounds::IntegerOperation::Remainder,Scalar::U32,bounds::Operand::Value(ValueIdentity::Register(self.global_index)),bounds::Operand::Constant(Scalar::U32,Bits::from(plan.width)))));}
  let value=Value{proof,literal:false,ty:Ty::scalar(Scalar::U32),regs:vec![r],uniform:false,constant:None};self.observe_bounds(&value);value
 }
 fn tile_width_bound(&self)->u32 {unsafe{tile_width_bound(width_range(self.tiles.first().map(|tile|tile.width)))}}
 pub(super) fn tile_index_proof(&self)->bounds::Proof {let plan=unsafe{tile_lane_plan(width_range(self.tiles.first().map(|tile|tile.width)))};bounds::Proof::unsigned(0,plan.maximum)}
 pub(super) async fn tile_place(&mut self,view:TileView,index:&Expr)->Result<Option<Place>> {
  let value=unsafe{self.world.call(self.expr(index)).await}?;let mut cursor=SourceValueCursor{phase:0};let mut range=TileRange::default();let mut count=None;let mut minimum=1;let mut facts=TilePlaceFacts{present:u32::from(value.is_some()),..Default::default()};
  loop {let step=unsafe{tile_place_step(cursor,facts)};cursor=step.cursor;match step.need {
   TilePlaceNeed::Type=>self.require_same(&Ty::scalar(Scalar::U32),&value.as_ref().unwrap().ty,index.span())?,
   TilePlaceNeed::Range=>{range=width_range(Some(self.tiles[view.tile].width));facts.fixed=range.fixed;facts.prefix=u32::from(self.prefix.is_some());},
   TilePlaceNeed::Count=>{count=self.program.buffers[self.domain].count;facts.known=u32::from(count.is_some());},
   TilePlaceNeed::Minimum=>{minimum=unsafe{tile_index_minimum(range.width,count.unwrap())};},
   TilePlaceNeed::Bound=>{facts.bounded=u32::from(self.index_in(value.as_ref().unwrap(),IndexDomain::Tile,minimum,None));},
   TilePlaceNeed::Publish=>return Ok(Some(Place::TileElement{ty:view.ty,tile:view.tile,index:value.unwrap().regs[0],words:view.words})),
   TilePlaceNeed::Complete=>return Ok(None),
   TilePlaceNeed::Invalid=>return self.fail(index.span(),"B201","tile 索引需要本块有界游标、lane、有效长度条件或在所有尾块内有效的常量"),
  }}
 }
}
