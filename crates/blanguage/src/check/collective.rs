use super::*;

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Form { Unknown, TileReduce, TileScan, Reduce, SegmentedReduce, Scan, SegmentedScan }

#[repr(C)]
pub(super) struct Builtin {
    pub(super) form: Form,
    pub(super) operation: CollectiveOperator,
}

unsafe extern "C" {
    #[link_name = "bootstrap.operator_types::source_collective"]
    fn source_collective(bytes: *const u8, length: usize) -> Builtin;
    #[link_name = "bootstrap.operator_types::collective_accepts"]
    fn collective_accepts(operation: CollectiveOperator, numeric: u32, integer: u32) -> u32;
}

pub(super) fn from_source(name: &str) -> Builtin {
    unsafe { source_collective(name.as_ptr(), name.len()) }
}

impl Builtin {
    pub(super) fn is_tile(&self) -> bool {
        matches!(self.form, Form::TileReduce | Form::TileScan)
    }

    pub(super) fn is_stage(&self) -> bool {
        matches!(self.form, Form::Reduce | Form::SegmentedReduce | Form::Scan | Form::SegmentedScan)
    }

    pub(super) fn tile(self) -> Option<TileOp> {
        use CollectiveOperator::*;
        use Form::*;
        Some(match (self.form, self.operation) {
            (TileReduce, Sum) => TileOp::ReduceSum,
            (TileReduce, Min) => TileOp::ReduceMin,
            (TileReduce, Max) => TileOp::ReduceMax,
            (TileReduce, BitAnd) => TileOp::ReduceBitAnd,
            (TileReduce, BitOr) => TileOp::ReduceBitOr,
            (TileReduce, BitXor) => TileOp::ReduceBitXor,
            (TileScan, Sum) => TileOp::ScanExclusiveSum,
            (TileScan, Min) => TileOp::ScanExclusiveMin,
            (TileScan, Max) => TileOp::ScanExclusiveMax,
            (TileScan, BitAnd) => TileOp::ScanExclusiveBitAnd,
            (TileScan, BitOr) => TileOp::ScanExclusiveBitOr,
            (TileScan, BitXor) => TileOp::ScanExclusiveBitXor,
            _ => return None,
        })
    }
}

pub(super) fn accepts(operation: CollectiveOperator, ty: &Ty) -> bool {
    let numeric = ty.is_numeric();
    let integer = numeric && ty.components[0].0.is_integer();
    unsafe { collective_accepts(operation, u32::from(numeric), u32::from(integer)) != 0 }
}

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum NumericKind { Reduce, Scan, SegmentedScan, SegmentedReduce }
#[allow(dead_code)]
#[repr(u32)]
enum NumericNeed { Source, Target, Head, Count, Types, Access, TargetShape, CountShape, Capacity, Fields, CountWrite, GrantInput, GrantOutput, Reads, Publish, Error }
#[allow(dead_code)]
#[repr(u32)]
enum NumericError { ReduceArity, ScanArity, SegmentedArity, ReduceType, ScanType, SegmentedType, HeadAccess, ReduceAccess, ScanAccess, SegmentedAccess, ReduceShape, CountShape, ScanCapacity, SegmentedCapacity }
#[allow(dead_code)]
#[repr(u32)]
enum NumericOperation { Reduce, Scan, SegmentedReduce }
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct NumericFacts {
    arguments:u64,source:u64,target:u64,count:u64,prefix_count:u64,heads:u64,head_count:u64,
    has_prefix:u32,has_head_prefix:u32,prefix_word:u32,head_count_word:u32,
    source_access:u32,target_access:u32,head_access:u32,count_access:u32,
    numeric:u32,integer:u32,same:u32,source_overlap:u32,head_overlap:u32,
    source_known:u32,source_size:u32,target_known:u32,target_size:u32,count_known:u32,count_size:u32,
}
#[repr(C)]
struct NumericStep { cursor:SourceValueCursor,need:NumericNeed,error:NumericError,operation:NumericOperation,argument:u64 }
#[repr(C)]
struct CollectiveComponent { offset:u32,size:u32 }
#[repr(C)]
struct CollectiveReads { first:u64,second:u64,third:u64,count:u64 }
unsafe extern "C" {
    #[link_name="bootstrap.function_body::numeric_step"]
    fn numeric_step(kind:NumericKind,operation:CollectiveOperator,cursor:SourceValueCursor,facts:NumericFacts)->NumericStep;
    #[link_name="bootstrap.function_body::collective_write_fields"]
    fn collective_write_fields(ty:*const std::ffi::c_void,context:*mut std::ffi::c_void,base:u32,count:u64,
        component:unsafe extern "C" fn(*const std::ffi::c_void,u64)->CollectiveComponent,
        write:unsafe extern "C" fn(*mut std::ffi::c_void,u32,u32));
    #[link_name="bootstrap.function_body::collective_write_count"]
    fn collective_write_count(context:*mut std::ffi::c_void,word:u32,write:unsafe extern "C" fn(*mut std::ffi::c_void,u32,u32));
    #[link_name="bootstrap.function_body::collective_reads"]
    fn collective_reads(source:u64,second_present:u32,second:u64,third_present:u32,third:u64)->CollectiveReads;
}

struct CollectiveWrite<'a> {
    program:&'a Program,facts:&'a mut prefix::Facts,binding:usize,
    regions:&'a mut BTreeMap<usize,BTreeSet<(u32,u32)>>,proof:Option<&'a bounds::Proof>,
}
unsafe extern "C" fn collective_component(ty:*const std::ffi::c_void,index:u64)->CollectiveComponent {
    let ty=unsafe{&*ty.cast::<Ty>()};let (scalar,offset)=ty.components[index as usize];
    CollectiveComponent{offset,size:scalar.size()}
}
unsafe extern "C" fn collective_written(context:*mut std::ffi::c_void,start:u32,end:u32) {
    let context=unsafe{&mut *context.cast::<CollectiveWrite<'_>>()};
    context.regions.entry(context.binding).or_default().insert((start,end));
    context.facts.written(context.program,context.binding,start,end,context.proof);
}

pub(super) fn compile(
    program:&mut Program,env:&BTreeMap<String,Root>,operator:CollectiveOperator,kind:NumericKind,
    args:&Expressions,d:&Definition,span:Span,facts:&mut prefix::Facts,
    operation_source:Option<Arc<SourceSite>>,
)->Result<()> {
    let mut cursor=SourceValueCursor{phase:0};
    let mut control=NumericFacts{arguments:args.len() as u64,..Default::default()};
    let mut source=None;let mut target=None;let mut head=None;let mut count=None;
    let mut write_regions=BTreeMap::new();let mut prefix_guarantees=Vec::new();let mut reads=Vec::new();
    loop {
        let step=unsafe{numeric_step(kind,operator,cursor,control)};cursor=step.cursor;
        match step.need {
            NumericNeed::Source=>{
                let value=prefix::typed_field(program,env,&args.at(step.argument as usize),d,facts)?;
                control.source=value.0 as u64;control.source_access=value.2 as u32;
                if let Some(prefix)=value.3 {control.has_prefix=1;control.prefix_count=prefix.binding as u64;control.prefix_word=prefix.word;}
                source=Some(value);
            },
            NumericNeed::Target=>{
                let value=collective_value_field(program,env,&args.at(step.argument as usize),d)?;
                control.target=value.0 as u64;control.target_access=value.2 as u32;target=Some(value);
            },
            NumericNeed::Head=>{
                let value=prefix::field(program,env,&args.at(step.argument as usize),d,facts,Scalar::Bool)?;
                control.heads=value.0 as u64;control.head_access=value.2 as u32;
                if let Some(prefix)=value.3 {control.has_head_prefix=1;control.head_count=prefix.binding as u64;control.head_count_word=prefix.word;}
                head=Some(value);
            },
            NumericNeed::Count=>{
                let value=collective_field(program,env,&args.at(step.argument as usize),d,Scalar::U32)?;
                control.count=value.0 as u64;control.count_access=value.2 as u32;count=Some(value);
            },
            NumericNeed::Types=>{
                let ty=&source.as_ref().unwrap().4;control.numeric=u32::from(ty.is_numeric());
                control.integer=u32::from(ty.is_numeric()&&ty.components[0].0.is_integer());
                control.same=u32::from(*ty==target.as_ref().unwrap().3);
            },
            NumericNeed::Access=>{
                let (source,word,_,_,ty)=source.as_ref().unwrap();let (target,target_word,_,target_ty)=target.as_ref().unwrap();
                control.source_overlap=u32::from(prefix::fields_overlap(*source,*word,ty,*target,*target_word,target_ty));
                if let Some((heads,word,_,_))=head {control.head_overlap=u32::from(prefix::fields_overlap(heads,word,&Ty::scalar(Scalar::Bool),*target,*target_word,target_ty));}
            },
            NumericNeed::TargetShape=>{
                let size=program.buffers[target.as_ref().unwrap().0].count;
                control.target_known=u32::from(size.is_some());control.target_size=size.unwrap_or(0);
            },
            NumericNeed::CountShape=>{
                let size=program.buffers[count.unwrap().0].count;
                control.count_known=u32::from(size.is_some());control.count_size=size.unwrap_or(0);
            },
            NumericNeed::Capacity=>{
                let source_size=program.buffers[source.as_ref().unwrap().0].count;
                let target_size=program.buffers[target.as_ref().unwrap().0].count;
                control.source_known=u32::from(source_size.is_some());control.source_size=source_size.unwrap_or(0);
                control.target_known=u32::from(target_size.is_some());control.target_size=target_size.unwrap_or(0);
            },
            NumericNeed::Fields=>{
                let ty=&source.as_ref().unwrap().4;let (binding,base,_,_)=target.as_ref().unwrap();
                let mut context=CollectiveWrite{program,facts,binding:*binding,regions:&mut write_regions,proof:None};
                unsafe{collective_write_fields((ty as *const Ty).cast(),(&mut context as *mut CollectiveWrite<'_>).cast(),*base,ty.components.len() as u64,collective_component,collective_written)};
            },
            NumericNeed::CountWrite=>{
                let (binding,word,_)=count.unwrap();let mut context=CollectiveWrite{program,facts,binding,regions:&mut write_regions,proof:None};
                unsafe{collective_write_count((&mut context as *mut CollectiveWrite<'_>).cast(),word,collective_written)};
            },
            NumericNeed::GrantInput=>{
                let relation=PrefixRelation{domain_region:None,domain:target.as_ref().unwrap().0,count:source.as_ref().unwrap().3.unwrap()};
                facts.grant(relation);prefix_guarantees.push(relation);
            },
            NumericNeed::GrantOutput=>{
                let (binding,word,_)=count.unwrap();
                let relation=PrefixRelation{domain_region:None,domain:target.as_ref().unwrap().0,count:PrefixCount{binding,word}};
                facts.grant(relation);prefix_guarantees.push(relation);
            },
            NumericNeed::Reads=>{
                let value=unsafe{collective_reads(control.source,control.has_prefix,control.prefix_count,0,0)};
                reads.extend_from_slice(&[value.first as usize,value.second as usize,value.third as usize][..value.count as usize]);
            },
            NumericNeed::Publish=>{
                let (source,source_word,_,prefix,ty)=source.unwrap();let (target,target_word,_,_)=target.unwrap();
                let fields=Reduction{source,source_word,target,target_word};let mut writes=vec![target];
                let operation=match step.operation {
                    NumericOperation::Reduce=>Operation::Reduce(ReductionSpec{fields,ty,operator}),
                    NumericOperation::Scan=>Operation::ScanExclusive(Scan{values:fields,ty,operator,head_word:head.map(|(_,word,_,_)|word)}),
                    NumericOperation::SegmentedReduce=>{
                        let (binding,word,_)=count.unwrap();writes.push(binding);
                        Operation::SegmentedReduce(SegmentedReduction{values:ReductionSpec{fields,ty,operator},head_word:head.unwrap().1,count:PrefixCount{binding,word}})
                    },
                };
                program.stages.push(Stage{operation_source,generated_origin:None,domain:source,prefix,selection:None,operation,boundary_before:true,
                    registers:Vec::new(),locals:Default::default(),code:Vec::new(),reads:reads.clone(),writes,write_regions,prefix_guarantees,
                    element_guarantees:Vec::new(),cross_access:reads,tiles:Vec::new()});
                return Ok(());
            },
            NumericNeed::Error=>{
                let (code,message)=match step.error {
                    NumericError::ReduceArity=>("B169","归约需要源数值字段和输出字段"),
                    NumericError::ScanArity=>("B169","扫描需要源字段和输出字段；分段扫描还需要源记录的 bool 段起点字段"),
                    NumericError::SegmentedArity=>("B169","分段归约需要源数值、同源 bool 段头、目标数值和 u32 计数字段"),
                    NumericError::ReduceType=>("B175","归约需要相同类型的数值标量／向量字段；按位归约只接受整数"),
                    NumericError::ScanType=>("B175","扫描需要同类型数值标量／向量字段；按位算子只接受整数"),
                    NumericError::SegmentedType=>("B175","分段归约需要同类型数值标量／向量；按位算子只接受整数"),
                    NumericError::HeadAccess=>("B170","分段起点必须来自同一可读输入记录，和源值共享元素域"),
                    NumericError::ReduceAccess=>("B170","归约需要可读输入和可写输出，实际字段不能重叠或改写输入域计数"),
                    NumericError::ScanAccess=>("B170","扫描需要可读输入和可写输出，目标不能覆盖源值、段头或输入域计数"),
                    NumericError::SegmentedAccess=>("B170","分段归约需要同域可读数值与段头、非重叠可写目标及独立输出计数"),
                    NumericError::ReduceShape=>("B171","归约输出形状必须为 1"),
                    NumericError::CountShape=>("B171","分段归约计数必须为形状 1 的 u32 计数字段"),
                    NumericError::ScanCapacity=>("B186","扫描输出形状必须满足输入域容量"),
                    NumericError::SegmentedCapacity=>("B188","分段归约目标容量必须容纳最坏情况下的全部输入段"),
                };
                let span=match step.error{NumericError::CountShape=>args.at(3).span(),_=>span};
                return d.source.fail(span,code,message);
            },
        }
    }
}

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum RecordKind {Compact,Sort}
#[allow(dead_code)]
#[repr(u32)]
enum RecordNeed {Source,Predicate,Target,Count,Views,FlagInitialized,Types,CountShape,PredicateShape,Capacity,Key,RootRead,SourceFacts,Elements,JoinElements,Guarantees,Fields,CountWrite,GrantInput,GrantOutput,Reads,Publish,Error,KeyComponents,SameLength}
#[allow(dead_code)]
#[repr(u32)]
enum RecordError {CompactArity,SortArity,Descending,CompactViews,SortViews,CompactAccess,KeyAccess,KeyType,SortAccess,CompactType,SortType,CountShape,PredicateShape,CompactCapacity,SortCapacity}
#[derive(Clone,Copy)]
#[repr(C)]
struct RecordCursor {index:u64,phase:u32}
#[derive(Clone,Copy,Default)]
#[repr(C)]
struct RecordFacts {
    source:u64,predicate:u64,target:u64,count:u64,prefix_count:u64,key_binding:u64,
    has_prefix:u32,source_access:u32,predicate_access:u32,target_access:u32,count_access:u32,
    views:u32,initialized:u32,same_type:u32,count_known:u32,count_size:u32,
    source_known:u32,source_size:u32,predicate_known:u32,predicate_size:u32,target_known:u32,target_size:u32,
    key_access:u32,key_same_prefix:u32,key_numeric:u32,root_read:u32,source_proof:u32,same_length:u32,
}
#[repr(C)]
struct RecordStep {cursor:RecordCursor,need:RecordNeed,error:RecordError,argument:u64,expression:u64,order:u32,set_source:u32}
#[repr(C)]
struct RecordComponent {scalar:u32,offset:u32,size:u32}
unsafe extern "C" {
    #[link_name="bootstrap.function_body::record_step"]
    fn record_step(kind:RecordKind,cursor:RecordCursor,source:crate::syntax::wire::StorageView,arguments:crate::syntax::wire::Slice,facts:RecordFacts)->RecordStep;
    #[link_name="bootstrap.function_body::record_sort_components"]
    fn record_sort_components(ty:*const std::ffi::c_void,keys:*mut std::ffi::c_void,word:u32,order:u32,count:u64,
        component:unsafe extern "C" fn(*const std::ffi::c_void,u64)->RecordComponent,
        push:unsafe extern "C" fn(*mut std::ffi::c_void,u32,u32,u32));
}
unsafe extern "C" fn record_component(ty:*const std::ffi::c_void,index:u64)->RecordComponent {
    let ty=unsafe{&*ty.cast::<Ty>()};let (scalar,offset)=ty.components[index as usize];
    RecordComponent{scalar:scalar as u32,offset,size:scalar.size()}
}
unsafe extern "C" fn record_key_push(keys:*mut std::ffi::c_void,word:u32,scalar:u32,order:u32) {
    let keys=unsafe{&mut *keys.cast::<Vec<(u32,Scalar,SortOrder)>>()};
    keys.push((word,operators::SCALARS[scalar as usize],match order{0=>SortOrder::Ascending,_=>SortOrder::Descending}));
}
fn record_data(env:&BTreeMap<String,Root>,expr:&Expr,d:&Definition)->Result<(usize,Access)> {
    let source=unsafe{collective_field_syntax(expr.signature_node())};
    if source.named!=0 {
        if let Some(Root::Data(binding,access))=env.get(expr.source_view().text(source.name)) {return Ok((*binding,*access));}
    }
    d.source.fail(expr.span(),"B190","集合输入／输出需要命名 dataset")
}
pub(super) fn compile_record(
    program:&mut Program,env:&BTreeMap<String,Root>,kind:RecordKind,args:&Expressions,d:&Definition,
    span:Span,facts:&mut prefix::Facts,operation_source:Option<Arc<SourceSite>>,
)->Result<()> {
    let storage=args.source_view();let arguments=args.source_slice();let mut cursor=RecordCursor{index:0,phase:0};
    let mut control=RecordFacts::default();let mut source=None;let mut prefix=None;let mut predicate=None;let mut target=None;let mut count=None;
    let mut key=None;let mut order=0;let mut keys=Vec::new();let mut output:Option<bounds::Proof>=None;
    let mut element_guarantees=Vec::new();let mut prefix_guarantees=Vec::new();let mut write_regions=BTreeMap::new();let mut reads=Vec::new();
    loop {
        let step=unsafe{record_step(kind,cursor,storage.storage(),arguments,control)};cursor=step.cursor;
        match step.need {
            RecordNeed::Source=>{
                let (input,domain)=prefix::input(program,env,&args.at(step.argument as usize),d,facts)?;
                let (binding,access)=record_data(env,&input,d)?;source=Some(binding);prefix=domain;
                control.source=binding as u64;control.source_access=access as u32;control.has_prefix=u32::from(prefix.is_some());
                control.prefix_count=prefix.map_or(0,|p|p.binding as u64);
            },
            RecordNeed::Predicate=>{
                let value=collective_field(program,env,&args.at(step.argument as usize),d,Scalar::Bool)?;
                control.predicate=value.0 as u64;control.predicate_access=value.2 as u32;predicate=Some(value);
            },
            RecordNeed::Target=>{
                let value=record_data(env,&args.at(step.argument as usize),d)?;
                control.target=value.0 as u64;control.target_access=value.1 as u32;target=Some(value.0);
            },
            RecordNeed::Views=>control.views=u32::from(program.buffers[target.unwrap()].ty.contains_views()),
            RecordNeed::Count=>{
                let value=collective_field(program,env,&args.at(step.argument as usize),d,Scalar::U32)?;
                control.count=value.0 as u64;control.count_access=value.2 as u32;count=Some(PrefixCount{binding:value.0,word:value.1});
            },
            RecordNeed::FlagInitialized=>{let (binding,word,_)=predicate.unwrap();control.initialized=u32::from(facts.initialized(program,binding,word));},
            RecordNeed::Types=>control.same_type=u32::from(program.buffers[source.unwrap()].ty==program.buffers[target.unwrap()].ty),
            RecordNeed::CountShape=>{let size=program.buffers[count.unwrap().binding].count;control.count_known=u32::from(size.is_some());control.count_size=size.unwrap_or(0);},
            RecordNeed::PredicateShape=>{
                let source_size=program.buffers[source.unwrap()].count;let predicate_size=program.buffers[predicate.unwrap().0].count;
                control.source_known=u32::from(source_size.is_some());control.source_size=source_size.unwrap_or(0);
                control.predicate_known=u32::from(predicate_size.is_some());control.predicate_size=predicate_size.unwrap_or(0);
            },
            RecordNeed::Capacity=>{
                let source_size=program.buffers[source.unwrap()].count;let target_size=program.buffers[target.unwrap()].count;
                control.source_known=u32::from(source_size.is_some());control.source_size=source_size.unwrap_or(0);
                control.target_known=u32::from(target_size.is_some());control.target_size=target_size.unwrap_or(0);
            },
            RecordNeed::Key=>{
                let value=prefix::typed_field(program,env,&storage.node(step.expression),d,facts)?;
                if step.set_source!=0 {source=Some(value.0);prefix=value.3;control.source=value.0 as u64;control.has_prefix=u32::from(prefix.is_some());control.prefix_count=prefix.map_or(0,|p|p.binding as u64);}
                control.key_binding=value.0 as u64;control.key_access=value.2 as u32;control.key_same_prefix=u32::from(value.3==prefix);control.key_numeric=u32::from(value.4.is_numeric());
                order=step.order;key=Some(value);
            },
            RecordNeed::RootRead=>control.root_read=u32::from(env.values().any(|root|matches!(root,Root::Data(binding,Access::Read|Access::InOut) if *binding==source.unwrap()))),
            RecordNeed::KeyComponents=>{
                let (_,word,_,_,ty)=key.take().unwrap();
                unsafe{record_sort_components((&ty as *const Ty).cast(),(&mut keys as *mut Vec<(u32,Scalar,SortOrder)>).cast(),word,order,ty.components.len() as u64,record_component,record_key_push)};
            },
            RecordNeed::SourceFacts=>control.source_proof=u32::from(facts.elements.contains_key(&source.unwrap())),
            RecordNeed::Elements=>output=Some(facts.elements[&source.unwrap()].element_facts(|binding|bounds::Proof::default().with_domains(Domains::from([IndexDomain::Data(binding)])))),
            RecordNeed::SameLength=>control.same_length=u32::from(program.same_length(source.unwrap(),target.unwrap())),
            RecordNeed::JoinElements=>{
                let unknown=bounds::Proof::default();
                output.as_mut().unwrap().join(facts.elements.get(&target.unwrap()).unwrap_or(&unknown),program.buffers[target.unwrap()].ty.components.len(),None);
            },
            RecordNeed::Guarantees=>element_guarantees=output.as_ref().unwrap().element_guarantees(target.unwrap(),&program.buffers[target.unwrap()].ty,|_,_|true),
            RecordNeed::Fields=>{
                let binding=target.unwrap();let ty=&program.buffers[binding].ty;
                let mut context=CollectiveWrite{program,facts,binding,regions:&mut write_regions,proof:output.as_ref()};
                unsafe{collective_write_fields((ty as *const Ty).cast(),(&mut context as *mut CollectiveWrite<'_>).cast(),0,ty.components.len() as u64,collective_component,collective_written)};
            },
            RecordNeed::CountWrite=>{
                let count=count.unwrap();let mut context=CollectiveWrite{program,facts,binding:count.binding,regions:&mut write_regions,proof:None};
                unsafe{collective_write_count((&mut context as *mut CollectiveWrite<'_>).cast(),count.word,collective_written)};
            },
            RecordNeed::GrantInput=>{
                let relation=PrefixRelation{domain_region:None,domain:target.unwrap(),count:prefix.unwrap()};facts.grant(relation);prefix_guarantees.push(relation);
            },
            RecordNeed::GrantOutput=>{
                let relation=PrefixRelation{domain_region:None,domain:target.unwrap(),count:count.unwrap()};facts.grant(relation);prefix_guarantees.push(relation);
            },
            RecordNeed::Reads=>{
                let value=unsafe{collective_reads(control.source,step.argument as u32,control.predicate,control.has_prefix,control.prefix_count)};
                reads.extend_from_slice(&[value.first as usize,value.second as usize,value.third as usize][..value.count as usize]);
            },
            RecordNeed::Publish=>{
                let source=source.unwrap();let target=target.unwrap();let mut writes=vec![target];
                let operation=match kind {
                    RecordKind::Compact=>{let (flags,flag_word,_)=predicate.unwrap();let count=count.unwrap();writes.push(count.binding);Operation::Compact(Compaction{source,flags,flag_word,target,count:count.binding,count_word:count.word})},
                    RecordKind::Sort=>Operation::StableSort(Sorting{source,keys,target}),
                };
                program.stages.push(Stage{operation_source,generated_origin:None,tiles:Vec::new(),domain:source,prefix,selection:None,operation,boundary_before:true,
                    registers:Vec::new(),locals:Default::default(),code:Vec::new(),reads:reads.clone(),writes,write_regions,prefix_guarantees,element_guarantees,cross_access:reads});
                return Ok(());
            },
            RecordNeed::Error=>{
                let (code,message)=match step.error {
                    RecordError::CompactArity=>("B169","compact 需要输入、bool 谓词字段、输出和 u32 单值计数字段"),
                    RecordError::SortArity=>("B169","stable_sort_by_key 需要按优先级排列的数值源键字段和输出 dataset"),
                    RecordError::Descending=>("B169","descending 需要一个排序键字段"),
                    RecordError::CompactViews=>("B185","compact不能复制持久view载荷；原引用图须保持"),
                    RecordError::SortViews=>("B185","排序不能复制持久view载荷；原引用图须保持"),
                    RecordError::CompactAccess=>("B170","compact 需要不可变输入／谓词，输出与计数不得与读资源或彼此别名"),
                    RecordError::KeyAccess=>("B170","排序键必须来自同一 read 输入和同一有效域"),
                    RecordError::KeyType=>("B175","排序键需要数值字段；bool、记录、数组和资源不能作为键"),
                    RecordError::SortAccess=>("B170","排序需要独立 read 输入和可写输出，输出不得改写输入域计数"),
                    RecordError::CompactType=>("B189","compact 输入／输出的记录类型必须相同"),
                    RecordError::SortType=>("B189","排序输入／输出的完整记录类型必须相同"),
                    RecordError::CountShape=>("B171","compact 计数输出必须为形状 1 的 u32 计数字段"),
                    RecordError::PredicateShape=>("B187","compact 的输入与谓词形状必须相同"),
                    RecordError::CompactCapacity=>("B188","compact 输出容量必须容纳全部输入"),
                    RecordError::SortCapacity=>("B188","排序输出容量必须容纳完整输入域"),
                };
                let span=match step.error {
                    RecordError::Descending|RecordError::KeyAccess|RecordError::KeyType=>args.at(step.argument as usize).span(),
                    RecordError::CompactViews|RecordError::CompactType=>args.at(2).span(),
                    RecordError::SortViews|RecordError::SortType=>args.last().unwrap().span(),
                    RecordError::CountShape=>args.at(3).span(),_=>span,
                };
                return d.source.fail(span,code,message);
            },
        }
    }
}
