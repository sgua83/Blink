//! dataset 与块内原子共用位置、类型、索引证明及显式内存序。
use super::*;
use std::ffi::c_void;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum AtomicSourceKind { Unknown, Load, Store, Add, Sub, Exchange, CompareExchange, Min, Max, And, Or, Xor }
#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct AtomicBuiltin {kind:AtomicSourceKind,access:Access,arity:u32,order_index:u32}
#[allow(dead_code)]#[repr(u32)]enum AtomicOrder {Unknown,Relaxed,Acquire,Release,AcquireRelease}
#[derive(Clone,Copy,Default)]#[repr(C)]struct LocationCursor {phase:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct LocationFacts {valid:u32,kind:u32,offset:u32}
#[allow(dead_code)]#[repr(u32)]enum LocationNeed {Place,Type,BadType,Access,Effect,Position,Data,Tile,Array,Reference,Application,Complete}
#[repr(C)]struct LocationStep {cursor:LocationCursor,need:LocationNeed}
#[derive(Clone,Copy,Default)]#[repr(C)]struct Cursor {phase:u32}
#[derive(Clone,Copy,Default)]#[repr(C)]struct Facts {count:u64,present:u32,order:u32,failure:u32,location:u32,data:u32}
#[allow(dead_code)]#[repr(u32)]enum Instruction {Load,Store,Add,Sub,Exchange,CompareExchange,Min,Max,And,Or,Xor}
#[allow(dead_code)]#[repr(u32)]enum Need {BadStoreArity,BadArity,Location,Order,Failure,Value,Desired,ValueType,DesiredType,BadStoreOrder,BadLoadOrder,BadFailureOrder,BadOperation,Build,Result,Proof,Access,Read,Write,Cross,Emit,Empty,Complete}
#[repr(C)]struct Step {cursor:Cursor,need:Need,operation:Instruction,stored:u32}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::source_atomic_name"]fn source_atomic_name(bytes:*const u8,length:usize,statement:u32)->AtomicBuiltin;
    #[link_name="bootstrap.operator_types::atomic_storage"]fn storage(kind:u32,access:u32)->u32;
    #[link_name="bootstrap.operator_types::atomic_scalar"]fn atomic_scalar(context:*const c_void,same:unsafe extern "C" fn(*const c_void,u32)->u32)->u32;
    #[link_name="bootstrap.operator_types::source_atomic_order"]fn source_order(named:u32,bytes:*const u8,length:usize)->AtomicOrder;
    #[link_name="bootstrap.operator_types::atomic_location_step"]fn location_step(cursor:LocationCursor,facts:LocationFacts)->LocationStep;
    #[link_name="bootstrap.operator_types::atomic_step"]fn step(cursor:Cursor,builtin:AtomicBuiltin,facts:Facts)->Step;
}
unsafe extern "C" fn scalar_type(context:*const c_void,index:u32)->u32 {
    let ty=unsafe{&*context.cast::<Ty>()};
    u32::from(*ty==Ty::scalar([Scalar::U32,Scalar::I32,Scalar::U64,Scalar::I64][index as usize]))
}
impl AtomicBuiltin {
    pub(super) fn from_source(name:&str,statement:bool)->Option<Self> {
        let builtin=unsafe{source_atomic_name(name.as_ptr(),name.len(),u32::from(statement))};
        (builtin.kind!=AtomicSourceKind::Unknown).then_some(builtin)
    }
}

impl Builder<'_> {
    pub(super) fn atomic_storage(&self,place:&Place)->bool {
        let (kind,access)=match place.storage(){
            Place::AtomicParameter(_)=>(1,Access::Atomic),
            Place::Reference{access,..}=>(2,*access),
            Place::ApplicationElement{access,..}=>(3,*access),
            Place::Element{binding,..}=>(4,self.view_access[binding]),
            Place::TileElement{tile,..}=>(5,self.tiles[*tile].access),
            _=>(0,Access::Value),
        };
        unsafe{storage(kind,access as u32)!=0}
    }
    pub(super) async fn atomic_place(&mut self,target:&Expr)->Result<Option<Place>> {
        let Some(place)=unsafe{self.world.call(self.place(target)).await}? else{return Ok(None);};
        self.atomic_place_value(place,target.span())
    }
    pub(super) fn atomic_place_value(&mut self,place:Place,span:Span)->Result<Option<Place>> {
        if self.atomic_storage(&place){Ok(Some(place))}
        else{self.fail(span,"B214","原子借用需要显式 atomic dataset／tile 字段或已有原子参数")}
    }
    pub(super) async fn atomic_position_borrows(&self, place: &Place, span: Span) -> Result<view::Borrow> {
        let borrow = unsafe { self.world.call(self.position_borrows(place, span)).await }?;
        Ok(borrow.map_sources(|source| match source {
            view::Source::Position(mut origin) => {
                Arc::make_mut(&mut origin).writable = true;
                view::Borrow::source(view::Source::Position(origin))
            }
            source => view::Borrow::source(source),
        }))
    }

    async fn atomic_location(&mut self,target:&Expr,access:Access)->Result<Option<(Place,Option<(IndexedSource,Option<Reg>,u32)>,Scalar)>> {
        let mut cursor=LocationCursor::default();let mut facts=LocationFacts::default();
        let mut place:Option<Place>=None;let mut scalar=None;let mut offset=None;let mut word=None;let mut location=None;
        loop {
            let next=unsafe{location_step(cursor,facts)};cursor=next.cursor;
            match next.need {
                LocationNeed::Place=>{place=unsafe{self.world.call(self.atomic_place(target)).await}?;if place.is_none(){return Ok(None);}},
                LocationNeed::Type=>{facts.valid=unsafe{atomic_scalar(std::ptr::from_ref(place.as_ref().unwrap().ty()).cast(),scalar_type)};},
                LocationNeed::BadType=>return self.fail(target.span(),"B215","原子目标只支持 u32/i32/u64/i64 标量字段"),
                LocationNeed::Access=>{
                    scalar=Some(place.as_ref().unwrap().ty().components[0].0);
                    unsafe{self.world.call(self.require_place_access(place.as_ref().unwrap(),access,target.span())).await}?;
                },
                LocationNeed::Effect => {
                    let place = place.as_ref().unwrap();
                    let span = target.span();
                    if matches!(place.storage(), Place::Reference { .. }) {
                        let borrow = unsafe { self.world.call(self.atomic_position_borrows(place, span)).await }?;
                        self.check_view_requirement(view::Requirement::Dataset {
                            borrow, index: None, access, atomic: true, values: None, span,
                        })?;
                    } else {
                        self.dataset_effect(place, access, true, span)?;
                    }
                },
                LocationNeed::Position=>{
                    let p=place.as_ref().unwrap();
                    match p {
                        Place::Indexed{offset:at,words,..}=>{offset=Some(*at);word=Some(words[0]);},
                        Place::Element{words,..}|Place::TileElement{words,..}|Place::Reference{words,..}|Place::ApplicationElement{words,..}=>{word=Some(words[0]);},
                        Place::AtomicParameter(_)=>{},
                        _=>unreachable!(),
                    }
                    facts.kind=match p.storage(){Place::AtomicParameter(_)=>0,Place::Element{..}=>1,Place::TileElement{..}=>2,Place::Reference{..}=>3,Place::ApplicationElement{..}=>4,_=>unreachable!()};
                    facts.offset=u32::from(offset.is_some());
                },
                LocationNeed::Array=>{
                    let (source,at,byte)=self.array_memory(place.as_ref().unwrap().storage(),offset.unwrap(),word.unwrap(),scalar.unwrap(),access.writes(),target.span())?;
                    location=Some((source,Some(at),byte));
                },
                LocationNeed::Data|LocationNeed::Tile|LocationNeed::Reference|LocationNeed::Application=>{
                    let word=word.unwrap();
                    let source=match place.as_ref().unwrap().storage(){
                        Place::Element{binding,index,..}=>IndexedSource::Data{binding:*binding,index:*index},
                        Place::TileElement{tile,index,..}=>IndexedSource::Tile{tile:*tile,index:*index},
                        Place::Reference{address,..}=>{
                            let(address,byte)=address.component(word);let index=self.constant(Scalar::U32,0).regs[0];
                            location=Some((IndexedSource::View{address,index,stride:ElementStride::Constant(0)},None,byte));continue;
                        },
                        Place::ApplicationElement{handle,identity,index,stride,..}=>{
                            let(address,index,stride)=self.application_memory(*handle,*identity,*index,*stride,access.writes(),None,[(scalar.unwrap(),word)],target.span())?;
                            IndexedSource::View{address,index,stride:ElementStride::Constant(u64::from(stride))}
                        },
                        _=>unreachable!(),
                    };
                    location=Some((source,offset,word));
                },
                LocationNeed::Complete=>return Ok(Some((place.take().unwrap(),location,scalar.unwrap()))),
            }
        }
    }
    fn atomic_order(&self,expr:&Expr)->Result<MemoryOrder> {
        let(named,bytes,length)=match expr.kind(){ExprKind::Name(name)=>(1,name.as_ptr(),name.len()),_=>(0,std::ptr::null(),0)};
        match unsafe{source_order(named,bytes,length)} {
            AtomicOrder::Relaxed=>Ok(MemoryOrder::Relaxed),AtomicOrder::Acquire=>Ok(MemoryOrder::Acquire),
            AtomicOrder::Release=>Ok(MemoryOrder::Release),AtomicOrder::AcquireRelease=>Ok(MemoryOrder::AcquireRelease),
            AtomicOrder::Unknown=>self.fail(expr.span(),"B216","原子内存序需要显式 relaxed、acquire、release 或 acq_rel"),
        }
    }
    pub(super) async fn atomic_operation(&mut self,builtin:AtomicBuiltin,name:&str,args:&Expressions,span:Span)->Result<Option<Value>> {
        let mut cursor=Cursor::default();let mut facts=Facts{count:args.len() as u64,..Default::default()};
        let mut place=None;let mut location=None;let mut scalar=None;let mut binding=None;let mut value:Option<Value>=None;let mut desired:Option<Value>=None;
        let mut order=None;let mut failure=None;let mut op=None;let mut result=None;let mut output=None;
        loop {
            let next=unsafe{step(cursor,builtin,facts)};cursor=next.cursor;
            match next.need {
                Need::BadStoreArity=>return self.fail(span,"B217","atomic_store(目标, 值, 内存序) 需要三个参数"),
                Need::BadArity=>return self.fail(span,"B217",format!("{name} 需要 {} 个参数，最后显式提供内存序；比较交换提供成功与失败两种序",builtin.arity)),
                Need::Location=>{
                    let target=unsafe{self.world.call(self.atomic_location(&args.at(0),builtin.access)).await}?;facts.present=u32::from(target.is_some());
                    if let Some((p,at,s))=target{
                        place=Some(p);location=at;scalar=Some(s);facts.location=u32::from(location.is_some());
                        binding=match &location{Some((IndexedSource::Data{binding,..},..))=>Some(*binding),_=>None};facts.data=u32::from(binding.is_some());
                    }
                },
                Need::Order=>{let at=self.atomic_order(&args.at(builtin.order_index as usize))?;facts.order=at as u32;order=Some(at);},
                Need::Failure=>{let at=self.atomic_order(&args.at(4))?;facts.failure=at as u32;failure=Some(at);},
                Need::Value=>{value=unsafe{self.world.call(self.expr(&args.at(1))).await}?;facts.present=u32::from(value.is_some());},
                Need::Desired=>{desired=unsafe{self.world.call(self.expr(&args.at(2))).await}?;facts.present=u32::from(desired.is_some());},
                Need::ValueType=>self.require_same(&Ty::scalar(scalar.unwrap()),&value.as_ref().unwrap().ty,args.at(1).span())?,
                Need::DesiredType=>self.require_same(&Ty::scalar(scalar.unwrap()),&desired.as_ref().unwrap().ty,args.at(2).span())?,
                Need::BadStoreOrder=>return self.fail(args.at(builtin.order_index as usize).span(),"B216","atomic_store 的内存序只能是 relaxed 或 release"),
                Need::BadLoadOrder=>return self.fail(args.at(builtin.order_index as usize).span(),"B216","atomic_load 的内存序只能是 relaxed 或 acquire"),
                Need::BadFailureOrder=>return self.fail(args.at(4).span(),"B216","比较交换失败序不得带 release，也不得强于成功序"),
                Need::BadOperation=>return self.fail(span,"B158",format!("未知原子操作 {name}")),
                Need::Build=>{
                    op=Some(match next.operation {
                        Instruction::Load=>AtomicOp::Load,Instruction::Store=>AtomicOp::Store(value.as_ref().unwrap().regs[0]),
                        Instruction::Add=>AtomicOp::Add(value.as_ref().unwrap().regs[0]),Instruction::Sub=>AtomicOp::Sub(value.as_ref().unwrap().regs[0]),
                        Instruction::Exchange=>AtomicOp::Exchange(value.as_ref().unwrap().regs[0]),
                        Instruction::CompareExchange=>AtomicOp::CompareExchange{expected:value.as_ref().unwrap().regs[0],desired:desired.as_ref().unwrap().regs[0],failure:failure.unwrap()},
                        Instruction::Min=>AtomicOp::Min(value.as_ref().unwrap().regs[0]),Instruction::Max=>AtomicOp::Max(value.as_ref().unwrap().regs[0]),
                        Instruction::And=>AtomicOp::And(value.as_ref().unwrap().regs[0]),Instruction::Or=>AtomicOp::Or(value.as_ref().unwrap().regs[0]),Instruction::Xor=>AtomicOp::Xor(value.as_ref().unwrap().regs[0]),
                    });
                },
                Need::Result=>{result=Some(self.reg(scalar.unwrap()));},
                Need::Proof=>{
                    let stored=match next.stored{1=>value.as_ref().unwrap(),_=>desired.as_ref().unwrap()};
                    output=Some(self.path_proof(&stored.ty,bounds::Proof::capture(stored)).element_facts(|binding|bounds::Proof::default().with_domains(Domains::from([IndexDomain::Data(binding)]))));
                },
                Need::Access=>self.data_access(place.as_ref().unwrap(),Access::Write,output.as_ref()),
                Need::Read=>{self.reads.insert(binding.unwrap());},
                Need::Write=>{self.writes.insert(binding.unwrap());},
                Need::Cross=>{self.cross_access.insert(binding.unwrap());},
                Need::Emit=>{let(target,offset,word)=location.take().unwrap();self.emit(InstKind::Atomic(Atomic{result,scalar:scalar.unwrap(),target,word,offset,op:op.take().unwrap(),order:order.unwrap()}));},
                Need::Empty=>return Ok(None),
                Need::Complete=>return Ok(Some(Value{proof:bounds::Proof::default(),literal:false,ty:Ty::scalar(scalar.unwrap()),regs:vec![result.unwrap()],uniform:false,constant:None})),
            }
        }
    }
}
