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
pub(super) enum ValueKind { Reduce, Scan, SegmentedScan, SegmentedReduce }
#[allow(dead_code)]
#[repr(u32)]
enum NumericNeed { Source, Target, Head, Count, Types, Access, TargetShape, CountShape, Capacity, Fields, CountWrite, GrantInput, GrantOutput, Reads, Publish, Error }
#[allow(dead_code)]
#[repr(u32)]
enum NumericError { ReduceArity, ScanArity, SegmentedArity, ReduceType, ScanType, SegmentedType, HeadAccess, ReduceAccess, ScanAccess, SegmentedAccess, ReduceShape, CountShape, ScanCapacity, SegmentedCapacity }
#[allow(dead_code)]
#[repr(u32)]
enum ValueOperation { Reduce, Scan, SegmentedReduce }
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
struct NumericStep { cursor:SourceValueCursor,need:NumericNeed,error:NumericError,operation:ValueOperation,argument:u64 }
#[repr(C)]
struct CollectiveComponent { offset:u32,size:u32 }
#[repr(C)]
struct CollectiveReads { first:u64,second:u64,third:u64,count:u64 }
unsafe extern "C" {
    #[link_name="bootstrap.function_body::numeric_step"]
    fn numeric_step(kind:ValueKind,operation:CollectiveOperator,cursor:SourceValueCursor,facts:NumericFacts)->NumericStep;
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
    program:&mut Program,env:&BTreeMap<String,Root>,operator:CollectiveOperator,kind:ValueKind,
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
                    ValueOperation::Reduce=>Operation::Reduce(ReductionSpec{fields,ty,operator}),
                    ValueOperation::Scan=>Operation::ScanExclusive(Scan{values:fields,ty,operator,head_word:head.map(|(_,word,_,_)|word)}),
                    ValueOperation::SegmentedReduce=>{
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

#[repr(u32)]enum ValueKernelError {Valid,Missing,Reference}
#[repr(C)]struct ValueKernelSource {error:ValueKernelError,explicit:u32,reference:u64,name:crate::syntax::wire::Text,captures:crate::syntax::wire::Slice,types:crate::syntax::wire::Slice}
#[repr(C)]#[derive(Clone,Copy)]struct ValueKernelCursor {phase:u32,index:u64}
#[repr(C)]#[derive(Clone,Copy)]struct ValueKernelFacts {unique:u32,valid:u32,parameters:u64,captures:u64,explicit:u32,uniform:u32,views:u32,reads:u32,writes:u32}
#[repr(u32)]enum ValueKernelNeed {Resolve,Unknown,Signature,InvalidSignature,Begin,Parameter,BeginCaptures,Capture,InvalidCapture,PushCapture,EndCaptures,Explicit,Specialize,Origin,Invoke,Ordinary,Value,Same,Publish}
#[repr(C)]struct ValueKernelStep {cursor:ValueKernelCursor,need:ValueKernelNeed,index:u64}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::value_kernel_source"]fn value_kernel_source(source:crate::syntax::wire::SignatureNode)->ValueKernelSource;
 #[link_name="bootstrap.function_body::value_kernel_signature"]fn value_kernel_signature(declaration:crate::syntax::wire::SignatureNode,invocation:u32,parameters:u64,captures:u64)->u32;
 #[link_name="bootstrap.function_body::value_kernel_step"]fn value_kernel_step(cursor:ValueKernelCursor,facts:ValueKernelFacts)->ValueKernelStep;
}
async fn value_kernel(
    world: &World,
    program: &mut Program,
    env: &BTreeMap<String, Root>,
    owner: &crate::generics::Instance,
    domain_path: &Arc<[DomainSite]>,
    expr: &Expr,
    ty: &Ty,
    parameters: &[&Ty],
    functions: &mut native_function::Module,
) -> Result<ValueCode> {
    let d=&owner.definition;
    let source=unsafe {value_kernel_source(expr.signature_node())};
    let reference=expr.child::<crate::syntax::wire::ExprNode>(source.reference);
    match source.error {
        ValueKernelError::Valid=>{},
        ValueKernelError::Missing=>return d.source.fail(expr.span(),"B212","bind 需要静态值函数引用"),
        ValueKernelError::Reference=>return d.source.fail(reference.span(),"B212","集合需要静态函数引用，或 bind(静态函数引用, 统一值…)"),
    }
    let captures=expr.list::<crate::syntax::wire::ExprNode>(source.captures);
    let mut cursor=ValueKernelCursor {phase:0,index:0};
    let mut facts=ValueKernelFacts {unique:0,valid:0,parameters:parameters.len() as u64,captures:source.captures.count,explicit:source.explicit,uniform:0,views:0,reads:0,writes:0};
    let mut key=None;let mut builder=None;let mut inputs=Vec::new();let mut args=Vec::new();
    let mut explicit=None;let mut instance=None;let mut entry_origin=None;
    let mut returned=None;let mut ordinary=None;let mut value=None;
    loop {
        let step=unsafe {value_kernel_step(cursor,facts)};cursor=step.cursor;
        match step.need {
            ValueKernelNeed::Resolve=>{
                key=world.resolve(expr.source_view().text(source.name),&d.module,&d.imports,true).unique();
                facts.unique=u32::from(key.is_some());
            }
            ValueKernelNeed::Unknown=>return d.source.fail(expr.span(),"B158","未知或未导出的集合值函数"),
            ValueKernelNeed::Signature=>{
                let callee=&world.functions[key.as_ref().unwrap()];
                facts.valid=unsafe {value_kernel_signature(callee.function.signature_node(),u32::from(callee.execution==crate::execution::Requirement::Invocation),parameters.len() as u64,captures.len() as u64)};
            }
            ValueKernelNeed::InvalidSignature=>return d.source.fail(expr.span(),"B212","集合需要 compute 值函数：投影接收一个源值，单位元无元素参数，合并器接收两个结果值；bind 参数追加在元素参数之后"),
            ValueKernelNeed::Begin=>{
                let mut b=unsafe {world.call(Builder::new(world,owner,program,0,Some(&owner.name))).await}?;
                b.source_context=Arc::new(SourceContext {domain_path:domain_path.clone(),..(*b.source_context).clone()});
                b.functions=std::mem::take(functions);b.roots(env);builder=Some(b);
            }
            ValueKernelNeed::Parameter=>{
                let b=builder.as_mut().unwrap();let ty=parameters[step.index as usize];
                let regs:Vec<_>=ty.components.iter().map(|(s,_)|b.reg(*s)).collect();inputs.extend(&regs);
                args.push(cooperative::Argument::Value(Value {proof:bounds::Proof::default(),literal:false,ty:ty.clone(),regs,uniform:false,constant:None}));
            }
            ValueKernelNeed::BeginCaptures=>builder.as_mut().unwrap().in_function=true,
            ValueKernelNeed::Capture=>{
                let capture=captures.at(step.index as usize);let b=builder.as_mut().unwrap();
                let result=unsafe {world.call(b.expr(&capture)).await}?;let captured=b.value_result(result,capture.span())?;
                facts.uniform=u32::from(captured.uniform);facts.views=u32::from(captured.ty.contains_views());
                facts.reads=u32::from(!b.reads.is_empty());facts.writes=u32::from(!b.writes.is_empty());value=Some(captured);
            }
            ValueKernelNeed::InvalidCapture=>return d.source.fail(captures.at(step.index as usize).span(),"B213","bind 只能捕获执行统一的普通值，不能捕获资源或读取数据集元素"),
            ValueKernelNeed::PushCapture=>args.push(cooperative::Argument::Value(value.take().unwrap())),
            ValueKernelNeed::EndCaptures=>builder.as_mut().unwrap().in_function=false,
            ValueKernelNeed::Explicit=>{
                let arguments=expr.list(source.types);
                explicit=Some(unsafe {world.call(owner.bindings.arguments(world,&arguments,(&d.source,&d.module,&d.imports),reference.span())).await}?);
            }
            ValueKernelNeed::Specialize=>{
                instance=Some(unsafe {world.call(builder.as_ref().unwrap().specialized_arguments(key.take().unwrap(),&args,explicit.as_deref(),reference.span())).await}?);explicit=None;
            }
            ValueKernelNeed::Origin=>{
                let b=builder.as_ref().unwrap();let instance=instance.as_ref().unwrap();
                let source_site=|span|b.source_definition.as_ref().map(|origin|Arc::new(SourceSite {origin:origin.clone(),context:b.source_context.clone(),span,syntax:SourceSyntax::Expression}));
                entry_origin=Some(Arc::new(ValueCodeEntry {definition:instance.definition.source_origin().map(Arc::new),instance:Arc::from(instance.name.as_str()),reference:source_site(reference.span()),capture_parameter_start:parameters.len(),captures:captures.iter().map(|capture|source_site(capture.span())).collect()}));
            }
            ValueKernelNeed::Invoke=>returned=unsafe {world.call(builder.as_mut().unwrap().invoke_returned(instance.take().unwrap(),std::mem::take(&mut args),reference.span(),None,None)).await}?,
            ValueKernelNeed::Ordinary=>ordinary=builder.as_ref().unwrap().ordinary_return(returned.take(),expr.span())?,
            ValueKernelNeed::Value=>value=Some(builder.as_ref().unwrap().value_result(ordinary.take(),expr.span())?),
            ValueKernelNeed::Same=>builder.as_ref().unwrap().require_same(ty,&value.as_ref().unwrap().ty,expr.span())?,
            ValueKernelNeed::Publish=>{
                let mut b=builder.take().unwrap();*functions=std::mem::take(&mut b.functions);
                return Ok(ValueCode {entry_origin,generated_origin:None,registers:b.registers,locals:b.locals,code:b.code,inputs,outputs:value.take().unwrap().regs});
            }
        }
    }
}

unsafe extern "C" {
    #[link_name="bootstrap.operator_types::source_custom_reduce"]
    fn source_custom_reduce(bytes:*const u8,length:usize)->u32;
}
impl ValueKind {
    pub(super) fn from_custom_source(name:&str)->Option<Self> {
        match unsafe{source_custom_reduce(name.as_ptr(),name.len())}{
            1=>Some(Self::Reduce),2=>Some(Self::Scan),3=>Some(Self::SegmentedScan),4=>Some(Self::SegmentedReduce),_=>None,
        }
    }
}
#[allow(dead_code)]
#[repr(u32)]
enum CustomNeed {Source,Head,Target,Types,Access,Count,CountShape,TargetShape,Capacity,Project,Identity,Combine,Fields,CountWrite,GrantInput,GrantOutput,Reads,Publish,Error}
#[allow(dead_code)]
#[repr(u32)]
enum CustomError {Arity,Head,Type,Access,CountAccess,CountShape,Capacity,TargetShape}
#[derive(Clone,Copy,Default)]
#[repr(C)]
struct CustomFacts {
    arguments:u64,source:u64,target:u64,heads:u64,count:u64,prefix_count:u64,head_count:u64,
    has_prefix:u32,has_head_prefix:u32,prefix_word:u32,head_count_word:u32,
    source_access:u32,target_access:u32,head_access:u32,count_access:u32,
    same:u32,source_views:u32,target_views:u32,source_overlap:u32,head_overlap:u32,
    source_known:u32,source_size:u32,target_known:u32,target_size:u32,count_known:u32,count_size:u32,
}
#[repr(C)]
struct CustomStep {cursor:SourceValueCursor,need:CustomNeed,error:CustomError,operation:ValueOperation,argument:u64}
unsafe extern "C" {
    #[link_name="bootstrap.function_body::custom_collection_step"]
    fn custom_collection_step(kind:ValueKind,map:u32,cursor:SourceValueCursor,facts:CustomFacts)->CustomStep;
}
pub(super) async fn compile_custom(
    world:&World,program:&mut Program,env:&BTreeMap<String,Root>,args:&Expressions,
    owner:&crate::generics::Instance,span:Span,facts:&mut prefix::Facts,map:bool,kind:ValueKind,
    domain_path:Arc<[DomainSite]>,native_functions:&mut native_function::Module,
    operation_source:Option<Arc<SourceSite>>,
)->Result<()> {
    let d=&owner.definition;let mut cursor=SourceValueCursor{phase:0};let mut control=CustomFacts{arguments:args.len() as u64,..Default::default()};
    let mut source=None;let mut target=None;let mut head=None;let mut count=None;
    let mut project=None;let mut identity=None;let mut combine=None;
    let mut write_regions=BTreeMap::new();let mut prefix_guarantees=Vec::new();let mut reads=Vec::new();
    loop {
        let step=unsafe{custom_collection_step(kind,u32::from(map),cursor,control)};cursor=step.cursor;
        match step.need {
            CustomNeed::Source=>{
                let value=prefix::typed_field(program,env,&args.at(step.argument as usize),d,facts)?;
                control.source=value.0 as u64;control.source_access=value.2 as u32;
                if let Some(prefix)=value.3{control.has_prefix=1;control.prefix_count=prefix.binding as u64;control.prefix_word=prefix.word;}
                source=Some(value);
            },
            CustomNeed::Head=>{
                let value=prefix::field(program,env,&args.at(step.argument as usize),d,facts,Scalar::Bool)?;
                control.heads=value.0 as u64;control.head_access=value.2 as u32;
                if let Some(prefix)=value.3{control.has_head_prefix=1;control.head_count=prefix.binding as u64;control.head_count_word=prefix.word;}
                head=Some(value);
            },
            CustomNeed::Target=>{
                let value=collective_value_field(program,env,&args.at(step.argument as usize),d)?;
                control.target=value.0 as u64;control.target_access=value.2 as u32;target=Some(value);
            },
            CustomNeed::Types=>{
                let source_ty=&source.as_ref().unwrap().4;let ty=&target.as_ref().unwrap().3;
                control.same=u32::from(source_ty==ty);control.source_views=u32::from(source_ty.contains_views());control.target_views=u32::from(ty.contains_views());
            },
            CustomNeed::Access=>{
                let (source,word,_,_,source_ty)=source.as_ref().unwrap();let (target,target_word,_,ty)=target.as_ref().unwrap();
                control.source_overlap=u32::from(prefix::fields_overlap(*source,*word,source_ty,*target,*target_word,ty));
                if let Some((heads,word,_,_))=head{control.head_overlap=u32::from(prefix::fields_overlap(heads,word,&Ty::scalar(Scalar::Bool),*target,*target_word,ty));}
            },
            CustomNeed::Count=>{
                let (binding,word,access)=collective_field(program,env,&args.at(step.argument as usize),d,Scalar::U32)?;
                control.count=binding as u64;control.count_access=access as u32;count=Some(PrefixCount{binding,word});
            },
            CustomNeed::CountShape=>{let size=program.buffers[count.unwrap().binding].count;control.count_known=u32::from(size.is_some());control.count_size=size.unwrap_or(0);},
            CustomNeed::TargetShape=>{let size=program.buffers[target.as_ref().unwrap().0].count;control.target_known=u32::from(size.is_some());control.target_size=size.unwrap_or(0);},
            CustomNeed::Capacity=>{
                let source_size=program.buffers[source.as_ref().unwrap().0].count;let target_size=program.buffers[target.as_ref().unwrap().0].count;
                control.source_known=u32::from(source_size.is_some());control.source_size=source_size.unwrap_or(0);
                control.target_known=u32::from(target_size.is_some());control.target_size=target_size.unwrap_or(0);
            },
            CustomNeed::Project=>project=Some(unsafe{world.call(value_kernel(world,program,env,owner,&domain_path,&args.at(step.argument as usize),&target.as_ref().unwrap().3,&[&source.as_ref().unwrap().4],native_functions)).await}?),
            CustomNeed::Identity=>identity=Some(unsafe{world.call(value_kernel(world,program,env,owner,&domain_path,&args.at(step.argument as usize),&target.as_ref().unwrap().3,&[],native_functions)).await}?),
            CustomNeed::Combine=>combine=Some(unsafe{world.call(value_kernel(world,program,env,owner,&domain_path,&args.at(step.argument as usize),&target.as_ref().unwrap().3,&[&target.as_ref().unwrap().3,&target.as_ref().unwrap().3],native_functions)).await}?),
            CustomNeed::Fields=>{
                let (binding,base,_,ty)=target.as_ref().unwrap();let mut context=CollectiveWrite{program,facts,binding:*binding,regions:&mut write_regions,proof:None};
                unsafe{collective_write_fields((ty as *const Ty).cast(),(&mut context as *mut CollectiveWrite<'_>).cast(),*base,ty.components.len() as u64,collective_component,collective_written)};
            },
            CustomNeed::CountWrite=>{
                let count=count.unwrap();let mut context=CollectiveWrite{program,facts,binding:count.binding,regions:&mut write_regions,proof:None};
                unsafe{collective_write_count((&mut context as *mut CollectiveWrite<'_>).cast(),count.word,collective_written)};
            },
            CustomNeed::GrantInput=>{
                let relation=PrefixRelation{domain_region:None,domain:target.as_ref().unwrap().0,count:source.as_ref().unwrap().3.unwrap()};facts.grant(relation);prefix_guarantees.push(relation);
            },
            CustomNeed::GrantOutput=>{
                let relation=PrefixRelation{domain_region:None,domain:target.as_ref().unwrap().0,count:count.unwrap()};facts.grant(relation);prefix_guarantees.push(relation);
            },
            CustomNeed::Reads=>{
                let value=unsafe{collective_reads(control.source,control.has_prefix,control.prefix_count,0,0)};
                reads.extend_from_slice(&[value.first as usize,value.second as usize,value.third as usize][..value.count as usize]);
            },
            CustomNeed::Publish=>{
                let (source,source_word,_,prefix,source_ty)=source.unwrap();let (target,target_word,_,ty)=target.unwrap();let mut writes=vec![target];
                let values=CustomReduction{fields:Reduction{source,source_word,target,target_word},source_ty,ty,project,identity:identity.unwrap(),combine:combine.unwrap()};
                let operation=match step.operation{
                    ValueOperation::Reduce=>Operation::CustomReduce(values),
                    ValueOperation::Scan=>Operation::CustomScan(CustomScan{values,head_word:head.map(|(_,word,_,_)|word)}),
                    ValueOperation::SegmentedReduce=>{let count=count.unwrap();writes.push(count.binding);Operation::CustomSegmentedReduce(CustomSegmentedReduction{values,head_word:head.unwrap().1,count})},
                };
                program.stages.push(Stage{operation_source,generated_origin:None,domain:source,prefix,selection:None,operation,boundary_before:true,
                    registers:Vec::new(),locals:Default::default(),code:Vec::new(),reads:reads.clone(),writes,write_regions,prefix_guarantees,element_guarantees:Vec::new(),cross_access:reads,tiles:Vec::new()});
                return Ok(());
            },
            CustomNeed::Error=>{
                let (code,message)=match step.error{
                    CustomError::Arity=>("B169","自定义集合需要源字段、目标字段、单位元和合并函数；分段形式在源值后提供同源 bool 段头，分段归约在目标后提供输出计数，映射形式在单位元前提供投影函数"),
                    CustomError::Head=>("B170","分段起点必须来自同一可读输入记录，和源值共享元素域"),
                    CustomError::Type=>("B175","无投影的集合需要相同普通值类型，聚合值不能持有资源引用"),
                    CustomError::Access=>("B170","集合需要可读输入和可写输出，目标不能覆盖源值、段头或输入域计数"),
                    CustomError::CountAccess=>("B170","分段归约需要独立的可写目标与输出计数，不能改写输入域计数"),
                    CustomError::CountShape=>("B171","分段归约计数必须为形状 1 的 u32 计数字段"),
                    CustomError::Capacity=>("B188","扫描／分段归约输出容量必须容纳全部源元素"),
                    CustomError::TargetShape=>("B171","归约输出形状必须为 1"),
                };return d.source.fail(span,code,message);
            },
        }
    }
}
