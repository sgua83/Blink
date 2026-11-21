//! 应用入口是一次普通 CPU 值函数调用，不伪造 dataset 或 parallel 域。
use super::cooperative::Argument;
use super::*;
mod drops;
#[derive(Default)]
pub(super) struct Context {
    pub frame: Option<Vec<Reg>>,
    pub range: bool,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Ownership {
    Owned,
    Borrowed,
    Moved,
    Conditional(Box<owned_presence::Presence>),
}
// Current element facts belong to this Data, including conditional cleanup
// premises supplied by a real input declaration. They are not type invariants.
#[derive(Clone,Default)]
pub(super) enum ElementFacts {
    #[default]
    Unknown,
    Input(bounds::Condition),
    Known(Box<returns::Returned>),
    Cleanup(bounds::Condition),
    Common(Box<ElementFacts>,Box<ElementFacts>),
    Selected(Box<bounds::Condition>,Box<ElementFacts>,Box<ElementFacts>),
}
impl ElementFacts {
    pub(super) fn known(&self)->Option<&returns::Returned>{match self{Self::Known(value)=>Some(value),Self::Unknown|Self::Input(_)|Self::Cleanup(_)|Self::Common(..)|Self::Selected(..)=>None}}
    pub(super) fn known_mut(&mut self)->Option<&mut returns::Returned>{match self{Self::Known(value)=>Some(value),Self::Unknown|Self::Input(_)|Self::Cleanup(_)|Self::Common(..)|Self::Selected(..)=>None}}
    pub(super) fn common(self,other:Self)->Self {match(self,other){
        (Self::Unknown,_)|(_,Self::Unknown)=>Self::Unknown,
        (Self::Input(a),Self::Input(b))=>Self::Input(a.logical(b,false)),
        (Self::Known(mut a),Self::Known(b))=>{a.join(*b,true,None);Self::Known(a)},
        (Self::Cleanup(a),Self::Cleanup(b))=>Self::Cleanup(a.logical(b,false)),
        (a,b)=>Self::Common(Box::new(a),Box::new(b)),
    }}
    pub(super) fn selected(self, other: Self, when: &bounds::Condition) -> Self {
        self.join_selected(other, when, None)
    }
    pub(super) fn retain_side(
        mut self, mut other: Self, take_right: bool, presence: Option<&owned_presence::Presence>,
    ) -> Self {
        if let Some(presence) = presence
            && let (Some(left), Some(right)) = (self.known_mut(), other.known_mut())
        {
            left.merge_retained_presence(right, presence, take_right);
        }
        if take_right { other } else { self }
    }
    pub(super) fn join_selected(
        self, other: Self, when: &bounds::Condition, presence: Option<&owned_presence::Presence>,
    ) -> Self {
        if let Some(truth) = when.boolean_constant() {
            return self.retain_side(other, !truth, presence);
        }
        match(self,other){
            (Self::Unknown,Self::Unknown)=>Self::Unknown,
            (Self::Input(a),Self::Input(b))=>Self::Input(bounds::Condition::branch(when.clone(),a,b)),
            (Self::Known(mut a),Self::Known(b))=>{a.join_with_presence(*b,true,Some(when),presence);Self::Known(a)},
            (Self::Cleanup(a),Self::Cleanup(b))=>Self::Cleanup(bounds::Condition::branch(when.clone(),a,b)),
            (Self::Cleanup(a),Self::Unknown)=>Self::Cleanup(when.clone().logical(a,false)),
            (Self::Unknown,Self::Cleanup(b))=>Self::Cleanup(when.clone().negate().logical(b,false)),
            (a,b)=>Self::Selected(Box::new(when.clone()),Box::new(a),Box::new(b)),
        }
    }
    pub(super) fn map(self,values:&mut impl FnMut(Box<returns::Returned>)->Box<returns::Returned>,conditions:&mut impl FnMut(bounds::Condition)->bounds::Condition)->Self {match self{
        Self::Unknown=>Self::Unknown,
        Self::Input(when)=>Self::Input(conditions(when)),
        Self::Known(value)=>Self::Known(values(value)),
        Self::Cleanup(when)=>Self::Cleanup(conditions(when)),
        Self::Common(a,b)=>a.map(values,conditions).common(b.map(values,conditions)),
        Self::Selected(when,a,b)=>{let when=conditions(*when);a.map(values,conditions).selected(b.map(values,conditions),&when)},
    }}
}

// CPU 应用的私有资源描述由原绑定持有，不铺进每层设备表达式的按值载荷。
pub(super) type Data = Box<DataPayload>;
#[derive(Clone)]
pub(super) struct DataPayload {
    // Frame-owned storage has a u32 handle; an Invocation read/atomic borrow has its actual u64 storage address.
    pub root: Value,
    pub place: Option<Place>,
    pub initialization: Option<Position>,
    pub ty: Ty,
    pub length: Value,
    pub ownership: Ownership,
    pub access: Access,
    pub count: Option<u32>,
    pub residency: residency::Residency,
    pub layout: DataLayout,
    // Element facts do not own storage, initialization, resource access or lifetime.
    pub elements: ElementFacts,
    pub prefixes: prefix::Fields,
}
impl DataPayload {
    pub(super) fn owns_write_elements(&self) -> bool {
        self.ownership == Ownership::Borrowed && self.access == Access::Write
            && self.ty.owns_resources()
    }

    pub(super) fn returns_loan_on_error(&self) -> bool {
        self.ownership == Ownership::Borrowed && self.access == Access::InOut
            && matches!(self.layout, DataLayout::Parameter { .. })
            && matches!(self.elements, ElementFacts::Known(_) | ElementFacts::Common(..) | ElementFacts::Selected(..))
            && self.ty.owns_resources()
    }

    pub(super) async fn loop_compatible(&self,other:&Self,builder:&mut Builder<'_>,span:Span,path:&bounds::Clause,inputs:&BTreeMap<ValueIdentity,bounds::Proof>)->Result<bool> {
        if !self.ownership.loop_compatible(&other.ownership,inputs,path){return Ok(false);}
        let path=other.ownership.presence().condition().assume(path,true);
        if !path.possible(){return Ok(true);}
        if !builder.residency.same(&self.residency,&other.residency,builder.scope.source(),span,&path){return Ok(false);}
        // Only an existing element model introduces child resources to compare.
        // An unknown counterpart is projected by the same finite model owner.
        if matches!((&self.elements,&other.elements),
            (ElementFacts::Unknown|ElementFacts::Input(_)|ElementFacts::Cleanup(_),ElementFacts::Unknown|ElementFacts::Input(_)|ElementFacts::Cleanup(_))){return Ok(true);}
        let before=unsafe { builder.world.call(builder.element_model(&self.ty,&self.elements,(&self.residency,&[]),span)).await }?;
        let after=unsafe { builder.world.call(builder.element_model(&other.ty,&other.elements,(&other.residency,&[]),span)).await }?;
        match(before.as_ref(),after.as_ref()){
            (returns::Returned::OwnedValue(a),returns::Returned::OwnedValue(b))=>unsafe { builder.world.call(a.loop_compatible(b,builder,span,&path,inputs)).await },
            (returns::Returned::Dataset(a),returns::Returned::Dataset(b))=>unsafe { builder.world.call(a.loop_compatible(b,builder,span,&path,inputs)).await },
            _=>Ok(true),
        }
    }
    pub(super) fn permits(&self, access: Access) -> bool {
        match self.access {
            Access::Atomic => access == Access::Atomic,
            Access::Read => access == Access::Read,
            Access::Write | Access::InOut => access != Access::Value,
            Access::Value => unreachable!(),
        }
    }
}
impl Binding {
    pub(super) fn application_resources(&self)->Vec<&Data> {
        match self {Self::ApplicationData(data)=>vec![data],Self::DataReference(data)=>vec![&data.data],Self::OwnedValue(value)=>value.resources(),_=>Vec::new()}
    }
}
// 更新描述只属于原共同元素模型，不成为运行期状态。
pub(super) enum ElementUpdate<'a> {
    Unknown,
    Bytes {effect:&'a address::MemoryEffect,index:Option<(bounds::Operand,(u32,u32))>},
    Element { proof: &'a bounds::Proof, index: Option<(bounds::Operand,(u32,u32))>, when: Option<&'a bounds::Condition> },
    WholeFacts(&'a bounds::Proof),
}
// 动态数组借用携带实际可能的资源叶；这些来源只用于编译期别名和存活期。
#[derive(Clone)]
pub(super) struct DataReference {
    pub data: Data,
    pub sources: Vec<ValueIdentity>,
    pub projection: Vec<usize>,
}
enum DomainCallArgument {
    Dataset(usize),
    Uniform(Value),
    Texture(CallArgument),
}

#[derive(Clone)]
pub(super) enum DataLayout {
    Declared,
    Parameter { parameter:usize, stride:Value, offset:Value },
}
#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) enum DataStride {
    Declared(u32),
    Parameter { stride:Reg, offset:Reg },
}

crate::module::codec::variants!(DataStride{0=>Declared(stride),1=>Parameter{stride,offset}});

impl std::ops::Deref for DataReference {
    type Target = Data;
    fn deref(&self) -> &Data {
        &self.data
    }
}
impl DataReference {
    pub(super) fn direct(data: Data) -> Self {
        let sources = vec![data.root.identity()];
        Self { data, sources, projection:Vec::new() }
    }
    pub(super) fn element(&self, index: &Value, range: (u32, u32)) -> Place {
        let mut place = Builder::application_element(&self.data, index, range);
        let Place::ApplicationElement { sources, .. } = &mut place else {
            unreachable!()
        };
        *sources = self.sources.clone();
        let (offset,ty)=self.element_layout();
        let components=ty.components.iter().map(|(scalar,at)|self.data.ty.components.iter().position(|item|item==&(*scalar,offset+at)).unwrap()).collect::<Vec<_>>();
        place.project(ty.clone(),&components)
    }
    pub(super) fn element_layout(&self)->(u32,&Ty){
        let mut ty=&self.data.ty;let mut offset=0;
        for field in &self.projection{let(at,next)=ty.aggregate_part(*field);offset+=at;ty=next;}
        (offset,ty)
    }
    pub(super) fn words(&self)->Vec<u32>{
        let(offset,ty)=self.element_layout();ty.components.iter().map(|(_,word)|offset+word).collect()
    }
}

pub(super) fn dataset_origin(data:&Data,field:u32,ty:&Ty,initialized:bool)->address::Origin {
    address::Origin {
        root:address::Root::Resource{borrow:view::resource_borrow(data),index:None},base:address::Layout::of(&data.ty),layout:address::Layout::of(ty),
        words:ty.components.iter().map(|(_,word)|field+word).collect(),positions:Arc::from([]),
        writable:data.access.writes(),initialized:vec![initialized;ty.components.len()].into(),
    }
}

impl effects::StorageWrite {
    pub(super) fn includes_root(&self,builder:&Builder<'_>,root:Reg)->bool {
        match self {
            Self::Frame{root:written,..}=>*written==root,
            Self::Descendants{root:ancestor}=>{
                let loads=ApplicationLoads(builder);
                !loads.same(root,*ancestor)&&!loads.frame_descendant(*ancestor,root)
            },
        }
    }
}
// Numeric snapshots query the original emitted operations, including the outer
// IR owned by active loop contexts. Loop write inputs describe future backedges;
// no loaded-value or memory-version state is retained beside IR and Proof.
struct ApplicationLoads<'a,'w>(&'a Builder<'w>);
impl ApplicationLoads<'_, '_> {
    fn code(&self)->impl DoubleEndedIterator<Item=&Inst> {
        self.0.loop_contexts.iter().flat_map(|context|context.outer.iter()).chain(self.0.code.iter())
    }
    fn len(&self)->usize {self.0.loop_contexts.iter().map(|context|context.outer.len()).sum::<usize>()+self.0.code.len()}
    fn storage_writes(&self,writes:&effects::LoopWrites,root:Option<Reg>,range:Option<std::ops::Range<u128>>)->bool {
        writes.unknown_storage||writes.storage.iter().any(|written|match written {
            effects::StorageWrite::Descendants{..}=>root.is_none_or(|root|written.includes_root(self.0,root)),
            effects::StorageWrite::Frame{root:written,range:changed}=>match(root,range.as_ref(),changed.as_ref()) {
                (Some(root),_,_)if self.disjoint_roots(root,*written)||self.frame_descendant(root,*written)||self.frame_descendant(*written,root)=>false,
                (Some(root),Some(range),Some(changed))if self.same(root,*written)=>Self::byte_overlap(range,changed),
                _=>true,
            },
        })
    }
    fn loop_writes(&self,a:usize,b:usize,read:&Inst)->bool {
        let mut boundary=0;
        self.0.loop_contexts.iter().any(|context|{
            boundary+=context.outer.len();
            a.min(b)<boundary && a.max(b)>=boundary && match &read.kind {
                InstKind::LoadLocal(_,slot)=>context.writes.external||context.writes.locals.contains(slot),
                InstKind::Load{..}=>context.writes.external||context.writes.unknown_storage,
                InstKind::ViewLoad{dst,address,index,stride,word}=>{
                    let range=self.constant(*index).zip(self.stride(*stride)).map(|(index,stride)|{
                        let start=u128::from(index)*u128::from(stride)+u128::from(*word);
                        start..start+u128::from(self.0.registers[*dst].size())
                    });
                    self.storage_writes(&context.writes,self.frame_root(*address),range)
                },
                _=>false,
            }
        })
    }
    fn definition(&self,reg:Reg)->Option<(usize,&Inst)> {
        fn nested(inst:&Inst,reg:Reg)->Option<&Inst>{
            if inst.results().any(|result|result==reg){return Some(inst);}
            match &inst.kind{
                InstKind::If{yes,no,..}=>yes.iter().chain(no).find_map(|inst|nested(inst,reg)),
                InstKind::Loop{condition,body,..}=>condition.iter().chain(body).find_map(|inst|nested(inst,reg)),
                InstKind::Scope(body)=>body.iter().find_map(|inst|nested(inst,reg)),
                _=>None,
            }
        }
        let mut at=self.len();
        self.code().rev().find_map(|inst|{at-=1;nested(inst,reg).map(|inst|(at,inst))})
    }
    fn pointer_call(call:&Call)->Option<Reg> {
        let CallTarget::External(name)=&call.target else{return None;};
        if name.library!=crate::application::RUNTIME_LIBRARY{return None;}
        match name.symbol.as_str() {
            "blink_address" | "blink_storage_address" if call.error.is_none()=>{},
            _=>return None,
        }
        match call.arguments.get(1) {Some(CallArgument::Value{values,..})=>values.first().copied(),_=>None}
    }
    fn local_source(&self,at:usize,read:&Inst)->Option<Reg> {
        let InstKind::LoadLocal(_,slot)=&read.kind else{return None;};
        if self.writes_local(self.code().nth(at)?,*slot){return None;}
        let(distance,write)=self.code().rev().skip(self.len()-at).enumerate().find(|(_,inst)|self.writes_local(inst,*slot))?;
        if self.loop_writes(at-distance-1,at,read){return None;}
        match &write.kind {InstKind::StoreLocal(_,value)=>Some(*value),_=>None}
    }
    fn constant(&self,reg:Reg)->Option<Bits> {
        let(at,inst)=self.definition(reg)?;
        match &inst.kind {
            InstKind::Constant(_,_,bits)=>Some(*bits),
            InstKind::Cast{src,..}=>constant::cast(self.0.registers[*src],self.0.registers[reg],self.constant(*src)?),
            InstKind::LoadLocal(..)=>self.local_source(at,inst).and_then(|value|self.constant(value)),
            _=>None,
        }
    }
    fn same(&self,a:Reg,b:Reg)->bool {
        if a==b{return true;}
        if self.0.registers[a]!=self.0.registers[b]{return false;}
        let a_definition=self.definition(a);let b_definition=self.definition(b);
        if let Some((at,inst))=a_definition && let Some(value)=self.local_source(at,inst){return self.same(value,b);}
        if let Some((at,inst))=b_definition && let Some(value)=self.local_source(at,inst){return self.same(a,value);}
        let(Some((ai,a_inst)),Some((bi,b_inst)))=(a_definition,b_definition)else{return false;};
        match (&a_inst.kind,&b_inst.kind) {
            (InstKind::Constant(_,sa,a),InstKind::Constant(_,sb,b))=>sa==sb&&a==b,
            (InstKind::Cast{src:a,..},InstKind::Cast{src:b,..})=>self.same(*a,*b),
            (InstKind::Unary{op:a_op,a,..},InstKind::Unary{op:b_op,a:b,..})=>a_op==b_op&&self.same(*a,*b),
            (InstKind::Binary{op:a_op,a:aa,b:ab,..},InstKind::Binary{op:b_op,a:ba,b:bb,..})=>a_op==b_op&&self.same(*aa,*ba)&&self.same(*ab,*bb),
            (InstKind::Math{op:a_op,args:a,..},InstKind::Math{op:b_op,args:b,..})=>a_op==b_op&&a.len()==b.len()&&a.iter().zip(b).all(|(a,b)|self.same(*a,*b)),
            (InstKind::LoadLocal(_,a_slot),InstKind::LoadLocal(_,b_slot))=>{
                let direct=self.code().nth(ai).unwrap().results().any(|reg|reg==a)&&self.code().nth(bi).unwrap().results().any(|reg|reg==b);
                let range=if direct{ai.min(bi)+1..ai.max(bi)}else{ai.min(bi)..ai.max(bi)+1};
                a_slot==b_slot&&!self.loop_writes(ai,bi,a_inst)
                    &&!self.code().skip(range.start).take(range.len()).any(|inst|self.writes_local(inst,*a_slot))
            },
            (InstKind::Call(a_call),InstKind::Call(b_call))=>{
                let(Some(a_root),Some(b_root))=(Self::pointer_call(a_call),Self::pointer_call(b_call))else{return false;};
                a_call.values.iter().position(|r|*r==a)==b_call.values.iter().position(|r|*r==b)
                    &&self.same(a_root,b_root)
                    &&match(a_call.arguments.first(),b_call.arguments.first()) {
                        (Some(CallArgument::Value{values:a,..}),Some(CallArgument::Value{values:b,..}))=>a.len()==b.len()&&a.iter().zip(b).all(|(a,b)|self.same(*a,*b)),_=>false,
                    }
            },
            (InstKind::Load{..},InstKind::Load{..})|(InstKind::ViewLoad{..},InstKind::ViewLoad{..})=>{
                let direct=self.code().nth(ai).unwrap().results().any(|reg|reg==a)&&self.code().nth(bi).unwrap().results().any(|reg|reg==b);
                let range=if direct{ai.min(bi)+1..ai.max(bi)}else{ai.min(bi)..ai.max(bi)+1};
                self.same_load(a_inst,b_inst)&&!self.code().skip(range.start).take(range.len()).any(|inst|self.writes(inst,a_inst))
            },
            _=>false,
        }
    }
    fn stride(&self,value:ElementStride)->Option<u64>{match value{ElementStride::Constant(bytes)=>Some(bytes),ElementStride::Value(parts)=>Some(self.constant(parts[0])?|(self.constant(parts[1])?<<32))}}
    fn same_stride(&self,a:ElementStride,b:ElementStride)->bool{
        match (a,b){(ElementStride::Value(a),ElementStride::Value(b))=>self.same_address(a,b),_=>matches!((self.stride(a),self.stride(b)),(Some(a),Some(b))if a==b)}
    }
    fn same_address(&self,a:[Reg;2],b:[Reg;2])->bool {a.into_iter().zip(b).all(|(a,b)|self.same(a,b))}
    fn same_load(&self,a_inst:&Inst,b_inst:&Inst)->bool {
        match (&a_inst.kind,&b_inst.kind) {
            (InstKind::Load{dst:a_dst,binding:a,index:ai,word:aw},InstKind::Load{dst:b_dst,binding:b,index:bi,word:bw})=>
                self.0.registers[*a_dst]==self.0.registers[*b_dst]&&a==b&&aw==bw
                    &&!self.loop_writes(self.definition(*a_dst).unwrap().0,self.definition(*b_dst).unwrap().0,a_inst)&&self.same(*ai,*bi),
            (InstKind::ViewLoad{dst:a_dst,address:a,index:ai,stride:as_,word:aw},InstKind::ViewLoad{dst:b_dst,address:b,index:bi,stride:bs,word:bw})=>
                self.0.registers[*a_dst]==self.0.registers[*b_dst]&&self.same_stride(*as_,*bs)&&aw==bw
                    &&!self.loop_writes(self.definition(*a_dst).unwrap().0,self.definition(*b_dst).unwrap().0,a_inst)
                    &&self.same_address(*a,*b)&&self.same(*ai,*bi),
            _=>false,
        }
    }
    fn frame_root(&self,address:[Reg;2])->Option<Reg> {
        let(a,first)=self.definition(address[0])?;let(b,_)=self.definition(address[1])?;
        if a!=b{return None;}
        let InstKind::Call(call)=&first.kind else{return None;};Self::pointer_call(call)
    }
    fn allocation(call:&Call)->bool {
        matches!(&call.target,CallTarget::External(name)if name.library==crate::application::RUNTIME_LIBRARY&&name.symbol.starts_with("blink_allocate_"))
    }
    fn allocated(&self,root:Reg)->bool {
        matches!(self.definition(root),Some((_,Inst{kind:InstKind::Call(call),..}))if Self::allocation(call))
    }
    fn disjoint_roots(&self,a:Reg,b:Reg)->bool {a!=b&&self.allocated(a)&&self.allocated(b)}
    fn frame_descendant(&self,child:Reg,parent:Reg)->bool {
        let Some((_,Inst{kind:InstKind::ViewLoad{address,..},..}))=self.definition(child)else{return false;};
        let Some(container)=self.frame_root(*address)else{return false;};
        self.same(container,parent)||self.frame_descendant(container,parent)
    }
    // Resolve only metadata already produced by the original IR. No model register becomes storage.
    async fn resource_storage(&self,input:&Data,resource:&view::Borrow)->Result<Option<Vec<(effects::StorageWrite,Ty,DataLayout)>>> {
        let resource=resource.restricted(&self.0.path);
        if view::resource_borrow(input).restricted(&self.0.path)==resource {
            return Ok(Some(vec![(effects::StorageWrite::Frame{root:input.root.regs[0],range:None},input.ty.clone(),input.layout.clone())]));
        }
        if !resource.inputs.is_empty()||!resource.calls.is_empty(){return Ok(None);}
        let mut storage=Vec::new();
        for(source,when)in resource.sources.entries().filter(|(_,when)|when.possible()) {
            let view::Source::Element{parent,fields,index}=source else{return Ok(None);};
            let Some(parents)=unsafe { self.0.world.call(self.resource_storage(input,parent)).await }?else{return Ok(None);};
            for(location,ty,layout)in parents {
                let(mut word,mut field)=(0,&ty);
                for at in fields{let(offset,next)=field.aggregate_part(*at);word+=offset;field=next;}
                let Some(reference)=field.dataset.as_deref()else{return Ok(None);};
                let element=unsafe { self.0.world.call(self.0.world.ty(reference,&self.0.scope.source(),self.0.scope.span(),&self.0.scope.module(),&self.0.scope.imports())).await }?;
                let root=match location {
                    effects::StorageWrite::Descendants{root}=>{storage.push((effects::StorageWrite::Descendants{root},element,DataLayout::Declared));continue;},
                    effects::StorageWrite::Frame{root,..}=>root,
                };
                let Some(index)=index else{storage.push((effects::StorageWrite::Descendants{root},element,DataLayout::Declared));continue;};
                let index_equal=|at|{
                    let actual=match self.constant(at){Some(value)=>bounds::Operand::Constant(Scalar::U32,value),None=>bounds::Operand::Value(ValueIdentity::Register(at))};
                    let actual=self.0.element_index_operand(actual,(&self.0.path,&self.0.env));
                    bounds::Condition::index_equal(actual,*index).truth_in(&self.0.path)==Some(true)
                };
                let mut at=self.len();
                let loaded=self.code().rev().find_map(|inst|{
                    at-=1;
                    let InstKind::ViewLoad{dst,address,index,stride,word:offset}=&inst.kind else{return None;};
                    if *offset!=word||self.0.registers[*dst]!=Scalar::U32{return None;}
                    let selected=match &layout {
                        DataLayout::Declared=>self.stride(*stride)==Some(u64::from(ir::align_up(ty.size,ty.align)))
                            && self.frame_root(*address).is_some_and(|actual|self.same(actual,root))&&index_equal(*index),
                        DataLayout::Parameter{stride:step,offset:field,..}=>self.constant(*index)==Some(0)&&self.stride(*stride)==Some(0)
                            && self.offset_position(*address).is_some_and(|(base,index,stride,offset)|{
                                let same_root=if self.0.registers[root]==Scalar::U64{self.split_address(base).is_some_and(|actual|self.same(actual,root))}
                                    else{self.frame_root(base).is_some_and(|actual|self.same(actual,root))};
                                same_root&&self.same(stride,step.regs[0])&&self.same(offset,field.regs[0])&&index_equal(index)
                            }),
                    };
                    (selected&&!self.code().skip(at+1).any(|later|self.writes(later,inst))).then_some(*dst)
                });
                let root=match loaded{Some(root)=>effects::StorageWrite::Frame{root,range:None},None=>effects::StorageWrite::Descendants{root}};
                storage.push((root,element,DataLayout::Declared));
            }
        }
        Ok((!storage.is_empty()).then_some(storage))
    }
    fn byte_overlap(a:&std::ops::Range<u128>,b:&std::ops::Range<u128>)->bool {a.start<b.end&&b.start<a.end}
    fn store_overlaps(&self,read:&IndexedSource,read_word:u32,read_scalar:Scalar,source:&IndexedSource,offset:Option<Reg>,word:u32,scalar:Scalar)->bool {
        if let (IndexedSource::Data{binding:a,index:ai},IndexedSource::Data{binding:b,index:bi})=(read,source) {
            if a!=b{return false;}
            let offset=match offset {None=>0,Some(reg)=>match self.constant(reg){Some(value)=>u128::from(value),None=>return true}};
            if matches!((self.constant(*ai),self.constant(*bi)),(Some(a),Some(b))if a!=b){return false;}
            let a=u128::from(read_word);let b=offset+u128::from(word);
            return Self::byte_overlap(&(a..a+u128::from(read_scalar.size())),&(b..b+u128::from(scalar.size())));
        }
        let IndexedSource::View{address,index,stride}=read else{return true;};
        let mut selected=source;let mut written_word=u128::from(word);
        while let IndexedSource::Pointer{address,index,stride,..}=selected {
            let Some(pointer)=self.split_address(*address)else{break;};
            let Some((_,Inst{kind:InstKind::PositionAddress{address:reference,..},..}))=self.definition(pointer)else{break;};
            let ir::CallStorage::Indexed(storage)=&reference.storage else{break;};
            let pointer_offset=if *stride==0{0}else{
                let Some(index)=self.constant(*index)else{return true;};u128::from(index)*u128::from(*stride)
            };
            let offset=match reference.offset {None=>0,Some(reg)=>match self.constant(reg){Some(value)=>u128::from(value),None=>return true}};
            written_word+=pointer_offset+offset+u128::from(reference.word);
            selected=storage;
        }
        let(other,other_index,other_stride,same_stride)=match selected{
            IndexedSource::View{address,index,stride:other} => (address,index,self.stride(*other),self.same_stride(*stride,*other)),
            IndexedSource::Pointer{address,index,stride:other,..} => (address,index,Some(u64::from(*other)),self.stride(*stride)==Some(u64::from(*other))),
            _=>return true,
        };
        if !self.same_address(*address,*other) {
            // A new allocation cannot alias a live input Frame. Other roots
            // retain the strict owned-child relation; source offsets stay intact.
            let roots=[*address,*other].map(|parts|self.frame_root(parts)
                .or_else(||self.offset_position(parts).and_then(|(base,..)|self.frame_root(base))));
            if let [Some(a),Some(b)]=roots && (self.disjoint_roots(a,b)
                ||self.allocated(a)&&self.definition(b).is_none()||self.allocated(b)&&self.definition(a).is_none()
                ||self.frame_descendant(a,b)||self.frame_descendant(b,a)){return false;}
            return true;
        }
        let offset=match offset {None=>0,Some(reg)=>match self.constant(reg){Some(value)=>value,None=>return true}};
        let size=u128::from(read_scalar.size());let other_size=u128::from(scalar.size());
        let (a,b)=if let(Some(a),Some(b),Some(stride),Some(other_stride))=(self.constant(*index),self.constant(*other_index),self.stride(*stride),other_stride) {
            (u128::from(a)*u128::from(stride)+u128::from(read_word),u128::from(b)*u128::from(other_stride)+u128::from(offset)+written_word)
        }else if same_stride&&self.same(*index,*other_index){(u128::from(read_word),u128::from(offset)+written_word)}else{return true;};
        Self::byte_overlap(&(a..a+size),&(b..b+other_size))
    }
    fn call_memory_writes(&self,call:&Call,mut writes:impl FnMut(&view::Borrow)->bool)->bool {
        if let Some(usage)=call.memory_use {
            // The first use owns additional memory effects. C typed positions
            // remain in the original use group; Reference ABI writes separately
            // describe their actual local slots and indexed bytes.
            return match &self.0.view_model.uses[usage].requirement {
                view::Requirement::Memory{borrow,access,..}if access.is_some_and(|access|access.writes())=>
                    writes(&self.0.current_address_borrow(borrow)),
                _=>false,
            };
        }
        if call.arguments.iter().any(|arg|matches!(arg,CallArgument::Reference{access,..}if access.writes()))||call.memory.iter().any(|effect|effect.access.writes()){return true;}
        // Private completion and explicit transfers preserve payload values; storage and address lifetimes remain owned by Frame and the transfer checker.
        if Self::pointer_call(call).is_some()||Self::allocation(call)||matches!(&call.target,CallTarget::External(name)if name.library==crate::application::RUNTIME_LIBRARY&&(name.symbol.starts_with("blink_complete_")||matches!(name.symbol.as_str(),"blink_upload"|"blink_download"))){return false;}
        match &call.target {
            CallTarget::Defined(name)=>!self.0.functions.writes_only_references(name),
            _=>true,
        }
    }
    fn writes_local(&self,inst:&Inst,slot:usize)->bool {
        if inst.local_writes().any(|written| written == slot){return true;}
        if let InstKind::Call(call)=&inst.kind {
            return self.call_memory_writes(call,|borrow|self.0.address_write_positions(borrow).is_none_or(|positions|positions.iter().any(|written|owned_presence::Presence::selected(written,&Position::Local(slot))!=owned_presence::Presence::None)));
        }
        match &inst.kind {
            InstKind::If{yes,no,..}=>yes.iter().chain(no).any(|inst|self.writes_local(inst,slot)),
            InstKind::Loop{condition,body,..}=>condition.iter().chain(body).any(|inst|self.writes_local(inst,slot)),
            InstKind::Scope(body)=>body.iter().any(|inst|self.writes_local(inst,slot)),
            _=>false,
        }
    }
    fn writes(&self,inst:&Inst,load:&Inst)->bool {
        match &load.kind {
            InstKind::Load{dst,binding,index,word}=>self.writes_indexed(inst,&IndexedSource::Data{binding:*binding,index:*index},*word,self.0.registers[*dst]),
            InstKind::ViewLoad{dst,address,index,stride,word}=>self.writes_indexed(inst,&IndexedSource::View{address:*address,index:*index,stride:*stride},*word,self.0.registers[*dst]),
            _=>false,
        }
    }
    fn writes_indexed(&self,inst:&Inst,read:&IndexedSource,word:u32,scalar:Scalar)->bool {
        match &inst.kind {
            InstKind::Store{binding,index,word:at_word,value}=>self.store_overlaps(read,word,scalar,&IndexedSource::Data{binding:*binding,index:*index},None,*at_word,self.0.registers[*value]),
            InstKind::StoreIndexed{source,offset,word:at_word,value}=>self.store_overlaps(read,word,scalar,source,Some(*offset),*at_word,self.0.registers[*value]),
            InstKind::Call(call)=>{
                if call.arguments.iter().any(|argument|match argument {
                    CallArgument::Reference{ty,access,address,..}if access.writes()=>match &address.storage {
                        ir::CallStorage::Indexed(source)=>ty.components.iter().any(|(written_scalar,written_word)|self.store_overlaps(read,word,scalar,source,address.offset,address.word+written_word,*written_scalar)),
                        ir::CallStorage::Local{..}=>false,
                    },
                    _=>false,
                }){return true;}
                let allocated=matches!(read,IndexedSource::View{address,..}if self.frame_root(*address).is_some_and(|root|self.allocated(root)));
                self.call_memory_writes(call,|borrow|{
                    if let CallTarget::Defined(name)=&call.target
                        && self.0.functions.writes_only_references(name) {
                        return call.memory.iter().any(|effect|effect.access.writes());
                    }
                    if borrow.pending()||address::external_memory(borrow){return true;}
                    borrow.sources.entries().any(|(source,when)|when.possible()&&match source{
                        view::Source::Position(origin)if allocated&&matches!(origin.root,address::Root::Local{..})=>false,
                        _=>true,
                    })
                })
            },
            InstKind::If{yes,no,..}=>yes.iter().chain(no).any(|inst|self.writes_indexed(inst,read,word,scalar)),
            InstKind::Loop{condition,body,..}=>condition.iter().chain(body).any(|inst|self.writes_indexed(inst,read,word,scalar)),
            InstKind::Scope(body)=>body.iter().any(|inst|self.writes_indexed(inst,read,word,scalar)),
            InstKind::Atomic(_)|InstKind::Append(_)=>true,
            _=>false,
        }
    }
    // Query the original loop IR and scoped facts; no store-value history is retained.
    fn same_frame(&self, root: Reg, data: &Data, writes: &effects::LoopWrites) -> bool {
        if let Some(place) = &data.place {
            let Place::Local(_, slots, _) = place.storage() else { return false; };
            if writes.external || writes.unknown_storage || slots.iter().any(|slot| writes.locals.contains(slot)) { return false; }
            if matches!(self.definition(root), Some((_, Inst { kind: InstKind::LoadLocal(_, slot), .. })) if *slot == slots[0]) { return true; }
        }
        self.same(root, data.root.regs[0])
    }
    fn index_equal(&self, reg: Reg, index: &Value, path: &bounds::Clause, env: &BTreeMap<String, Binding>) -> bool {
        if self.same(reg, index.regs[0]) { return true; }
        let ty = Ty::scalar(Scalar::U32);
        let proof = bounds::scoped_proof(&ty, bounds::Proof::default().with_identity(ValueIdentity::Register(reg)), path, env);
        let value = Value { ty, regs: vec![reg], proof, literal: false, uniform: false, constant: self.constant(reg).map(|value| vec![value]) };
        bounds::Condition::comparison(Comparison::Equal, &value, index).truth_in(path) == Some(true)
    }
    fn cast_source(&self, reg: Reg) -> Option<Reg> {
        match &self.definition(reg)?.1.kind { InstKind::Cast { src, .. } => Some(*src), _ => None }
    }
    fn split_address(&self, address: [Reg; 2]) -> Option<Reg> {
        let low = self.cast_source(address[0])?;
        let high = self.cast_source(address[1])?;
        let InstKind::Binary { op: BinaryOp::ShiftRightLogical, a, b, .. } = &self.definition(high)?.1.kind else { return None; };
        (self.constant(*b) == Some(32) && self.same(low, *a)).then_some(low)
    }
    fn offset_position(&self, address: [Reg; 2]) -> Option<([Reg; 2], Reg, Reg, Reg)> {
        let pointer = self.split_address(address)?;
        let InstKind::Binary { op: BinaryOp::IAdd, a: base, b: at, .. } = &self.definition(pointer)?.1.kind else { return None; };
        let InstKind::Binary { op: BinaryOp::IAdd, a: scaled, b: offset, .. } = &self.definition(*at)?.1.kind else { return None; };
        let InstKind::Binary { op: BinaryOp::IMul, a: index, b: stride, .. } = &self.definition(*scaled)?.1.kind else { return None; };
        let InstKind::Binary { op: BinaryOp::Or, a: low, b: high, .. } = &self.definition(*base)?.1.kind else { return None; };
        let InstKind::Binary { op: BinaryOp::ShiftLeft, a: upper, b: bits, .. } = &self.definition(*high)?.1.kind else { return None; };
        if self.constant(*bits) != Some(32) { return None; }
        Some(([self.cast_source(*low)?, self.cast_source(*upper)?], self.cast_source(*index)?, self.cast_source(*stride)?, self.cast_source(*offset)?))
    }
    fn dataset_position(&self, source: &IndexedSource, data: &Data, index: &Value, state: &flow::LoopState, writes: &effects::LoopWrites) -> bool {
        let IndexedSource::View { address, index: at, stride } = source else { return false; };
        if self.constant(*at) == Some(0) && self.stride(*stride) == Some(0)
            && let Some((base, at, step, offset)) = self.offset_position(*address) {
            let root = if data.root.ty == Ty::scalar(Scalar::U64) {
                self.split_address(base).is_some_and(|root| self.same(root, data.root.regs[0]))
            } else { self.frame_root(base).is_some_and(|root| self.same_frame(root, data, writes)) };
            let layout = match &data.layout {
                DataLayout::Declared => self.constant(step) == Some(u64::from(ir::align_up(data.ty.size, data.ty.align))) && self.constant(offset) == Some(0),
                DataLayout::Parameter { stride, offset: field, .. } => self.same(step, stride.regs[0]) && self.same(offset, field.regs[0]),
            };
            return root && layout && self.index_equal(at, index, &state.path, &state.env);
        }
        self.stride(*stride) == Some(u64::from(ir::align_up(data.ty.size, data.ty.align)))
            && self.frame_root(*address).is_some_and(|root| self.same_frame(root, data, writes))
            && self.index_equal(*at, index, &state.path, &state.env)
    }
    fn same_position(&self, a: &IndexedSource, b: &IndexedSource, exact_index: bool) -> bool {
        let (IndexedSource::View { address: a, index: ai, stride: astride }, IndexedSource::View { address: b, index: bi, stride: bstride }) = (a, b) else { return false; };
        if self.same_address(*a, *b) && self.same_stride(*astride, *bstride) && (!exact_index || self.same(*ai, *bi)) { return true; }
        if self.constant(*ai) == Some(0) && self.constant(*bi) == Some(0) && self.stride(*astride) == Some(0) && self.stride(*bstride) == Some(0)
            && let (Some((a, ai, astep, afield)), Some((b, bi, bstep, bfield))) = (self.offset_position(*a), self.offset_position(*b)) {
            return self.same_address(a,b) && self.same(astep,bstep) && self.same(afield,bfield) && (!exact_index || self.same(ai,bi));
        }
        false
    }
    fn stored_position<'a>(&self, code: &'a [Inst], data: &Data, index: &Value, word: u32, scalar: Scalar, state: &flow::LoopState, writes: &effects::LoopWrites) -> Option<&'a IndexedSource> {
        for inst in code {
            match &inst.kind {
                InstKind::StoreIndexed { source, offset, word: at_word, value }
                    if *at_word == word && self.0.registers[*value] == scalar && self.constant(*offset) == Some(0)
                        && self.dataset_position(source, data, index, state, writes) => return Some(source),
                InstKind::Call(call) => for argument in &call.arguments {
                    if let CallArgument::Reference { ty, access, address, .. } = argument
                        && access.writes() && address.offset.is_none_or(|offset| self.constant(offset)==Some(0))
                        && ty.components.iter().any(|(part,at)|*part==scalar && address.word+at==word)
                        && let ir::CallStorage::Indexed(source)=&address.storage
                        && self.dataset_position(source,data,index,state,writes) { return Some(source); }
                },
                InstKind::If { yes, no, .. } => if let Some(source) = self.stored_position(yes, data, index, word, scalar, state, writes).or_else(|| self.stored_position(no, data, index, word, scalar, state, writes)) { return Some(source); },
                InstKind::Scope(body) => if let Some(source) = self.stored_position(body, data, index, word, scalar, state, writes) { return Some(source); },
                InstKind::Loop{condition,body,..}=>if let Some(source)=self.stored_position(condition,data,index,word,scalar,state,writes).or_else(||self.stored_position(body,data,index,word,scalar,state,writes)){return Some(source);},
                _ => {},
            }
        }
        None
    }
    fn field_store(&self, source: &IndexedSource, word: u32, scalar: Scalar, inst: &Inst, exact_index: bool) -> Option<Reg> {
        let InstKind::StoreIndexed { source: written, offset, word: at_word, value } = &inst.kind else { return None; };
        (self.constant(*offset) == Some(0) && *at_word == word && self.0.registers[*value] == scalar && self.same_position(source, written, exact_index)).then_some(*value)
    }
    fn current_reference_writes(&self, call: &Call, source: &IndexedSource, word: u32, scalar: Scalar) -> bool {
        let CallTarget::Defined(name)=&call.target else { return false; };
        self.0.functions.writes_only_references(name) && call.arguments.iter().all(|argument| {
            let CallArgument::Reference { ty, access, address, .. }=argument else { return true; };
            if !access.writes() { return true; }
            let ir::CallStorage::Indexed(written)=&address.storage else { return true; };
            !ty.components.iter().any(|(part,at)|self.store_overlaps(source,word,scalar,written,address.offset,address.word+at,*part))
                || address.offset.is_none_or(|offset|self.constant(offset)==Some(0)) && self.same_position(source,written,true)
        })
    }
    fn iteration_interference(&self, code: &[Inst], source: &IndexedSource, word: u32, scalar: Scalar, path: &bounds::Clause, state: &flow::LoopState, controls: &BTreeMap<Reg, Value>) -> Option<bounds::Proof> {
        let mut result: Option<bounds::Proof> = None;
        for inst in code {
            let proof = match &inst.kind {
                InstKind::If { condition, yes, no } => {
                    let raw = Value { ty: Ty::scalar(Scalar::Bool), regs: vec![*condition], proof: bounds::Proof::default(), literal: false, uniform: false, constant: None };
                    let condition = bounds::Condition::of(controls.get(condition).unwrap_or(&raw));
                    let truth = condition.truth_in(path);
                    let a = (truth != Some(false)).then(||self.iteration_interference(yes,source,word,scalar,&condition.assume(path,true),state,controls)).flatten();
                    let b = (truth != Some(true)).then(||self.iteration_interference(no,source,word,scalar,&condition.assume(path,false),state,controls)).flatten();
                    match (a,b) { (Some(mut a),Some(b))=>{a.join_condition(&b,1,None);Some(a)}, (a,None)|(None,a)=>a }
                },
                InstKind::Scope(body) => self.iteration_interference(body,source,word,scalar,path,state,controls),
                InstKind::Loop{condition,body,..}=>match(self.iteration_interference(condition,source,word,scalar,path,state,controls),self.iteration_interference(body,source,word,scalar,path,state,controls)){
                    (Some(mut a),Some(b))=>{a.join_condition(&b,1,None);Some(a)},(a,None)|(None,a)=>a,
                },
                InstKind::Call(call) if self.current_reference_writes(call,source,word,scalar) => None,
                _ if self.field_store(source,word,scalar,inst,true).is_some() => None,
                InstKind::StoreIndexed { value, .. } if self.writes_indexed(inst,source,word,scalar) => {
                    // An overlapping store to another index can affect an earlier iteration.
                    // Its actual scalar value is another possible final value, regardless of order.
                    Some(if self.field_store(source,word,scalar,inst,false).is_some() {
                        bounds::scoped_proof(&Ty::scalar(scalar), bounds::Proof::default().with_identity(ValueIdentity::Register(*value)), path, &state.env)
                    } else { bounds::Proof::default() })
                },
                _ if self.writes_indexed(inst,source,word,scalar) => Some(bounds::Proof::default()),
                _ => None,
            };
            if let Some(proof) = proof { if let Some(common)=&mut result { common.join_condition(&proof,1,None); } else { result=Some(proof); } }
        }
        result
    }
    fn stored_paths(&self, code: &[Inst], mut next: Option<bounds::Proof>, data: &Data, source: &IndexedSource, word: u32, scalar: Scalar, path: &bounds::Clause, state: &flow::LoopState, controls: &BTreeMap<Reg, Value>) -> (Option<bounds::Proof>, Option<bounds::Proof>, Option<bounds::Proof>, Option<bounds::Proof>) {
        fn join(a: Option<bounds::Proof>, b: Option<bounds::Proof>,choice:Option<&bounds::Condition>) -> Option<bounds::Proof> {
            match (a, b) { (Some(mut a), Some(b)) => { a.join_condition(&b, 1, choice); Some(a) }, (a, None) | (None, a) => a }
        }
        let mut continues = None;
        let mut exits = None;
        let mut breaks = None;
        for inst in code {
            let Some(current) = next.take() else { break; };
            match &inst.kind {
                InstKind::Continue => continues = join(continues, Some(current),None),
                InstKind::Break => breaks = join(breaks, Some(current),None),
                InstKind::Exit => exits = join(exits, Some(current),None),
                InstKind::If { condition, yes, no } => {
                    let raw = Value { ty: Ty::scalar(Scalar::Bool), regs: vec![*condition], proof: bounds::Proof::default(), literal: false, uniform: false, constant: None };
                    let condition = bounds::Condition::of(controls.get(condition).unwrap_or(&raw));
                    let truth = condition.truth_in(path);
                    let a = if truth == Some(false) { (None, None, None, None) } else { self.stored_paths(yes, Some(current.clone()), data, source, word, scalar, &condition.assume(path, true), state, controls) };
                    let b = if truth == Some(true) { (None, None, None, None) } else { self.stored_paths(no, Some(current), data, source, word, scalar, &condition.assume(path, false), state, controls) };
                    next = join(a.0,b.0,Some(&condition)); continues = join(continues,join(a.1,b.1,Some(&condition)),None); exits = join(exits,join(a.2,b.2,Some(&condition)),None); breaks = join(breaks,join(a.3,b.3,Some(&condition)),None);
                },
                InstKind::Scope(body) => {
                    let (normal, continued, exited, broken) = self.stored_paths(body, Some(current), data, source, word, scalar, path, state, controls);
                    next = join(normal,exited,None); continues = join(continues,continued,None); breaks = join(breaks,broken,None);
                },
                InstKind::Call(call @ Call{target:CallTarget::Defined(name),..}) if self.current_reference_writes(call,source,word,scalar) && self.writes_indexed(inst,source,word,scalar) => {
                    let contract=&self.0.functions.units[name].as_ref().unwrap().contract;
                    let mut common:Option<bounds::Proof>=None;
                    if let Some(invocation)=self.0.view_model.calls.iter().find(|entry|Some(entry.memory)==call.memory_use){
                        for (identity,storage) in &invocation.arguments {
                            let ValueIdentity::ParameterStorage(index)=identity else{continue};
                            for source in &storage.view_borrow(0).sources {
                                let view::Source::Position(origin)=source else{continue};
                                if !matches!(&origin.root,address::Root::Resource{borrow:root,..}if self.0.application_element_scopes(&[],root,data,(path,&state.env)).into_iter().any(|(scope,complete,_)|scope.is_empty()&&complete)){continue;}
                                for (component,_) in origin.words.iter().enumerate().filter(|(component,at)|**at==word&&origin.layout.parts[*component].0==scalar){
                                    let proof=contract.written.get(*index).and_then(Option::as_ref).map(|written|{
                                        let value=self.0.returned_storage(written);
                                        bounds::Proof::capture(&value).project(&[component],value.regs.len()).at_call(&invocation.arguments)
                                    }).unwrap_or_default();
                                    if let Some(common)=&mut common{common.join_condition(&proof,1,None);}else{common=Some(proof);}
                                }
                            }
                        }
                    }
                    next=Some(common.unwrap_or_default());
                },
                _ => next = Some(if let Some(value) = self.field_store(source, word, scalar, inst, true) {
                    bounds::Proof::capture(&Value{ty:Ty::scalar(scalar),regs:vec![value],proof:bounds::scoped_proof(&Ty::scalar(scalar),bounds::Proof::default().with_identity(ValueIdentity::Register(value)),path,&state.env),literal:false,uniform:false,constant:self.constant(value).map(|bits|vec![bits])})
                } else if self.writes_indexed(inst, source, word, scalar) { bounds::Proof::default() } else { current }),
            }
        }
        (next, continues, exits, breaks)
    }

    fn block_contains_snapshot_candidate(&self, code: &[Inst], load: &Inst) -> bool {
        code.iter().any(|candidate| match (&candidate.kind, &load.kind) {
            (InstKind::If { yes, no, .. }, _) =>
                self.block_contains_snapshot_candidate(yes, load)
                    || self.block_contains_snapshot_candidate(no, load),
            (InstKind::Scope(body), _) => self.block_contains_snapshot_candidate(body, load),
            (InstKind::Load { dst: a, binding: left, word: w1, .. },
             InstKind::Load { dst: b, binding: right, word: w2, .. }) =>
                left == right && w1 == w2 && self.0.registers[*a] == self.0.registers[*b],
            (InstKind::ViewLoad { dst: a, word: left, .. },
             InstKind::ViewLoad { dst: b, word: right, .. }) =>
                left == right && self.0.registers[*a] == self.0.registers[*b],
            _ => false,
        })
    }
    fn prior_snapshot<'a>(&'a self, code: impl Iterator<Item=&'a Inst>, mut load: &'a Inst, branches: &mut BTreeMap<Reg, Option<bool>>) -> Option<Reg> {
        let mut result=None;
        for inst in code {
            let nested=match &inst.kind {
                InstKind::If { yes, no, .. }
                    if !self.block_contains_snapshot_candidate(yes, load)
                        && !self.block_contains_snapshot_candidate(no, load) => {
                    None
                }
                InstKind::If{condition,yes,no}=>{
                    let mut source=*condition;
                    while let Some((at,Inst{kind:InstKind::LoadLocal(_,slot),..}))=self.definition(source) {
                        let Some(Inst{kind:InstKind::StoreLocal(_,value),..})=self.code().rev().skip(self.len()-at).find(|inst|self.writes_local(inst,*slot))else{break;};
                        source=*value;
                    }
                    let truth = *branches.entry(source).or_insert_with(|| {
                        let value=Value{ty:Ty::scalar(Scalar::Bool),regs:vec![source],proof:bounds::Proof::default(),literal:false,uniform:false,constant:None};
                        self.0.condition_truth(&bounds::Condition::of(&value), None)
                    });
                    truth.map(|truth| if truth { yes.as_slice() } else { no.as_slice() })
                },
                InstKind::Scope(body)=>Some(body.as_slice()),
                _=>None,
            };
            if let Some(reg)=nested.and_then(|code|self.prior_snapshot(code.iter().rev(), load, branches)) {
                result=Some(reg);load=self.definition(reg).unwrap().1;
            }
            if self.writes(inst,load){break;}
            if self.same_load(inst,load) {
                if let InstKind::Load{dst,..}|InstKind::ViewLoad{dst,..}=inst.kind{result=Some(dst);load=inst;}
            }
        }
        result
    }
    fn snapshot(&self, reg: Reg, branches: &mut BTreeMap<Reg, Option<bool>>) -> Reg {
        let Some((at,load))=self.definition(reg)else{return reg;};
        if !matches!(load.kind,InstKind::Load{..}|InstKind::ViewLoad{..}){return reg;}
        self.prior_snapshot(self.code().rev().skip(self.len()-at), load, branches).unwrap_or(reg)
    }
}
impl Builder<'_> {
    pub(super) fn application_condition_writes(&self,code:&[Inst],place:&Place,component:usize)->bool {
        let(positions,index)=self.value_memory_location(place);
        let position=positions.get(component);
        fn writes(loads:&ApplicationLoads<'_, '_>,code:&[Inst],position:Option<&Position>,scalar:Scalar,index:Option<bounds::Operand>)->bool {
            code.iter().any(|inst|match &inst.kind {
                InstKind::If{yes,no,..}=>writes(loads,yes,position,scalar,index)||writes(loads,no,position,scalar,index),
                InstKind::Loop{condition,body,..}=>writes(loads,condition,position,scalar,index)||writes(loads,body,position,scalar,index),
                InstKind::Scope(body)|InstKind::Cleanup{body,..}=>writes(loads,body,position,scalar,index),
                InstKind::Call(call)=>{
                    let affected=|borrow:&view::Borrow|borrow.pending()||position.is_none_or(|position|address::MemoryEffect::of(borrow,Access::InOut,None).written_bytes(position,scalar,index)!=0);
                    position.is_some_and(|p|matches!(p,Position::Local(slot)if inst.local_writes().any(|written| written == *slot)))
                        ||loads.call_memory_writes(call,affected)
                        ||call.memory_use.is_some_and(|first|loads.0.view_model.uses.iter().skip(first+1)
                            .take_while(|usage|matches!(&usage.requirement,view::Requirement::Memory{target:address::MemoryTarget::Call(at),..}if *at==first))
                            .any(|usage|matches!(&usage.requirement,view::Requirement::Memory{borrow,access:Some(access),..}if access.writes()&&affected(&loads.0.current_address_borrow(borrow)))))
                },
                InstKind::Store{..}|InstKind::StoreIndexed{..}|InstKind::Atomic(_)|InstKind::Append(_)=>true,
                _=>position.is_some_and(|p|matches!(p,Position::Local(slot)if inst.local_writes().any(|written| written == *slot))),
            })
        }
        writes(&ApplicationLoads(self),code,position,place.ty().components[component].0,index)
    }
    pub(super) fn application_iteration_interference<'s>(&self, data: &DataReference, index: &Value, proof: &mut bounds::Proof, states: impl Iterator<Item=&'s flow::LoopState>+Clone, controls: &BTreeMap<Reg, Value>, writes: &effects::LoopWrites) {
        let loads = ApplicationLoads(self);
        let (offset, ty) = data.element_layout();
        for (component, &(scalar, word)) in ty.components.iter().enumerate() {
            let mut current = proof.project(&[component], ty.components.len());
            for state in states.clone() {
                if let Some(source) = loads.stored_position(&self.code, &data.data, index, offset + word, scalar, state, writes) {
                    if let Some(other) = loads.iteration_interference(&self.code, source, offset + word, scalar, &state.path, state, controls) { current.join_condition(&other, 1, None); }
                } else { current = bounds::Proof::default(); }
            }
            proof.assign(&[component], ty.components.len(), current);
        }
    }
    pub(super) fn application_indexed_proof<'s>(&self, data: &Data, index: &Value, initial:&Value, states: impl Iterator<Item=&'s flow::LoopState>+Clone, controls: &BTreeMap<Reg, Value>, writes: &effects::LoopWrites) -> (bounds::Proof,bounds::Proof) {
        let loads = ApplicationLoads(self);
        let mut output = [bounds::Proof::default(),bounds::Proof::default()];
        let initial=bounds::Proof::capture(initial);
        for (component, &(scalar, word)) in data.ty.components.iter().enumerate() {
            let mut common: [Option<bounds::Proof>;2] = [None,None];
            for state in states.clone() {
                let paths=if let Some(source) = loads.stored_position(&self.code, data, index, word, scalar, state, writes) {
                    let (normal, continued, _, broken) = loads.stored_paths(&self.code, Some(initial.project(&[component],data.ty.components.len())), data, source, word, scalar, &state.path, state, controls);
                    [[normal,continued].into_iter().flatten().reduce(|mut a,b|{a.join_condition(&b,1,None);a}),broken]
                }else{[Some(bounds::Proof::default()),Some(bounds::Proof::default())]};
                for (part,proof) in paths.into_iter().enumerate(){if let Some(proof)=proof{
                    if let Some(value)=&mut common[part]{value.join_condition(&proof,1,None);}else{common[part]=Some(proof);}
                }}
            }
            for (part,proof) in common.into_iter().enumerate(){if let Some(proof)=proof{output[part].assign(&[component],data.ty.components.len(),proof);}}
        }
        let [normal,broken]=output;(normal,broken)
    }
    pub(super) fn application_read_snapshots(&self) -> impl FnMut(Reg) -> Reg + '_ {
        let loads = ApplicationLoads(self);
        let mut branches = BTreeMap::new();
        move |reg| loads.snapshot(reg, &mut branches)
    }
}

pub(super) fn pointer_type() -> Ty {
    let mut ty = Ty::vector(Scalar::U32, 2);
    ty.name = "__blink_frame".into();
    ty
}
pub(super) fn argument(value: Value) -> CallArgument {
    CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
        dataset_borrow:None, ty: value.ty,
        values: value.regs,
    }
}
pub(super) fn runtime_call(
    name: &str,
    arguments: Vec<CallArgument>,
    result: Option<Ty>,
    values: Vec<Reg>,
    error: Option<Reg>,
    failure: Option<FailureContinuation>,
) -> Inst {
    Inst::new(InstKind::Call(Call {
        failure,
        error,
        target: CallTarget::External(ExternalName {
            library: crate::application::RUNTIME_LIBRARY.into(),
            symbol: format!("blink_{name}"),
        }),
        arguments,
        memory: Vec::new(),
        memory_use: None,
        result,
        values,
    }))
}
pub(crate) async fn compile_application(
    world: &World,
    instance: &crate::generics::Instance,
) -> Result<crate::application::Application> {
    let d = &instance.definition;
    let result_name=unsafe { world.call(instance.bindings.type_name(world,&d.function.result(),
        (&d.source,&d.module,&d.imports),d.function.span())).await }?;
    let result = if result_name.name == "void" || result_name.is_never() {
        None
    } else {
        Some(unsafe { world.call(world.ty(
            &result_name,
            &d.source,
            d.function.span(),
            &d.module,
            &d.imports,
        )).await }?)
    };
    if d.function.kind() != FunctionKind::Value
        || d.function.external().is_some()
        || d.function.target() != Target::Cpu
        || !d.function.params().is_empty()
        || !result.is_some_and(|result| {
            [Ty::scalar(Scalar::U32), Ty::scalar(Scalar::I32)].contains(&result)
        })
    {
        return d.source.fail(
            d.function.span(),
            "B262",
            "应用入口需要无参数的 u32／i32 普通函数，显式 on cpu；返回值是进程退出码",
        );
    }
    let context = Program {
        name: instance.name.clone(),
        source_origin: d.source_origin(),
        generated_origin: None,
        target: Target::Cpu,
        buffers: Vec::new(),
        accelerations: Vec::new(),
        textures: Vec::new(),
        uniforms: Vec::new(),
        uniform_bytes: 0,
        native_frame: None,
        resources: None,
        source_stages: 0,
        stages: Vec::new(),
        functions: std::sync::Arc::from([]),
        prefix_requirements: Vec::new(),
        prefix_guarantees: Vec::new(), element_requirements: Vec::new(), element_guarantees: Vec::new(), length_requirements: Vec::new(), imports: BTreeMap::new(),
    };
    let mut builder = unsafe { world.call(Builder::new(world, instance, &context, 0, Some(&instance.name))).await }?;
    let entry_anchor = GenerationAnchor::Definition {
        origin: d.source_origin().map(Arc::new), instance: Arc::from(instance.name.as_str()),
    };
    builder.application = Some(Context::default());
    builder.in_function = true;
    let result = unsafe { builder.world.call(builder.native_function(instance.clone(), Vec::new(), d.function.span(), Some(Arc::new(GeneratedOrigin {
        reason: GenerationReason::ApplicationEntryCall, trigger: Some(entry_anchor.clone()),
    })), None, None)).await }?;
    let result = builder.ordinary_return(result, d.function.span())?.unwrap();
    let app = builder.application.take().unwrap();
    let mut body = ValueCode {
        entry_origin: None,
        generated_origin: Some(Arc::new(GeneratedOrigin { reason: GenerationReason::ApplicationEntry, trigger: Some(entry_anchor.clone()) })),
        registers: builder.registers,
        locals: builder.locals,
        code: builder.code,
        inputs: Vec::new(),
        outputs: result.regs,
    };
    let finalized = unsafe { world.call(native_function::finish(world, builder.functions, &mut body, &builder.view_model)).await }?;
    let mut functions = finalized.functions;
    let imports = finalized.imports;
    let resources = live_resources(&mut body, &mut functions, app.frame, finalized.resources, &entry_anchor);
    crate::optimize::value_code(&mut body);
    Ok(crate::application::Application {
        compiler: world.worker.compiler().clone(),
        name: instance.name.clone(),
        body,
        functions,
        imports,
        resources,
    })
}

fn runtime_calls(code: &mut [Inst], visitor: &mut impl FnMut(&mut ExternalName)) {
    for instruction in code {
        match &mut (instruction).kind {
            InstKind::Call(call) => {
                if let CallTarget::External(name) = &mut call.target
                    && name.library == crate::application::RUNTIME_LIBRARY
                {
                    visitor(name)
                }
            }
            InstKind::Scope(body) | InstKind::Cleanup { body, .. } => runtime_calls(body, visitor),
            InstKind::If { yes, no, .. } => {
                runtime_calls(yes, visitor);
                runtime_calls(no, visitor);
            }
            InstKind::Loop {
                condition, body, ..
            } => {
                runtime_calls(condition, visitor);
                runtime_calls(body, visitor);
            }
            _ => {}
        }
    }
}
fn visit_stage_bodies(
    stages: &mut [Stage],
    mut visit: impl FnMut(&mut Vec<Scalar>, &mut BTreeMap<usize, Ty>, &mut Vec<Inst>),
) {
    for stage in stages {
        visit(&mut stage.registers, &mut stage.locals, &mut stage.code);
        let values = match &mut stage.operation {
            Operation::CustomReduce(values) => values,
            Operation::CustomScan(scan) => &mut scan.values,
            Operation::CustomSegmentedReduce(reduction) => &mut reduction.values,
            _ => continue,
        };
        for body in values.project.iter_mut().chain([&mut values.identity, &mut values.combine]) {
            visit(&mut body.registers, &mut body.locals, &mut body.code);
        }
    }
}

fn declarations(
    codes: &mut [&mut Vec<Inst>],
    mut resources: crate::application::Resources,
) -> crate::application::Resources {
    fn collect(code: &mut [Inst], names: &mut BTreeSet<String>, pending: &mut Vec<usize>) {
        runtime_calls(code, &mut |call| {
            if names.insert(call.symbol.clone())
                && let Some(index) = call.symbol.strip_prefix("blink_run_").and_then(|index| index.parse().ok())
            {
                pending.push(index);
            }
        });
    }
    let mut names = BTreeSet::new();
    let mut pending = Vec::new();
    for code in codes.iter_mut() {
        collect(code, &mut names, &mut pending);
    }
    while let Some(index) = pending.pop() {
        visit_stage_bodies(&mut resources.programs[index].1.stages, |_, _, code| {
            collect(code, &mut names, &mut pending);
        });
    }
    fn retain<T>(
        items: Vec<T>,
        prefix: &str,
        names: &BTreeSet<String>,
        renames: &mut BTreeMap<String, String>,
    ) -> Vec<T> {
        let mut live = Vec::new();
        for (old, item) in items.into_iter().enumerate() {
            let name = format!("blink_{prefix}_{old}");
            if names.contains(&name) {
                if old != live.len() {
                    renames.insert(name, format!("blink_{prefix}_{}", live.len()));
                }
                live.push(item);
            }
        }
        live
    }
    let mut renames = BTreeMap::new();
    let types = retain(resources.types, "allocate", &names, &mut renames);
    let mut programs = retain(resources.programs, "run", &names, &mut renames);
    let completions = retain(resources.completions, "complete", &names, &mut renames);
    if !renames.is_empty() {
        let rewrite = |code: &mut [Inst]| {
            runtime_calls(code, &mut |call| {
                if let Some(name) = renames.get(&call.symbol) {
                    call.symbol = name.clone();
                }
            });
        };
        for code in codes.iter_mut() {
            rewrite(code);
        }
        for (_, program) in &mut programs {
            visit_stage_bodies(&mut program.stages, |_, _, code| rewrite(code));
        }
    }
    crate::application::Resources { types, programs, completions, task_starts: names.contains("blink_task_start") }
}
pub(super) fn module_resources(
    functions: &mut [FunctionBody],
    resources: crate::application::Resources,
) -> Option<crate::application::Resources> {
    let mut codes = functions
        .iter_mut()
        .map(|function| &mut function.body.code)
        .collect::<Vec<_>>();
    let resources = declarations(&mut codes, resources);
    (!resources.types.is_empty()
        || !resources.programs.is_empty()
        || !resources.completions.is_empty()
        || resources.task_starts)
        .then_some(resources)
}
fn live_resources(
    body: &mut ValueCode,
    functions: &mut [FunctionBody],
    frame: Option<Vec<Reg>>,
    resources: crate::application::Resources,
    entry_anchor: &GenerationAnchor,
) -> Option<crate::application::Resources> {
    let frame = frame?;
    fn uses(code: &[Inst], registers: &[Reg]) -> bool {
        code.iter().any(|inst| {
            !inst.all_operands(&mut |reg| !registers.contains(&reg))
                || match &(inst).kind {
                    InstKind::Scope(code) => uses(code, registers),
                    InstKind::If { yes, no, .. } => uses(yes, registers) || uses(no, registers),
                    InstKind::Loop {
                        condition, body, ..
                    } => uses(condition, registers) || uses(body, registers),
                    _ => false,
                }
        })
    }
    if !uses(&body.code, &frame) {
        return None;
    }
    let mut codes = std::iter::once(&mut body.code)
        .chain(functions.iter_mut().map(|function| &mut function.body.code))
        .collect::<Vec<_>>();
    let resources = declarations(&mut codes, resources);
    let base = body.registers.len();
    body.registers.extend([Scalar::U64, Scalar::U64, Scalar::U32, Scalar::U32, Scalar::Bool]);
    let [receiver, context, address, zero, ready] = [base, base + 1, base + 2, base + 3, base + 4];
    let origin = |reason| Arc::new(GeneratedOrigin { reason, trigger: Some(entry_anchor.clone()) });
    let generated = |kind, reason| Inst::generated(kind, origin(reason));
    let readiness = origin(GenerationReason::FrameReadiness);
    fn root_failure(code: &mut [Inst], frame: &[Reg]) {
        for inst in code {
            if let InstKind::Raise(error)=&inst.kind {
                inst.kind=runtime_call("application_failed",vec![
                    CallArgument::Value{origin:None,dataset_borrow:None,ty:pointer_type(),values:frame.to_vec()},
                    CallArgument::Value{origin:None,dataset_borrow:None,ty:Ty::scalar(Scalar::U64),values:vec![*error]},
                ],None,Vec::new(),None,None).kind;
            } else {match &mut inst.kind {
                InstKind::Scope(body)=>root_failure(body,frame),
                InstKind::If{yes,no,..}=>{root_failure(yes,frame);root_failure(no,frame);}
                InstKind::Loop{condition,body,..}=>{root_failure(condition,frame);root_failure(body,frame);}
                _=>{}
            }}
        }
    }
    let mut code = std::mem::take(&mut body.code);
    root_failure(&mut code,&frame);
    code.push(generated(runtime_call(
        "destroy",
        vec![CallArgument::Value { #[cfg(feature = "compiler")] origin: None, dataset_borrow:None, ty: pointer_type(), values: frame.clone() }],
        None,
        Vec::new(),
        None,
        None,
    ).kind, GenerationReason::FrameDestroy));
    body.code = vec![
        generated(InstKind::FunctionAddress(receiver, CallTarget::External(ExternalName {
            library: crate::application::RUNTIME_LIBRARY.into(),
            symbol: "blink_application_error".into(),
        })), GenerationReason::FrameErrorReceiver),
        generated(InstKind::Constant(context, Scalar::U64, 0), GenerationReason::FrameErrorContext),
        generated(runtime_call(
            "create",
            vec![
                CallArgument::Value { #[cfg(feature = "compiler")] origin: None, dataset_borrow:None, ty: Ty::scalar(Scalar::U64), values: vec![receiver] },
                CallArgument::Value { #[cfg(feature = "compiler")] origin: None, dataset_borrow:None, ty: Ty::scalar(Scalar::U64), values: vec![context] },
            ],
            Some(pointer_type()),
            frame.clone(),
            None,
            None,
        ).kind, GenerationReason::FrameCreate),
        Inst::generated(InstKind::Binary { dst: address, op: BinaryOp::Or, a: frame[0], b: frame[1] }, readiness.clone()),
        Inst::generated(InstKind::Constant(zero, Scalar::U32, 0), readiness.clone()),
        Inst::generated(InstKind::Binary { dst: ready, op: BinaryOp::Compare(Comparison::NotEqual), a: address, b: zero }, readiness.clone()),
        Inst::generated(InstKind::If {
            condition: ready,
            yes: code,
            no: vec![generated(InstKind::Constant(body.outputs[0], body.registers[body.outputs[0]], 1), GenerationReason::FrameFailureExit)],
        }, readiness),
    ];
    Some(resources)
}
impl Builder<'_> {
    // 独立检查 CPU 函数时，参数提供符号身份和真实接口形状；不创建物理存储。
    // Invocation read/write/inout/atomic交接原实际地址及布局，Host借用与拥有值沿原Frame句柄。
    pub(super) async fn application_parameter(
        &mut self,
        binding: usize,
        parameter: usize,
        access: Access,
        scalar: Scalar,
    ) -> Result<Data> {
        if access == Access::Value {
            self.application_frame(self.scope.span())?;
        }
        let reg = self.reg(scalar);
        let mut handle = self.application_root(reg);
        if access==Access::Value{handle.proof.set_view_borrow(0,view::Borrow::source(view::Source::Data(ValueIdentity::ResourceParameter(parameter,0))));}
        let length_reg = self.reg(Scalar::U32);
        let count = self.program.buffers[binding].count;
        let mut length = Value {
            ty: Ty::scalar(Scalar::U32),
            regs: vec![length_reg],
            proof: count
                .map(|n| bounds::Proof::unsigned(n, n))
                .unwrap_or_default(),
            literal: false,
            uniform: false,
            constant: count.map(|n| vec![Bits::from(n)]),
        };
        length.proof = bounds::Proof::capture(&length).with_domains(Domains::from([
            IndexDomain::ApplicationLength(handle.identity()),
        ]));
        let initialization = if access == Access::Write {
            Some(Position::Parameter(self.application_output_scope(),0))
        } else {
            None
        };
        let element_ty=self.program.buffers[binding].ty.clone();
        let mut model = unsafe { self.world.call(self.unknown_element_model(&element_ty, (&residency::Residency::parameter(parameter), &[]))).await }?;
        if access == Access::Write {
            model.restrict_owned(&owned_presence::Presence::None);
        }
        let elements = ElementFacts::Known(model);
        Ok(Box::new(DataPayload {
            prefixes: BTreeMap::new(),
            elements,
            root: handle,
            place: None,
            initialization,
            ty: element_ty,
            length,
            count: self.program.buffers[binding].count,
            ownership: if access == Access::Value {
                Ownership::Owned
            } else {
                Ownership::Borrowed
            },
            access: if access == Access::Value { Access::InOut } else { access },
            residency: residency::Residency::parameter(parameter),
            layout: DataLayout::Declared,
        }))
    }
    pub(super) fn application_iteration_outputs(
        &self,
        begin: &Value,
        limit: &Value,
        selected: Option<&Data>,
    ) -> Vec<(Data, Vec<u32>)> {
        let domains = limit.proof.domains(0);
        self.env.values().flat_map(Binding::application_resources).chain(selected).filter_map(|data| {
            let scope = data.initialization.as_ref()?;
            if !(domains.contains(&IndexDomain::ApplicationLength(data.root.identity()))
                || limit.identity() == data.length.identity()
                || limit.minimum() == limit.maximum() && limit.minimum() == data.length.minimum() && limit.maximum() == data.length.maximum()) { return None; }
            let words = data.ty.components.iter().filter_map(|(_, offset)| {
                let word = *offset;
                let prefix = begin.minimum();
                (self.application_field_initialized(data, word) || begin.maximum() == prefix &&
                    scope.application_prefix_initialized(prefix,word,&self.initialized)).then_some(word)
            }).collect();
            Some((data.clone(), words))
        }).collect()
    }
    pub(super) fn application_iteration_prefix(
        &self,
        outputs: &[(Data, Vec<u32>)],
        index: ownership::ApplicationIndex,
    ) -> Vec<(Data, Vec<u32>)> {
        outputs
            .iter()
            .map(|(data, words)| {
                (
                    data.clone(),
                    words
                        .iter()
                        .copied()
                        .filter(|word| {
                            data.initialization.as_ref().unwrap()
                                .application_word_initialized(Some(index), *word, &self.initialized)
                        })
                        .collect(),
                )
            })
            .collect()
    }
    pub(super) fn initialize_application_words(&mut self, data: &Data, words: &[u32]) {
        let scope = data.initialization.as_ref().unwrap();
        self.initialized.extend(
            words
                .iter()
                .map(|word| scope.application_word(None,*word)),
        );
    }
    pub(super) fn application_empty_initialization(
        &self,
        mut initialized: BTreeSet<Position>,
    ) -> BTreeSet<Position> {
        for data in self.env.values().flat_map(Binding::application_resources) {
            if let Some(scope)=&data.initialization && data.length.maximum()==0 {
                initialized.extend(data.ty.components.iter().map(|(_,offset)|scope.application_word(None,*offset)));
            }
        }
        initialized
    }
    pub(super) fn application_initialization_state(&self) -> BTreeSet<Position> {
        self.application_empty_initialization(self.initialized.clone())
    }
    pub(super) fn take_application_initialization_state(&mut self) -> BTreeSet<Position> {
        let initialized = std::mem::take(&mut self.initialized);
        self.application_empty_initialization(initialized)
    }
    fn initialize_application_domain(
        &mut self,
        interface: &crate::module::domain::Interface,
        data: &[DataReference],
    ) {
        for output in &interface.outputs {
            let offset = data[output.binding].element_layout().0;
            let Some(scope) = &data[output.binding].initialization else {
                continue;
            };
            match output.domain {
                Some(domain) if self.lengths_equal(&data[output.binding].length, &data[domain].length) => {
                    self.initialized.extend(
                        output
                            .words
                            .iter()
                            .map(|word| scope.application_word(None,offset + word)),
                    );
                }
                None => self.initialized.extend(output.words.iter().map(|word| {
                    scope.application_word(Some(ownership::ApplicationIndex::Constant(0)),offset + word)
                })),
                _ => {}
            }
        }
    }
    pub(super) fn application_output_scope(&mut self) -> usize {
        self.output_scopes += 1;
        self.output_scopes
    }
    pub(super) fn application_initialized(&self, data: &Data) -> bool {
        data.ty
            .components
            .iter()
            .all(|(_, offset)| self.application_field_initialized(data, *offset))
    }
    pub(super) fn application_field_initialized(&self, data: &Data, word: u32) -> bool {
        self.application_field_initialized_in(data, word, &self.initialized)
    }
    pub(super) fn application_field_initialized_in(
        &self,
        data: &Data,
        word: u32,
        initialized: &BTreeSet<Position>,
    ) -> bool {
        fn complete(builder:&Builder<'_>,scope:&Position,word:u32,length:Option<u32>,initialized:&BTreeSet<Position>,assuming:owned_presence::Presence)->bool{
            let scope=builder.resolve_owned_initialization(scope,Some(&assuming));
            let facts=initialized.iter().map(|position|builder.resolve_owned_initialization(position,Some(&assuming))).collect();
            let ready = match length {
                Some(length) => scope.application_prefix_initialized(length,word,&facts),
                None => scope.application_word_initialized(None,word,&facts),
            };
            if ready { return true; }
            match scope {
                Position::Conditional(when,a,b)=>complete(builder,&a,word,length,initialized,assuming.clone().and(when.as_ref().clone()))
                    &&complete(builder,&b,word,length,initialized,assuming.and(when.as_ref().clone().not())),
                Position::Both(a,b)=>complete(builder,&a,word,length,initialized,assuming.clone())
                    &&complete(builder,&b,word,length,initialized,assuming),
                Position::Indexed(choices,index)=>choices.iter().enumerate().all(|(element,choice)|complete(builder,choice,word,length,initialized,assuming.clone().and(owned_presence::Presence::indices(Comparison::Equal,index,ownership::ApplicationIndex::Constant(element as u32))))),
                _=>false,
            }
        }
        let Some(scope)=&data.initialization else{return true;};
        let length=(data.length.minimum()==data.length.maximum()).then(||data.length.minimum());
        complete(self,scope,word,length,initialized,owned_presence::Presence::All)
    }
    pub(super) async fn require_application_initialized(&mut self, data: &Data, span: Span) -> Result<Option<Box<returns::Returned>>> {
        let elements = if data.length.maximum() != 0
            && !matches!(data.elements, ElementFacts::Unknown | ElementFacts::Input(_) | ElementFacts::Cleanup(_))
        {
            let mut model = unsafe { self.world.call(self.element_model(&data.ty, &data.elements, (&data.residency, &[]), span)).await }?;
            self.resolve_owned_model(&mut model, None);
            let available = match model.as_ref() {
                returns::Returned::OwnedValue(value) => value.all_available(),
                returns::Returned::Dataset(data) => data.ownership == Ownership::Owned,
                _ => true,
            };
            if !available {
                return self.fail(span, "B227", "完整dataset交接需要全部活动拥有元素；移出位置须先补回");
            }
            unsafe { self.world.call(self.require_returned_initialized(&mut model, span)).await }?;
            Some(model)
        } else {
            None
        };
        if !self.application_initialized(data)
            && !self.defer_position_initialization(data.ty.components.iter().map(|(_,word)|data.initialization.as_ref().unwrap().application_word(None,*word)).collect(),"B142",span)? {
            return self.fail(
                span,
                "B142",
                "读取或 inout 借用了尚未完整初始化的 dataset；write 输出须先完整写入所需元素与字段",
            );
        }
        Ok(elements)
    }


    pub(super) fn application_frame(&mut self, span: Span) -> Result<Value> {
        if self.application.is_none() {
            return self.fail(
                span,
                "B266",
                "动态数据与 CPU／GPU program 调用属于普通 CPU 应用主流程",
            );
        }
        let ty = pointer_type();
        let regs = if let Some(regs) = self.application.as_ref().unwrap().frame.clone() {
            regs
        } else {
            let regs: Vec<_> = ty.components.iter().map(|(s, _)| self.reg(*s)).collect();
            self.application.as_mut().unwrap().frame = Some(regs.clone());
            regs
        };
        let frame = Value {
            proof: bounds::Proof::default(),
            literal: false,
            regs,
            ty,
            uniform: false,
            constant: None,
        };
        Ok(frame)
    }
    pub(super) async fn task_call(
        &mut self, name:&str, args:&Expressions, span:Span,
    ) -> Result<Option<returns::Returned>> {
        use returns::Returned;
        if !self.application_entry() || self.program.target != Target::Cpu {
            return self.fail(span,"B224","任务启动和等待属于普通CPU函数");
        }
        if name=="wait_task" {
            if args.len()!=1 { return self.fail(span,"B157","wait_task需要一个实际拥有任务"); }
            let Some(owner)=unsafe { self.world.call(self.evaluate_returned(&args.at(0),None)).await }? else {return Ok(None);};
            let Returned::OwnedValue(owner)=owner else {
                return self.fail(span,"B224","wait_task需要一个实际拥有任务");
            };
            if owner.value.ty.task_result().is_none() {
                return self.fail(span,"B224","wait_task需要一个实际拥有任务");
            }
            return unsafe { self.world.call(self.wait_task_value(owner.value, span, async |_, value| Ok(Some(value)))).await };
        }
        if args.len()<2 {
            return self.fail(span,"B157","start_task需要静态普通CPU函数、拥有参数记录和借用位置");
        }
        let worker=args.at(0);
        let worker=Self::user_owned_function(self.scope.source(),&worker)?;
        let crate::frontend::NameResolution::Unique(key)=self.world.resolve(
            worker,self.scope.module(),self.scope.imports(),true,
        ) else { return self.fail(span,"B157","找不到实际工作函数"); };
        let callee=self.world.functions[&key].clone();
        let f=&callee.function;
        let declared=f.params();
        if f.kind()!=FunctionKind::Value || f.target()!=Target::Cpu || f.c_abi()
            || declared.iter().any(|param|!matches!(param.access(),Access::Value|Access::Read|Access::Write|Access::InOut))
        {
            return self.fail(span,"B224","工作必须使用普通CPU值参数或read、write、inout位置");
        }
        let value_count=declared.iter().filter(|param|param.access()==Access::Value).count();
        if args.len()!=2+declared.len()-value_count {
            return self.fail(span,"B157","按值参数放入拥有记录，借用位置按声明顺序提供");
        }
        let source=args.at(1);
        let Some(mut input)=unsafe { self.world.call(self.evaluate_returned(&source,None)).await }? else {return Ok(None);};
        let mut datasets=Vec::new();
        residency::visit(&mut input,&mut residency::Input::new(0),&mut |data,_|datasets.push(data.clone()));
        for data in datasets {
            unsafe { self.world.call(self.require_application_initialized(&data,span)).await }?;
            self.application_cpu(&data,span)?;
        }
        let packed=self.returned_storage(&input);
        if packed.ty.fields.len()!=value_count {
            return self.fail(source.span(),"B162","任务拥有参数记录只包含实际按值参数");
        }
        let held=match input {
            Returned::OwnedValue(owner)=>Some(self.hold_binding(Binding::OwnedValue(owner))),
            _=>None,
        };
        let signature=cooperative::CallParameters::Declaration(&callee);
        let mut next_position=2;
        let positions=declared.iter().map(|param| {
            if param.access()==Access::Value {None}
            else {
                let arg=args.at(next_position);
                next_position+=1;
                Some(arg)
            }
        }).collect::<Vec<_>>();
        let mut field=0;
        let parameters=unsafe { self.world.call(call_arguments::build(self,
            signature,declared.len(),span,
            |index|positions[index].as_ref().map_or(source.span(),Expr::span),
            async |builder,index,parameter| {
                if let Some(arg)=&positions[index] {
                    let argument=unsafe { builder.world.call(call_arguments::source(builder,signature,index,parameter,arg)).await }?;
                    let available=match argument.as_ref() {
                        None | Some(cooperative::Argument::Reference(_)
                            | cooperative::Argument::Value(_)
                            | cooperative::Argument::OwnedReference { .. }
                            | cooperative::Argument::BorrowedData(_)) => true,
                        Some(cooperative::Argument::OwnedValue(value)) if parameter.access==Access::Read =>
                            value.place().is_some(),
                        _=>false,
                    };
                    if !available {
                        return builder.fail(arg.span(),"B224","任务位置尚不能交接该拥有值或Frame资源");
                    }
                    return Ok(argument);
                }
                let value=if let Some(held)=&held {
                    let Binding::OwnedValue(owner)=builder.env[held].clone() else {unreachable!()};
                    let value=builder.unpack_record(&owner,field);
                    if let Binding::OwnedValue(owner)=builder.env.get_mut(held).unwrap()
                        && let Some(payload)=owner.payloads[field].as_deref_mut()
                    {
                        payload.restrict_owned(&owned_presence::Presence::None);
                    }
                    value
                } else {
                    Returned::value(builder.record_component(&packed,field))
                };
                field+=1;
                Ok(Some(cooperative::Argument::resource(value)))
            },
        )).await }?;
        let Some(parameters)=parameters else {return Ok(None);};
        if let Some(held)=held {self.env.remove(&held);}
        let worker=unsafe { self.world.call(self.specialized_function(key,&parameters,span)).await }?;
        unsafe { self.world.call(self.native_function(worker,parameters,span,None,None,Some(packed))).await }
    }

    pub(super) async fn emit_task_start(&mut self,worker:&str,source:Value,arguments:Vec<CallArgument>,captured:view::Borrow,
        result:Ty,mut outputs:Vec<view::Borrow>,initialized:view::Borrow,guards:Vec<(usize, outputs::TaskGuard)>,conditional_positions:Vec<Position>,
        borrowed_arguments:BTreeMap<usize,Option<(Ty,Access)>>,dataset_writes:Vec<(Access, DataReference)>,
        retired_outputs:Vec<(Place,returns::Returned)>,memory:usize,call:usize,span:Span)
        -> Result<Option<returns::Returned>> {
        let guarded = guards.iter().filter_map(|(parameter, guard)|
            guard.frame_output_type().map(|_| (*parameter, guard))).collect::<BTreeMap<_,_>>();
        let positions = arguments.iter().enumerate().filter(|(_, argument)| !matches!(argument,
            CallArgument::Value { ty, .. } if ty.name == "__blink_frame")).enumerate().filter_map(|(field, (argument_index, argument))| {
            let (ty, access, origin, layout) = match argument {
                CallArgument::Reference { ty, access, origin: Some(origin), .. } => (ty, access, origin, None),
                CallArgument::Value { ty, origin: Some(origin), .. } => {
                    let (element, access) = borrowed_arguments.get(&argument_index)?.as_ref()?;
                    (element, access, origin, Some(ty.clone()))
                }
                _ => return None,
            };
            if !access.writes() || !(if layout.is_some() { ty.owns_resources() } else { ty.contains_dataset() }) {
                return None;
            }
            let ArgumentOrigin::Parameter { index, .. } = origin.as_ref() else { return None; };
            let guard = guarded.get(index).copied();
            let guard = if layout.is_none() || *access == Access::Write { guard } else { None };
            Some((field, ty.clone(), guard, layout, *access))
        }).collect::<Vec<_>>();
        let formal_frame=usize::from(!matches!(arguments.first(),
            Some(CallArgument::Value{ty,..}) if ty.name=="__blink_frame"))*2;
        let (input_width, completed_inputs) = native_function::task_completion_parameters(
            arguments.iter().map(native_function::task_argument_slots), 0,
        );
        let first_completion = formal_frame + input_width;
        let mut packed=self.task_argument_packet(arguments,&borrowed_arguments,span)?;
        let overflow = || Diagnostic {
            source: self.scope.source().clone(), span, code: "B224", message: "任务完成布局过大".into(),
        };
        let mut tail = result.task_wait_layout().ok_or_else(&overflow)?.size;
        let mut selection = Vec::new();
        let mut drop_positions = Vec::new();
        let mut drop_datasets = Vec::new();
        for (field, ty, guard, layout, access) in &positions {
            let (name, offset, _) = &mut packed.ty.fields[*field];
            let ready = if let Some(guard) = guard {
                let base = tail.checked_next_multiple_of(Scalar::Bool.align()).ok_or_else(&overflow)?;
                let width = u32::try_from(ty.task_output_owners().len()).ok()
                    .and_then(|count| count.checked_mul(Scalar::Bool.size())).ok_or_else(&overflow)?;
                tail = base.checked_add(width).ok_or_else(&overflow)?;
                name.push_str(&format!(":{base}"));
                selection.push((base, *guard));
                Some(base)
            } else {
                None
            };
            if let Some(layout) = layout {
                drop_datasets.push((*offset, layout, ty, *access, ready));
            } else {
                drop_positions.push((*offset, ty, ready));
            }
        }
        let mut work_output=Ty::unit();
        work_output.name="@task_output".into();
        work_output.size=tail;
        let mut completions=Vec::<view::TaskInputCompletion>::new();
        for completion in completed_inputs {
            let parent = completion.parent();
            let task=match parent {
                Some((parent,component))=>completions[parent].task.task_projection(view::TaskProjection::Returned(component,None)),
                None=>self.view_model.calls[call].arguments[&ValueIdentity::Register(completion.input()+formal_frame)].view_borrow(0),
            };
            let result = completion.result;
            let layout=self.task_wait_output_layout(&result,&task,span)?.task_completion_storage().ok_or_else(&overflow)?;
            let offset = work_output.append_field_layout(&layout).ok_or_else(&overflow)?;
            let input=first_completion+2*completions.len();
            for (part,_) in result.task_components().into_iter().enumerate() {
                let slot=result.components.len()+part;
                let (_,word)=layout.components[3+slot];
                let (payload,_)=layout.task_completion_payload();
                self.view_model.calls[call].arguments.insert(ValueIdentity::TaskCompletion(input,slot),
                    bounds::Proof::default().with_identity(ValueIdentity::TaskWorkCompletion(offset+payload,word-payload)));
            }
            work_output.fields.push((completions.len().to_string(),offset,Box::new(layout.clone())));
            completions.push(view::TaskInputCompletion{offset,result,layout,task,parent});
        }
        for component in view_contracts::components(&result) {
            let actual=outputs[component].at_call(&self.view_model.calls[call].arguments);
            outputs[component]=if actual.empty() || actual.pending() || actual.unresolved() {
                self.view_model.result(call,view_contracts::Output::Returned(component))
            } else {
                actual
            };
        }
        packed.ty.arguments.push(work_output);
        let cleanup = if drop_positions.is_empty() && drop_datasets.is_empty() && !result.owns_resources() {
            None
        } else {
            Some(unsafe { self.world.call(self.task_completion_cleanup(&drop_positions, &drop_datasets, &result, span)).await }?)
        };
        let selector = if selection.is_empty() {
            None
        } else {
            Some(unsafe { self.world.call(self.task_selection(&packed, &result, &selection, span)).await }?)
        };
        let error_ty=Ty::task_start_error(source.ty.clone()).ok_or_else(||Diagnostic{
            source:self.scope.source().clone(),span,code:"B224",message:"任务错误载荷布局过大".into(),
        })?;
        let result=Ty::result(Ty::task(result),error_ty).ok_or_else(||Diagnostic{
            source:self.scope.source().clone(),span,code:"B224",message:"任务结果布局过大".into(),
        })?;
        let entry=self.reg(Scalar::U64);
        self.emit(InstKind::FunctionAddress(entry,CallTarget::Defined(worker.into())));
        let entry=Value { ty:Ty::scalar(Scalar::U64),regs:vec![entry],
            proof:bounds::Proof::default(),uniform:false,literal:false,constant:None };
        let cleanup_reg=self.reg(Scalar::U64);
        if let Some(cleanup)=cleanup {
            self.emit(InstKind::FunctionAddress(cleanup_reg,CallTarget::Defined(cleanup)));
        } else {
            self.emit(InstKind::Constant(cleanup_reg,Scalar::U64,0));
        }
        let cleanup=Value { ty:Ty::scalar(Scalar::U64),regs:vec![cleanup_reg],
            proof:bounds::Proof::default(),uniform:false,literal:false,constant:None };
        let (slots,storage)=self.local_slots(&packed.ty);
        for (slot,reg) in slots.into_iter().zip(packed.regs) { self.emit(InstKind::StoreLocal(slot,reg)); }
        let input=CallArgument::Reference { origin:None,ty:packed.ty,access:Access::Read,
            address:CallReference{storage:CallStorage::Local{base:storage.start,len:storage.len()},offset:None,word:0} };
        let frame=argument(self.application_frame(span)?);
        let mut values:Vec<_>=result.components.iter().map(|(s,_)|self.reg(*s)).collect();
        let error=self.reg(Scalar::U64);
        let mut start_arguments = vec![frame.clone(), input, argument(entry), argument(cleanup)];
        if let Some(selector) = selector {
            let reg = self.reg(Scalar::U64);
            self.emit(InstKind::FunctionAddress(reg, CallTarget::Defined(selector)));
            start_arguments.push(CallArgument::Value { ty: Ty::scalar(Scalar::U64), values: vec![reg], dataset_borrow: None, origin: None });
        }
        let mut call=runtime_call("task_start",start_arguments,
            Some(result.clone()),values.clone(),Some(error),self.failure_continuation(Vec::new()));
        if let InstKind::Call(call)=&mut call.kind {
            call.memory_use=Some(memory);
        }
        self.emit(call.kind);
        let payload=ir::align_up(4,result.align);
        let at=result.components.iter().position(|(_,word)|*word==payload).unwrap();
        let task=view::Borrow::source(view::Source::Task{captures:Box::new(captured),outputs,initialized:Box::new(initialized),guards: guards.into_iter().map(|(_, guard)| guard).collect(),completions});
        let mut proof={let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(at,task);component_proof};
        let error_type=(*result.variants[1].ty).clone();
        let error_input=payload+error_type.fields[1].1;
        for component in view_contracts::components(&source.ty){
            let word=source.ty.components[component].1;
            let at=result.components.iter().position(|(_,offset)|*offset==error_input+word).unwrap();
            proof.set_view_borrow(at,source.proof.view_borrow(component));
        }
        let mut error_regs=Vec::with_capacity(error_type.components.len());
        error_regs.push(values[1]);
        error_regs.extend(source.regs);
        let error=Value{ty:error_type,regs:error_regs,proof:bounds::Proof::default(),uniform:false,literal:false,constant:None};
        let failure=self.variant_value(result.clone(),1,Some(error));
        let failed=values[0];
        for (index,_) in result.components.iter().enumerate().filter(|(_,(_,word))|*word>=error_input) {
            let dst=self.reg(Scalar::U32);
            self.emit(InstKind::Select{dst,condition:failed,a:failure.regs[index],b:values[index]});
            values[index]=dst;
        }
        let value=Value{ty:result,regs:values,proof,uniform:false,literal:false,constant:None};
        let accepted=owned_presence::Presence::input(&self.result_condition(&value,span)?);
        unsafe { self.world.call(self.owned_when(&accepted,None,None,async |builder| {
            let owner=argument(builder.result_payload(&value,true).unwrap());
            let submit=|builder:&mut Builder<'_>| {
                for (_, reference) in &dataset_writes {
                    if reference.root.ty.components[0].0==Scalar::U32 {
                        let (stride,offset)=builder.application_reference_layout(reference);
                        let wide=builder.reg(Scalar::U64);
                        builder.emit(InstKind::Cast{dst:wide,src:offset.regs[0]});
                        let offset=Value{ty:Ty::scalar(Scalar::U64),regs:vec![wide],proof:bounds::Proof::default(),
                            literal:false,uniform:false,constant:None};
                        builder.application_address(reference.root.regs[0],reference.root.identity(),offset,
                            reference.element_layout().1.size,stride,reference.length.clone(),span)?;
                    }
                }
                let status=if let Some(pending)=builder.pending_error {
                    let status=builder.reg(Scalar::U64);
                    builder.emit(InstKind::LoadLocal(status,pending.slot()));
                    status
                } else {
                    builder.constant(Scalar::U64,0).regs[0]
                };
                let preparation=CallArgument::Value{
                    origin:None,ty:Ty::scalar(Scalar::U64),values:vec![status],dataset_borrow:None,
                };
                let error=builder.reg(Scalar::U64);
                let failure=builder.failure_continuation(Vec::new());
                builder.emit(runtime_call("task_submit",vec![frame,owner,preparation],None,Vec::new(),Some(error),failure).kind);
                Ok(())
            };
            let retired_datasets = dataset_writes.iter().filter_map(|(access, reference)|
                (*access == Access::Write && reference.element_layout().1.owns_resources()).then_some(reference)
            ).collect::<Vec<_>>();
            if retired_outputs.is_empty() && retired_datasets.is_empty() {
                return submit(builder);
            }
            let consumed=retired_outputs.iter().map(|(place,_)|place.clone()).collect();
            unsafe { builder.world.call(builder.cleanup_group(consumed,async |builder| {
                for (_,old) in retired_outputs {
                    unsafe { builder.world.call(builder.drop_returned(old,&view::Borrow::default(),span)).await }?;
                }
                for reference in retired_datasets {
                    unsafe { builder.world.call(builder.retire_application_elements(reference, span)).await }?;
                }
                submit(builder)
            })).await }
        })).await }?;
        if !conditional_positions.is_empty() {
            let positions=conditional_positions.into_iter()
                .map(|position|Position::selected(accepted.clone(),position,Position::Initialized)).collect::<Vec<_>>();
            self.forget_output_positions(&positions,span)?;
        }
        unsafe { self.world.call(self.task_value(value, residency::Residency::cpu())).await }.map(Some)
    }

    fn task_argument_packet(&mut self,arguments:Vec<CallArgument>,borrowed_arguments:&BTreeMap<usize,Option<(Ty,Access)>>,span:Span)->Result<Value> {
        let mut packet=Ty::unit();
        packet.name="@task_arguments".into();
        let mut registers=Vec::new();
        let source=self.scope.source().clone();
        let overflow=||Diagnostic{source:source.clone(),span,code:"B224",message:"任务参数布局过大".into()};
        for (index,argument) in arguments.into_iter().enumerate() {
            let owned_position=matches!(&argument,
                CallArgument::Reference{ty,access,..} if access.writes()&&ty.owns_resources());
            let (ty,values)=match argument {
                CallArgument::Value{ty,..} if ty.name=="__blink_frame"=>continue,
                CallArgument::Value{ty,values,..}=>(ty,values),
                CallArgument::Reference{address,..}=>{
                    let dst=self.reg(Scalar::U64);
                    self.emit(InstKind::PositionAddress{dst,address});
                    (Ty::builtin("address").unwrap(),vec![dst])
                },
            };
            let offset = packet.append_field_layout(&ty).ok_or_else(&overflow)?;
            let field=packet.fields.len();
            let name=if owned_position {
                format!("{}{field}",Ty::TASK_OWNED_REFERENCE_PREFIX)
            } else {
                match borrowed_arguments.get(&index) {
                    Some(Some((element,access))) => {
                        let element_index=packet.arguments.len();
                        packet.arguments.push(element.clone());
                        let mode=match access {
                            Access::Read=>"read",
                            Access::Write=>"write",
                            Access::InOut=>"inout",
                            _=>unreachable!(),
                        };
                        format!("{}{mode}:{element_index}",Ty::TASK_BORROWED_DATASET_PREFIX)
                    }
                    Some(None) => format!("{}{field}", Ty::TASK_READ_FIELD_PREFIX),
                    None => field.to_string(),
                }
            };
            packet.fields.push((name,offset,Box::new(ty)));
            registers.extend(values);
        }
        packet.finish_record_layout().ok_or_else(overflow)?;
        Ok(Value{ty:packet,regs:registers,proof:bounds::Proof::default(),uniform:false,literal:false,constant:None})
    }

    pub(super) fn task_wait_output_layout(&self,result:&Ty,sources:&view::Borrow,span:Span)->Result<Ty> {
        let mut layout=result.task_wait_layout().ok_or_else(||Diagnostic{
            source:self.scope.source().clone(),span,code:"B224",message:"任务结果布局过大".into(),
        })?;
        let mut words=BTreeSet::new();
        for (source,when) in sources.sources.entries() {
            if self.path.overlaps(&when) && let view::Source::Task{completions,..}=source {
                for input in completions {
                    layout.size=layout.size.max(input.offset+input.layout.size);
                    layout.align=layout.align.max(input.layout.align);
                    words.extend(input.layout.components.iter().map(|&(scalar,word)|(input.offset+word,scalar)));
                }
            }
        }
        layout.components.extend(words.into_iter().map(|(word,scalar)|(scalar,word)));
        Ok(layout)
    }

    pub(super) async fn complete_task_input_slots(&mut self,completions:&[view::TaskInputCompletion],
        layout:&Ty,slots:&[usize],base:u32,span:Span)->Result<()> {
        let mut outputs=Vec::<Arc<native_function::TaskCompletionOutput>>::new();
        for input in completions {
            let words=input.layout.components.iter().map(|&(scalar,word)| {
                let component=layout.components.iter().position(|&(kind,offset)|
                    kind==scalar&&offset==base+input.offset+word).unwrap();
                slots[component]
            }).collect();
            let parent=input.parent.map(|(parent,component)|(outputs[parent].clone(),component));
            let output=Arc::new(native_function::TaskCompletionOutput{
                result:input.result.clone(),layout:input.layout.clone(),slots:words,task:input.task.clone(),parent,span,
            });
            unsafe { self.world.call(output.complete_if_ready(self)).await }?;
            outputs.push(output);
        }
        Ok(())
    }

    pub(super) fn initialize_task_completion(&mut self,layout:&Ty,slots:&[usize]) {
        let (payload,work)=layout.task_completion_payload();
        for (&slot,value) in slots.iter().zip([Ty::TASK_COMPLETION_PENDING,work.size,payload]) {
            let value=self.constant(Scalar::U32,Bits::from(value)).regs[0];
            self.emit(InstKind::StoreLocal(slot,value));
        }
    }

    pub(super) async fn wait_task_value<T>(&mut self, mut owner: Value, span: Span,
        consume: impl std::ops::AsyncFnOnce(&mut Self, returns::Returned) -> Result<T>,
    ) -> Result<T> {
        let result=owner.ty.task_result().unwrap().clone();
        let sources=owner.proof.view_borrow(0);
        let writes=sources.task_projection(view::TaskProjection::Writes).restricted(&self.path);
        let memory=unsafe { self.world.call(self.call_address_memory([(writes,Some(Access::Write))],span)).await }?;
        owner.ty=Ty::scalar(Scalar::U64);
        let completion=self.task_completion_destination(owner.regs[0],&result,&sources);
        let identity=completion.and_then(|(_,identity)|identity);
        let layout=self.task_wait_output_layout(&result,&sources,span)?;
        let returned_words=result.components.len()+result.task_components().len();
        let storage_layout=layout.task_completion_storage().ok_or_else(||Diagnostic {
            source:self.scope.source().clone(),span,code:"B224",message:"任务完成布局过大".into(),
        })?;
        let (storage,local)=if let Some((address,_))=completion {
            let zero=self.constant(Scalar::U32,0).regs[0];
            (CallStorage::Indexed(IndexedSource::View{address,index:zero,stride:ElementStride::Constant(0)}),None)
        } else {
            let (slots,range)=self.local_slots(&storage_layout);
            self.initialize_task_completion(&storage_layout,&slots);
            let output=native_function::TaskCompletionOutput{
                result:result.clone(),layout:storage_layout.clone(),slots,task:sources.clone(),parent:None,span,
            };
            (CallStorage::Local{base:range.start,len:range.len()},Some(output))
        };
        let output=CallArgument::Reference { origin:None,ty:storage_layout,access:Access::InOut,
            address:CallReference{storage,offset:None,word:0} };
        let frame=argument(self.application_frame(span)?);
        let error=self.reg(Scalar::U64);
        let mut failure=self.failure_continuation(Vec::new());
        if let Some(FailureContinuation::Scope(scope))=&mut failure
            && let Some(output)=&local && output.has_owned_outputs() {
            scope.task_outputs.push(output.clone());
        }
        let mut call=runtime_call("task_wait",vec![frame,argument(owner),output],
            None,Vec::new(),Some(error),failure);
        if let InstKind::Call(call)=&mut call.kind {
            call.memory_use=Some(memory);
        }
        self.emit(call.kind);
        // In a cleanup group an error is saved while sibling destructors continue.
        let complete=async |builder:&mut Self| {
            let completion=completion.map(|(address,_)| {
                let zero=builder.constant(Scalar::U32,0).regs[0];
                let payload=builder.reg(Scalar::U32);
                builder.emit(InstKind::ViewLoad{dst:payload,address,index:zero,stride:ElementStride::Constant(0),word:Ty::TASK_COMPLETION_PAYLOAD});
                (address,payload)
            });
            let mut regs=layout.components[..returned_words].iter().enumerate().map(|(component,&(scalar,word))| {
                let dst=builder.reg(scalar);
                let load=match completion {
                    Some((address,payload))=>InstKind::ViewLoad{dst,address,index:payload,stride:ElementStride::Constant(1),word},
                    None=>InstKind::LoadLocal(dst,local.as_ref().unwrap().slots[3+component]),
                };
                builder.emit(load);
                dst
            }).collect::<Vec<_>>();
            let origins=regs.split_off(result.components.len());
            let origins=result.task_components().into_iter().zip(origins.into_iter().enumerate()).collect::<BTreeMap<_,_>>();
            let returned=regs.iter().enumerate().map(|(component,reg)|Some(match identity {
                Some(identity)=>ValueIdentity::TaskCompletion(identity,component),
                None=>ValueIdentity::Register(*reg),
            })).collect();
            let initialized=sources.task_projection(view::TaskProjection::Initialized(returned)).restricted(&builder.path);
            let fact=address::Initialization {
                source:address::InitializationSource::Task(initialized),
                when:bounds::Condition::constant(true),
            };
            builder.initialized.extend(address::InitializationEffect::Position(fact).into_positions(&builder.path));
            let mut proof=bounds::Proof::default();
            if let Some(identity)=identity {
                proof.origins=regs.iter().enumerate().map(|(component,reg)|
                    vec![ValueIdentity::Register(*reg),ValueIdentity::TaskCompletion(identity,component)]).collect();
            }
            for component in view_contracts::components(&result){
                let source=identity.zip(origins.get(&component)).map(|(input,(slot,_))|
                    ValueIdentity::TaskCompletion(input,result.components.len()+slot));
                let borrow=sources.task_projection(view::TaskProjection::Returned(component,source));
                let borrow=if let Some(&(_,origin))=origins.get(&component) {
                    builder.bind_work_task_return_sources(&borrow,component,origin,0,&|builder,word| {
                        if let Some((address,payload))=completion {
                            let marker=builder.reg(Scalar::U32);
                            builder.emit(InstKind::ViewLoad{
                                dst:marker,address,index:payload,stride:ElementStride::Constant(1),word,
                            });
                            marker
                        } else {
                            local.as_ref().unwrap().payload_source(builder,word)
                        }
                    })
                }else{borrow};
                proof.set_view_borrow(component,borrow);
            }
            let result=Value{ty:result,regs,proof,uniform:false,literal:false,constant:None};
            unsafe { builder.world.call(builder.complete_task_owned_places(&sources,|_,_|Some(result.clone()),None,
                async |builder,completions|match &local {
                    Some(output)=>unsafe { builder.world.call(output.complete_inputs(builder,completions)).await },
                    None=>Ok(()),
                },span)).await }?;
            let result = if result.ty.size == 0 { returns::Returned::Unit } else { unsafe { builder.world.call(builder.task_value(result, residency::Residency::task_transfer())).await }? };
            consume(builder, result).await
        };
        if self.pending_error.is_none() {
            return complete(self).await;
        }
        let zero=self.constant(Scalar::U64,0).regs[0];
        let condition=self.reg(Scalar::Bool);
        self.emit(InstKind::Binary {
            dst:condition,op:BinaryOp::Compare(Comparison::Equal),a:error,b:zero,
        });
        let success=Value {
            ty:Ty::scalar(Scalar::Bool),regs:vec![condition],
            proof:bounds::Proof::condition(bounds::Condition::guard(bounds::Clause::zero_results([error]))),
            uniform:false,literal:false,constant:None,
        };
        let mut outcome=None;
        unsafe { self.world.call(self.conditional_blocks(success,async |builder| {
            outcome=Some(complete(builder).await?);
            Ok(())
        },async |builder|match &local {
            Some(output) if output.has_owned_outputs()=>unsafe { builder.world.call(output.complete_if_ready(builder)).await },
            _=>Ok(()),
        },span)).await }?;
        Ok(outcome.unwrap())
    }

    async fn task_value(&mut self, mut value: Value, location: residency::Residency) -> Result<returns::Returned> {
        use returns::Returned;
        let mut value=if value.ty.owns_resources() {
            value.proof=bounds::Proof::capture(&value);
            unsafe { self.world.call(self.template_returned(value,true)).await }?
        } else { Returned::value(value) };
        residency::visit(&mut value,&mut residency::Input::new(0),
            &mut |data,_| data.residency=location.clone());
        Ok(value)
    }

    fn application_call(
        &mut self,
        name: &str,
        args: Vec<Value>,
        result: Option<Ty>,
        span: Span,
        generation: Option<Arc<GeneratedOrigin>>,
    ) -> Result<Option<Value>> {
        let frame = self.application_frame(span)?;
        let values: Vec<_> = result
            .iter()
            .flat_map(|ty| &ty.components)
            .map(|(s, _)| self.reg(*s))
            .collect();
        let arguments = std::iter::once(frame).chain(args).map(argument).collect();
        let error=self.reg(Scalar::U64);
        let kind = runtime_call(
            name,
            arguments,
            result.clone(),
            values.clone(),
            Some(error),
            self.failure_continuation(Vec::new()),
        ).kind;
        if let Some(generation) = generation {
            self.code.push(Inst::generated(kind, generation));
        } else {
            self.emit(kind);
        }
        Ok(result.map(|ty| Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty,
            regs: values,
            uniform: false,
            constant: None,
        }))
    }
    fn application_release(&self, handle: Reg) -> Inst {
        let frame = self.application.as_ref().unwrap().frame.clone().unwrap();
        runtime_call(
            "release",
            vec![
                CallArgument::Value { #[cfg(feature = "compiler")] origin: None, dataset_borrow:None, ty: pointer_type(), values: frame },
                CallArgument::Value { #[cfg(feature = "compiler")] origin: None,
                    dataset_borrow:None, ty: Ty::scalar(Scalar::U32),
                    values: vec![handle],
                },
            ],
            None,
            Vec::new(),
            None,
            None,
        )
    }

    pub(super) fn application_available(&self, data: &Data, span: Span) -> Result<()> {
        if !self.resolve_ownership(&data.ownership,None).available() {
            return self.fail(
                span,
                "B227",
                "dataset 尚未初始化或所有权已经移动，原绑定不能访问或再次转移",
            );
        }
        Ok(())
    }
    pub(super) fn application_handle_available(
        &self,
        identity: ValueIdentity,
        span: Span,
        resources: &[&Data],
        path:&bounds::Clause,
    ) -> Result<()> {
        let mut matching = self
            .env
            .values()
            .flat_map(|binding| match binding {
                Binding::ApplicationData(d) => vec![d],
                Binding::OwnedValue(v) => v.resources(),
                _ => Vec::new(),
            })
            .chain(resources.iter().copied())
            .filter(|d| d.root.proof.known_identity(0) == Some(identity)
                || self.owned_resource_borrow(d).restricted(path).resources().sources.contains(&view::Source::Data(identity)))
            .peekable();
        if matching.peek().is_some() && matching.all(|data|match &data.ownership {Ownership::Conditional(when)=>self.condition_truth(&when.condition(),Some(&bounds::Condition::guard(path.clone())))!=Some(true),ownership=>!ownership.available()}) {
            return self.fail(
                span,
                "B227",
                "dataset 在当前表达式中已移动，先前借用或元素位置不能继续消费",
            );
        }
        Ok(())
    }
    pub(super) async fn application_borrowed(&self,identity:ValueIdentity)->Result<bool>{Ok(unsafe { self.world.call(self.application_view_borrowed(identity)).await }?||self.application_position_borrowed(identity))}
    pub(super) fn application_position_borrowed(&self, identity: ValueIdentity) -> bool {
        self.env.iter().filter(|(name,_)|self.view_uses.used(name)).any(|(_,binding)| {
            let place = match binding {
                Binding::DataReference(reference) => return reference.sources.contains(&identity),
                Binding::Owned(owner) => &owner.place,
                Binding::AtomicBorrow(place) => place,
                _ => return false,
            };
            matches!(place.storage(), Place::ApplicationElement { sources, .. } if sources.contains(&identity))
        })
    }
    pub(super) fn application_movable(&mut self,data:&Data,retained:&view::Borrow,span:Span)->Result<()> {
        self.check_view_requirement(view::Requirement::Unborrowed{resource:self.owned_resource_borrow(data),borrow:retained.clone(),kind:view::Exclusion::Ownership,span})?;
        if self.application_position_borrowed(data.root.identity()) {
            return self.fail(
                span,
                "B227",
                "dataset 的元素仍被当前游标或位置借用，不能交接或覆盖整份所有权",
            );
        }
        Ok(())
    }
    fn application_root(&self, reg: Reg) -> Value {
        let mut value = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(self.registers[reg]),
            regs: vec![reg],
            uniform: false,
            constant: None,
        };
        value.proof = bounds::Proof::capture(&value);
        value
    }
    pub(super) async fn application_value(&mut self, name: &str, span: Span) -> Result<Option<Data>> {
        let Some(Binding::ApplicationData(mut data)) = self.env.get(name).cloned() else {
            return Ok(None);
        };
        data.ownership=self.resolve_ownership(&data.ownership,None);
        data.residency=self.resolve_owned_residency(&data.residency,None);
        self.application_available(&data, span)?;
        self.prepare_owned_data(&data);
        data = unsafe { self.world.call(self.refresh_application_data(data,span)).await }?;
        if let Some(Binding::ApplicationData(source)) = self.env.get_mut(name) {
            source.root.proof = data.root.proof.clone();
            source.length.proof = data.length.proof.clone();
        }
        Ok(Some(data))
    }
    pub(super) async fn refresh_application_data(&mut self,mut data:Data,span:Span)->Result<Data> {
        if let Some(place)=&data.place {
            let value=unsafe { self.world.call(self.read_storage_value(place,span, None)).await }?;data.root.regs=vec![value.regs[0]];data.length.regs=vec![value.regs[1]];
            data.root.proof=bounds::Proof::capture(&data.root);
            data.length.proof=bounds::Proof::capture(&data.length).with_domains(Domains::from([IndexDomain::ApplicationLength(data.root.identity())]));
            data.length.constant=data.length.proof.known_value(&data.length.ty);
        }
        Ok(data)
    }
    pub(super) fn application_bind(&mut self, mut data: Data, mutable: bool) -> Data {
        data.place = None;
        data.root.proof = bounds::Proof::capture(&data.root);
        data.length.proof = bounds::Proof::capture(&data.length);
        if mutable {
            let ty=Ty::owned_dataset((data.ty.clone()).syntax(),data.count);let(slots,storage)=self.local_slots(&ty);
            let origin=Arc::new(GeneratedOrigin{reason:GenerationReason::DatasetBindSlots,trigger:self.source_site.clone().map(GenerationAnchor::Syntax)});
            self.code.push(Inst::generated(InstKind::StoreLocal(slots[0],data.root.regs[0]),origin.clone()));
            self.code.push(Inst::generated(InstKind::StoreLocal(slots[1],data.length.regs[0]),origin));
            let place=Place::Local(ty,slots,storage);self.initialize(&place);data.place=Some(place);
        }
        data
    }
    pub(super) async fn release_application_data(&mut self,keep:Option<&BTreeSet<String>>,retained:view::Borrow)->Result<()> {
        unsafe { self.world.call(self.cleanup_group(Vec::new(),async |builder|{
        let mut names=builder.env.iter()
            .filter(|(name,_)|!keep.is_some_and(|names|names.contains(*name)))
            .filter_map(|(name,binding)|match binding {
                Binding::ApplicationData(data) if data.ownership.may_own()
                    || matches!(builder.pending_error, Some(PendingError::Exit(_))) && data.owns_write_elements()
                    => Some(name.clone()),
                Binding::OwnedValue(value) if value.ownership!=Ownership::Borrowed
                    && (matches!(builder.pending_error,Some(PendingError::Exit(_)))||!matches!(value.place(),Some(Place::Reference{..})))=>Some(name.clone()),
                _=>None,
            }).collect::<Vec<_>>();
        names.sort_by_key(|name|{
            let temporary=name.strip_prefix("\0argument:").map(|index|index.parse::<usize>().unwrap());
            let declaration=match &builder.env[name]{Binding::OwnedValue(value)=>value.declaration,_=>0};
            (temporary.is_some(),temporary.unwrap_or(declaration))
        });
        while let Some(name)=names.pop(){
            let binding=builder.env[&name].clone();
            let mut remaining=retained.clone();
            for future in &names {
                if future.starts_with('\0'){
                    unsafe { builder.world.call(user_owned::cleanup_borrow(builder,&builder.env[future],&mut remaining)).await }?;
                }
            }
            // Implicit waits can update another destination before its own cleanup.
            if let Binding::ApplicationData(data) = &binding && data.owns_write_elements() {
                unsafe { builder.world.call(builder.drop_application_elements(&DataReference::direct(data.clone()), &remaining, builder.scope.span())).await }?;
            } else {
                unsafe { builder.world.call(builder.drop_binding(binding,&remaining,builder.scope.span())).await }?;
            }
            if name.starts_with('\0'){
                builder.env.remove(&name);
            }else{
                match builder.env.get_mut(&name){
                    Some(Binding::OwnedValue(value))=>value.mark_moved(),
                    Some(Binding::ApplicationData(data))=>data.ownership=Ownership::Moved,
                    _=>{},
                }
            }
        }
        Ok(())
            })).await }
    }
    pub(super) fn application_data_type(
        &self,
        data: &mut Data,
        ty: &Ty,
        count: Option<u32>,
        span: Span,
    ) -> Result<()> {
        self.require_same(ty, &data.ty, span)?;
        if let Some(count) = count {
            if data.length.minimum() != count || data.length.maximum() != count {
                return self.fail(span, "B167", "dataset 实际长度未满足声明的固定形状");
            }
            data.count = Some(count);
        }
        Ok(())
    }
    pub(super) async fn application_declare(
        &mut self,
        annotation: &TypeName,
        name: &str,
        span: Span,
    ) -> Result<()> {
        self.application_frame(span)?;
        let scope = &self.scope;
        let ty = unsafe { self.world.call(self.world.ty(
            annotation,
            scope.source(),
            span,
            scope.module(),
            scope.imports(),
        )).await }?;
        let count = unsafe { self.world.call(self.world.shape(
            &annotation.count,
            scope.source(),
            span,
            scope.module(),
            scope.imports(),
        )).await }?;
        let slots = self.local_slots(&Ty::owned_dataset((ty.clone()).syntax(), count)).0;
        let handle = self.reg(Scalar::U32);
        let length = self.reg(Scalar::U32);
        // These are absent compile-time values. No native initialization or placeholder allocation is emitted.
        let value = |reg| Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![reg],
            uniform: false,
            constant: None,
        };
        let elements=ElementFacts::Unknown;
        self.env.insert(
            name.into(),
            Binding::ApplicationData(Box::new(DataPayload {
            prefixes: BTreeMap::new(),
                elements,
                root: value(handle),
                length: value(length),
                place: Some(Place::Local(Ty::owned_dataset((ty.clone()).syntax(),count),slots.to_vec(),slots[0]..slots[1]+1)),
                initialization: None,
                ty,
                count,
                ownership: Ownership::Moved,
                access: Access::InOut,
                residency: residency::Residency::default(),
                layout: DataLayout::Declared,
            })),
        );
        Ok(())
    }
    pub(super) async fn application_assign(
        &mut self,
        name: &str,
        target: Data,
        op: &str,
        rhs: &Expr,
        span: Span,
    ) -> Result<()> {
        if op != "=" {
            return self.fail(
                span,
                "B124",
                "dataset 赋值交接整份拥有资源，不支持数值复合运算",
            );
        }
        if target.ownership == Ownership::Borrowed {
            return self.fail(span, "B227", "借用的 dataset 不能替换其资源所有者");
        }
        let Some(mut value) = unsafe { self.world.call(self.application_resource(rhs, span)).await }? else {
            return Ok(());
        };
        self.application_data_type(&mut value, &target.ty, target.count, span)?;
        self.view_uses.after(span);
        if matches!(&self.env[name], Binding::ApplicationData(data) if data.ownership == Ownership::Owned)
        {
            let old = unsafe { self.world.call(self.application_value(name, span)).await }?.unwrap();
            unsafe { self.world.call(self.drop_returned(returns::Returned::Dataset(old),&view::Borrow::default(),span)).await }?;
        }
        let place=target.place.unwrap();let packed=self.returned_storage(&returns::Returned::Dataset(value.clone()));
        let origin=Arc::new(GeneratedOrigin{reason:GenerationReason::DatasetAssignSlots,trigger:self.source_site.clone().map(GenerationAnchor::Syntax)});
        unsafe { self.world.call(self.write_storage_value(&place,&packed,span,Some(origin))).await }?;value.place=Some(place);
        value.count = target.count;
        value.ownership = Ownership::Owned;
        value.access = Access::InOut;
        value.root.proof = bounds::Proof::capture(&value.root);
        value.length.proof = bounds::Proof::capture(&value.length);
        self.env
            .insert(name.into(), Binding::ApplicationData(value));
        Ok(())
    }
    #[inline(never)]
    pub(super) async fn application_resource(&mut self, expr: &Expr, span: Span) -> Result<Option<Data>> {
        match unsafe { self.world.call(self.evaluate_returned(expr, None)).await }? {
            Some(returns::Returned::Dataset(data)) => Ok(Some(data)),
            None => Ok(None),
            _ => self.fail(span, "B224", "dataset 资源需要同类型拥有值，不能由普通值或聚合整体替代"),
        }
    }
    #[inline(never)]
    pub(super) async fn move_application_dataset(&mut self, name: &str, span: Span) -> Result<Option<Data>> {
        self.application_frame(span)?;
        let Some(mut data) = unsafe { self.world.call(self.application_value(name, span)).await }? else {
            return self.fail(span, "B224", "dataset 移动需要应用拥有的实际资源");
        };
        if data.ownership != Ownership::Owned {
            return self.fail(span, "B227", "借用的 dataset 不能移动或作为拥有资源返回");
        }
        if self.application_position_borrowed(data.root.identity()) {
            return self.fail(span, "B227", "dataset的元素仍被游标或位置借用，不能移动拥有值");
        }
        if let Some(Binding::ApplicationData(source)) = self.env.get_mut(name) {
            source.ownership = Ownership::Moved;
            source.residency = residency::Residency::default();
        }
        data.place = None;
        Ok(Some(data))
    }
    #[inline(never)]
    pub(super) async fn resize_application_dataset(&mut self, args: &Expressions, span: Span) -> Result<Option<Data>> {
        if args.len() != 3 {
            return self.fail(span, "B266", "resize(data,length,fill) 消费拥有的 CPU dataset，以同型值初始化新增位置");
        }
        let Some(returned) = unsafe { self.world.call(self.evaluate_returned(&args.at(0), None)).await }? else { return Ok(None); };
        let returns::Returned::Dataset(data) = returned else {
            return self.fail(args.at(0).span(), "B224", "resize 需要一份拥有的 dataset");
        };
        let data = unsafe { self.world.call(self.refresh_application_data(data, span)).await }?;
        if data.ownership != Ownership::Owned {
            return self.fail(args.at(0).span(), "B227", "dataset 借用不拥有调整存储长度的权利");
        }
        unsafe { self.world.call(self.require_application_initialized(&data, span)).await }?;
        self.application_cpu(&data, span)?;
        let input=self.hold_binding(Binding::ApplicationData(data.clone()));
        let Some(length) = unsafe { self.world.call(self.expr(&args.at(1))).await }? else { return Ok(None); };
        self.require_same(&Ty::scalar(Scalar::U32), &length.ty, args.at(1).span())?;
        let Some(mut returned) = unsafe { self.world.call(self.evaluate_returned(&args.at(2), Some(&data.ty))).await }? else { return Ok(None); };
        unsafe { self.world.call(self.check_returned_type(&mut returned, &data.ty, args.at(2).span())).await }?;
        unsafe { self.world.call(self.require_returned_initialized(&mut returned, args.at(2).span())).await }?;
        let fill = self.returned_storage(&returned);
        if fill.ty.contains_views() {
            return self.fail(args.at(2).span(), "B224", "resize 填充值不能复制 view 借用图");
        }
        let pending=match &returned {
            returns::Returned::OwnedValue(value)=>Some(self.hold_binding(Binding::OwnedValue(value.clone()))),
            returns::Returned::Dataset(data)=>Some(self.hold_binding(Binding::ApplicationData(data.clone()))),
            _=>None,
        };
        let mut retained = fill.proof.typed_view_borrow(&fill.ty);
        self.view_uses.invocation(span);
        retained.union(&unsafe { self.world.call(self.view_uses.all(self,&[view_liveness::Transfer::OwnedData(&data)])).await }?);
        self.application_movable(&data, &retained, span)?;
        if fill.ty.owns_resources() {
            let exact = data.length.minimum() == data.length.maximum()
                && length.minimum() == length.maximum()
                && data.length.maximum().checked_add(1) == Some(length.minimum());
            let sources = data.length.component_origins(0);
            let offset = length.proof.domains(0).iter().copied().any(|domain|
                matches!(domain, IndexDomain::Offset(base, 1) if sources.contains(&base)));
            if !exact && !offset {
                return self.fail(args.at(1).span(), "B224", "拥有填充值只能移动到已证明不回绕且恰好新增的一项，不能广播复制或截短拥有元素");
            }
        }
        let incoming = self.capture_element_model(&data, &[], returned, None);
        let growth = bounds::Condition::comparison(Comparison::Greater, &length, &data.length);
        let grows = growth.truth_in(&self.path);
        let elements = if grows == Some(false) { data.elements } else if data.length.maximum() == 0 { ElementFacts::Known(incoming) } else if let ElementFacts::Known(mut previous)=data.elements {
            if let Some(value)=previous.view_value_mut(){
                let present=bounds::Condition::selected(ownership::ApplicationIndex::of(&data.length),0).negate();
                value.proof.under_path(&value.ty,&present.assume(&bounds::Clause::default(),true));
            }
            if grows == Some(true) {
                previous.join(*incoming,true,None);
            } else {
                let mut combined = previous.clone();
                combined.join(*incoming,true,None);
                previous.join(*combined,true,Some(&growth.negate()));
            }
            ElementFacts::Known(previous)
        }else{data.elements.common(ElementFacts::Known(incoming))};
        let (slots, storage) = self.local_slots(&fill.ty);
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetResizeFill,
            trigger: self.source_at(args.at(2).span(), SourceSyntax::Expression).map(GenerationAnchor::Syntax),
        });
        for (slot, reg) in slots.into_iter().zip(fill.regs.iter().copied()) {
            self.code.push(Inst::generated(InstKind::StoreLocal(slot, reg), origin.clone()));
        }
        let argument = CallArgument::Reference { #[cfg(feature = "compiler")] origin: None, ty: fill.ty.clone(), access: Access::Read, address: CallReference {
            storage: CallStorage::Local { base: storage.start, len: storage.len() }, offset: None, word: 0,
        } };
        let result=self.resize_application_storage(data.root,data.ty,length,elements,argument,&[&fill],span)?;
        if let Some(pending)=pending{self.env.remove(&pending);}
        self.env.remove(&input);
        Ok(Some(result))
    }
    fn resize_application_storage(&mut self,root:Value,ty:Ty,mut length:Value,elements:ElementFacts,fill:CallArgument,snapshots:&[&Value],span:Span)->Result<Data> {
        let frame = self.application_frame(span)?;
        let resource = root.proof.view_borrow(0);
        let reg = self.reg(Scalar::U32);
        let call_origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetResizeCall,
            trigger: self.source_at(span, SourceSyntax::Expression).map(GenerationAnchor::Syntax),
        });
        let error=self.reg(Scalar::U64);
        self.code.push(Inst::generated(runtime_call("resize", vec![
            argument(frame), argument(root), argument(length.clone()),
            fill,
        ], Some(Ty::scalar(Scalar::U32)), vec![reg],Some(error),self.failure_continuation(Vec::new())).kind, call_origin));
        // Frame 槽位及已有资源来源保持；新 SSA 身份仍区分旧长度／下标版本。
        let mut handle = self.application_root(reg);
        handle.proof.set_view_borrow(0, resource);
        length.literal = false;
        length.proof = bounds::Proof::capture(&length).with_domains(Domains::from([
            IndexDomain::ApplicationLength(handle.identity()),
        ]));
        let data = Box::new(DataPayload {
            prefixes: BTreeMap::new(),
            elements,
            root: handle, place: None, initialization: None, ty, length,
            ownership: Ownership::Owned, access: Access::InOut, count: None, residency: residency::Residency::cpu(), layout:DataLayout::Declared,
        });
        self.complete_application_elements(&DataReference::direct(data.clone()), data.elements.known(), snapshots, span, Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetResizeComplete,
            trigger: self.source_at(span, SourceSyntax::Expression).map(GenerationAnchor::Syntax),
        }))?;
        Ok(data)
    }
    pub(super) async fn take_last_application_dataset(&mut self,args:&Expressions,span:Span)->Result<Option<Data>> {
        if args.len()!=2{return self.fail(span,"B266","take_last(data,output)消费非空CPU dataset并交出唯一尾值");}
        let source=&args.at(0);
        let output=&args.at(1);
        let Some(returns::Returned::Dataset(data))=unsafe { self.world.call(self.evaluate_returned(source,None)).await }? else {
            if !self.flow.continues(){return Ok(None);}
            return self.fail(source.span(),"B224","take_last需要一份拥有的dataset");
        };
        let data=unsafe { self.world.call(self.refresh_application_data(data,span)).await }?;
        if data.ownership!=Ownership::Owned{return self.fail(source.span(),"B227","dataset借用不能交出尾元素或缩短存储");}
        unsafe { self.world.call(self.require_application_initialized(&data,span)).await }?;self.application_cpu(&data,span)?;
        let one=self.constant(Scalar::U32,1);let last=self.binary("-",data.length.clone(),one,span,None)?;
        if !self.index_in(&last,IndexDomain::ApplicationData(data.root.identity()),data.length.minimum(),None) {
            return self.fail(source.span(),"B137","take_last需要由原长度条件证明输入非空");
        }
        let held=self.hold_binding(Binding::ApplicationData(data.clone()));
        let destination=unsafe { self.world.call(self.mutable_place(output,Access::Write)).await }?;
        if !self.flow.continues(){return Ok(None);}
        let Some(destination)=destination else{return self.fail(output.span(),"B227","take_last输出需要同元素类型的可写位置");};
        self.require_same(&data.ty,destination.ty(),output.span())?;
        let place=Self::application_element(&data,&last,(last.minimum(),last.maximum()));
        let packed=unsafe { self.world.call(self.read_storage_value(&place,span,None)).await }?;
        let mut value=if data.ty.owns_resources(){unsafe { self.world.call(self.materialize_element(&DataReference::direct(data.clone()),Some(packed),&place)).await }?}else{returns::Returned::value(packed)};
        unsafe { self.world.call(self.require_returned_initialized(&mut value,span)).await }?;
        self.env.remove(&held);
        self.view_uses.invocation(span);let retained=unsafe { self.world.call(self.view_uses.all(self,&[])).await }?;
        self.application_movable(&data,&retained,span)?;
        if data.ty.owns_resources(){
            let old=if matches!(destination.storage(),Place::ApplicationElement{..}){unsafe { self.world.call(self.application_owned_argument(&destination, false, span)).await }?}else{self.owned_place_model(&destination)};
            self.view_uses.after(span);
            unsafe { self.world.call(self.store_owned_returned(destination,old,value,span)).await }?;
        }
        else {
            let packed=self.returned_storage(&value);unsafe { self.world.call(self.write_storage_value(&destination,&packed,span,None)).await }?;
            unsafe { self.world.call(self.assign_owned_proof(&destination,bounds::Proof::capture(&packed),packed.uniform)).await }?;self.initialize(&destination);
        }
        // A shrinking Frame never reads fill; the moved tail has no second owner there.
        let fill=argument(self.constant(Scalar::U64,0));
        self.resize_application_storage(data.root,data.ty,last,data.elements,fill,&[],span).map(Some)
    }
    pub(super) async fn dataset_constructor_type(world:&World,scope:&LexicalScope,explicit:&[GenericArgument],arguments:usize,span:Span)->Result<(Ty,Option<u32>)> {
        if !(1..=2).contains(&explicit.len())||!(1..=2).contains(&arguments) {
            return scope.source().fail(span,"B266","dataset<T>(length) 需要元素类型和一个 u32 长度");
        }
        let GenericArgument::Type(element) = &explicit[0] else {
            return scope.source().fail(span, "B266", "dataset<T> 需要元素类型");
        };
        if element.dataset {
            return scope.source().fail(span,"B232","资源包装不能直接嵌套；子资源通过记录字段表达");
        }
        if element.tile {
            return scope.source().fail(
                span,
                "B266",
                "dataset 的元素是普通值，不是另一份 dataset 或 tile 所有权",
            );
        }
        let ty = unsafe { world.call(world.ty(element,scope.source(),span,scope.module(),scope.imports())).await }?;
        let count = if let Some(argument) = explicit.get(1) {
            let Some(shape) = argument.shape() else {
                return scope.source().fail(span, "B266", "dataset<T,N> 的 N 需要静态 u32 形状");
            };
            unsafe { world.call(world.shape(
                &Some(shape),
                scope.source(),
                span,
                scope.module(),
                scope.imports(),
            )).await }?
        } else {
            None
        };
        Ok((ty,count))
    }
    #[inline(never)]
    pub(super) async fn create_application_dataset(&mut self, expr:&Expr, explicit:&[GenericArgument],args:&Expressions,span:Span)->Result<Option<Data>> {
        self.application_frame(span)?;
        let(ty,count)=unsafe { self.world.call(Self::dataset_constructor_type(self.world,&self.scope,explicit,args.len(),span)).await }?;
        let Some(mut length) = unsafe { self.world.call(self.expr(&args.at(0))).await }? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &length.ty, args.at(0).span())?;
        if (ty.owns_resources() || ty.contains_views()) && length.maximum() != 0 && args.len() != 2 {
            return self.fail(span, "B224", "资源或view元素的非空dataset构造需要静态生成函数逐个交付实际值");
        }
        length.literal = false;
        if count.is_some_and(|count| length.minimum() != count || length.maximum() != count) {
            return self.fail(
                span,
                "B266",
                "固定形状 dataset 的创建长度必须与声明的 const 形状相同",
            );
        }
        let resources = &mut self.functions.resources;
        let index = resources.types.iter().position(|item| *item == ty).unwrap_or_else(|| {
            let index = resources.types.len();
            resources.types.push(ty.clone());
            index
        });
        let handle = self
            .application_call(
                &format!("allocate_{index}"),
                vec![length.clone()],
                Some(Ty::scalar(Scalar::U32)),
                span,
                Some(Arc::new(GeneratedOrigin {
                    reason: GenerationReason::DatasetAllocate,
                    trigger: self.source_at(expr.span(), SourceSyntax::Expression).map(GenerationAnchor::Syntax),
                })),
            )?
            .unwrap();
        let handle = self.application_root(handle.regs[0]);
        length.proof = bounds::Proof::capture(&length).with_domains(Domains::from([
            IndexDomain::ApplicationLength(handle.identity()),
        ]));
        let elements=ElementFacts::Unknown;
        let mut data=Box::new(DataPayload {
            prefixes: BTreeMap::new(),
            elements,
            root: handle,
            place: None,
            initialization: if ty.zero_valid(){None}else{Some(Position::Parameter(self.application_output_scope(),0))},
            ty,
            length,
            count,
            ownership: Ownership::Owned,
            access: Access::InOut,
            residency: residency::Residency::cpu(),
            layout: DataLayout::Declared,
        });
        if let Some(factory)=args.get(1) {
            data.elements=match unsafe { self.world.call(self.initialize_application_elements(&data,&factory,span)).await }?{Some(model)=>ElementFacts::Known(model),None=>ElementFacts::Unknown};
            data.initialization=None;
        } else {
            let mut elements=unsafe { self.world.call(self.unknown_element_model(&data.ty,(&data.residency,&[]))).await }?;
            let value=elements.view_value_mut().unwrap();
            for component in view_contracts::components(&value.ty) {
                value.proof.set_view_borrow(component,view::Borrow::source(view::Source::Inactive));
            }
            data.elements=application::ElementFacts::Known(elements);
        }
        Ok(Some(data))
    }
    // Generate actual element storage with the original resource owner.
    async fn initialize_application_elements(&mut self,data:&Data,factory:&Expr,span:Span)->Result<Option<Box<returns::Returned>>> {
        let (name,explicit)=match factory.kind() {
            ExprKind::Name(name)=>(name,None),
            ExprKind::FunctionRef(name,args)=>(name,Some(args)),
            _=>return self.fail(factory.span(),"B224","dataset生成函数需要明确静态函数引用"),
        };
        let key=self.world.resolve(name,self.scope.module(),self.scope.imports(),true).unique()
            .ok_or_else(||Diagnostic{source:self.scope.source().clone(),span:factory.span(),code:"B158",message:format!("未知生成函数 {name}")})?;
        let trigger=self.source_at(factory.span(),SourceSyntax::Expression);
        let origin=|reason|Arc::new(GeneratedOrigin{reason,trigger:trigger.clone().map(GenerationAnchor::Syntax)});
        let cursor=origin(GenerationReason::DatasetInitializeCursor);
        let scalar=Ty::scalar(Scalar::U32);let zero=self.constant_with_origin(Scalar::U32,0,Some(cursor.clone()));let index_reg=self.reg(Scalar::U32);
        let mut proof=bounds::for_cursor(&zero,&data.length,false);
        for identity in data.length.component_origins(0) { proof.mark_domain(0,IndexDomain::Upper(identity,1)); }
        let index=Value{ty:scalar.clone(),regs:vec![index_reg],proof,uniform:false,literal:false,constant:None};
        let arguments=vec![Argument::Value(index.clone())];
        let explicit=match explicit.as_ref() {
            Some(arguments)=>Some(unsafe { self.world.call(self.scope.bindings().arguments(self.world,arguments,
                (self.scope.source(),self.scope.module(),self.scope.imports()),factory.span())).await }?),
            None=>None,
        };
        let instance=unsafe { self.world.call(self.specialized_arguments(key,&arguments,explicit.as_deref(),factory.span())).await }?;
        let function=&instance.definition.function;
        if function.kind()!=FunctionKind::Value || function.target()!=Target::Cpu || function.params().len()!=1 || function.params().at(0).access()!=Access::Value {
            return self.fail(factory.span(),"B224","dataset生成函数接收一个按值u32下标并返回一个元素，使用普通on cpu函数");
        }
        if data.length.maximum()==0 {
            let definition=&instance.definition;
            let parameter=unsafe { self.world.call(instance.parameter(self.world,function.params().at(0))).await }?;
            let input=unsafe { self.world.call(cooperative::parameter_type(self.world,&parameter,
                (&definition.source,&definition.module,&definition.imports,definition.function.kind()),self.program.target)).await }?;
            self.require_same(&scalar,&input,factory.span())?;
            let result_name=unsafe { self.world.call(instance.bindings.type_name(self.world,&function.result(),
                (&definition.source,&definition.module,&definition.imports),function.span())).await }?;
            let output=unsafe { self.world.call(self.world.ty(&result_name,&definition.source,function.span(),&definition.module,&definition.imports)).await }?;
            self.require_same(&data.ty,&output,factory.span())?;
            return Ok(None);
        }
        let slot=self.local_slots(&scalar).0[0];self.code.push(Inst::generated(InstKind::StoreLocal(slot,zero.regs[0]),cursor));
        let condition_origin=origin(GenerationReason::DatasetInitializeCondition);
        let outer=std::mem::take(&mut self.code);self.code.push(Inst::generated(InstKind::LoadLocal(index_reg,slot),condition_origin.clone()));
        let test=self.reg(Scalar::Bool);self.code.push(Inst::generated(InstKind::Binary{dst:test,op:ir::BinaryOp::Compare(ir::Comparison::Less),a:index_reg,b:data.length.regs[0]},condition_origin));
        let condition=std::mem::take(&mut self.code);
        // 构造拥有者的可结束范围就是原游标之前的初始化前缀；Frame仍持有完整分配。
        let mut prefix=data.clone();prefix.length=index.clone();prefix.count=None;prefix.initialization=None;
        let owner=self.hold_binding(Binding::ApplicationData(prefix));
        let Some(mut returned)=unsafe { self.world.call(self.invoke_returned(instance,arguments,span,Some(origin(GenerationReason::DatasetInitializeCall)),None)).await }? else{return self.fail(span,"B224","dataset生成函数必须交付一个实际元素");};
        unsafe { self.world.call(self.check_returned_type(&mut returned,&data.ty,span)).await }?;unsafe { self.world.call(self.require_returned_initialized(&mut returned,span)).await }?;
        let ordinal=index.identity();
        let mut model=returned.clone();
        residency::visit(&mut model,&mut residency::Input::new(0),&mut|resource,input|{
            let depth=input.fields.iter().filter(|part|matches!(part,residency::Part::Elements)).count();
            resource.residency=resource.residency.output_bound(&BTreeMap::from([(ordinal,ValueIdentity::ElementIndex(depth))]));
        });
        fn bind_indices(returned:&mut returns::Returned,ordinal:ValueIdentity,depth:usize){
            let bindings=BTreeMap::from([(ordinal,ValueIdentity::ElementIndex(depth))]);
            let value=match returned{returns::Returned::Value(value)=>Some(value.as_mut()),returns::Returned::OwnedValue(value)=>Some(&mut value.value),returns::Returned::Dataset(data)=>{data.length.proof=bounds::Proof::capture(&data.length).output_bound(&bindings);Some(&mut data.root)},returns::Returned::Unit=>None};
            if let Some(value)=value{value.proof=bounds::Proof::capture(value).output_bound(&bindings);}
            match returned{
                returns::Returned::OwnedValue(value)=>for child in value.payloads.iter_mut().flatten(){bind_indices(child,ordinal,depth);},
                returns::Returned::Dataset(data)=>if let Some(elements)=data.elements.known_mut(){bind_indices(elements,ordinal,depth+1);},
                _=>{},
            }
        }
        bind_indices(&mut model,ordinal,0);
        let elements=self.capture_element_model(data,&[],model,None);let value=self.returned_storage(&returned);
        if let Some(Binding::ApplicationData(prefix))=self.env.get_mut(&owner){prefix.elements=application::ElementFacts::Known(elements.clone());}
        // 原factory合同形成的模型交给此前捕获的同一前缀事实，不重建代码或调用图。
        native_function::calls(&mut self.code,&mut |call|{
            if let Some(FailureContinuation::Scope(scope))=&mut call.failure {
                if let Some(Binding::ApplicationData(prefix))=scope.env.get_mut(&owner){prefix.elements=application::ElementFacts::Known(elements.clone());}
            }
        });
        let pending=match &returned {
            returns::Returned::OwnedValue(value)=>Some(self.hold_binding(Binding::OwnedValue(value.clone()))),
            returns::Returned::Dataset(data)=>Some(self.hold_binding(Binding::ApplicationData(data.clone()))),
            _=>None,
        };
        let stride=DataStride::Declared(ir::align_up(data.ty.size,data.ty.align));let words=data.ty.components.iter().map(|(_,word)|*word).collect::<Vec<_>>();
        self.write_application_place(data.root.regs[0],data.root.identity(),index_reg,stride,&words,value,span,Some(origin(GenerationReason::DatasetInitializeWrite)))?;
        if let Some(pending)=pending{self.env.remove(&pending);}
        let advance=origin(GenerationReason::DatasetInitializeAdvance);
        let one=self.constant_with_origin(Scalar::U32,1,Some(advance.clone())).regs[0];let next=self.reg(Scalar::U32);self.code.push(Inst::generated(InstKind::Binary{dst:next,op:ir::BinaryOp::IAdd,a:index_reg,b:one},advance.clone()));self.code.push(Inst::generated(InstKind::StoreLocal(slot,next),advance));
        let body=std::mem::replace(&mut self.code,outer);self.code.push(Inst::generated(InstKind::Loop{condition,test,body},origin(GenerationReason::DatasetInitializeLoop)));
        self.complete_application_elements(&DataReference::direct(data.clone()), Some(elements.as_ref()), &[], span, origin(GenerationReason::DatasetInitializeComplete))?;
        self.env.remove(&owner);
        Ok(Some(elements))
    }
    pub(super) async fn complete_application_writes(&mut self, written: impl Fn(&Self,&Data) -> bool, span: Span) -> Result<()> {
        let path = self.residency_path();
        let eligible = |data: &Data| {
            data.root.ty == Ty::scalar(Scalar::U32)
                && data.elements.known().is_some()
                && self.resolve_ownership(&data.ownership, None).available()
                && self.residency.known_at(&data.residency, Target::Cpu, &path)
                && written(self, data)
        };
        let mut completed = BTreeSet::new();
        let mut writes = Vec::new();
        for (name, binding) in &self.env {
            if let Binding::DataReference(reference) = binding {
                if eligible(&reference.data) && completed.insert(reference.root.identity()) {
                    writes.push((name.clone(), Vec::new(), reference.data.clone()));
                }
                continue;
            }
            view::for_each_data_path(binding, |fields, resource, _| {
                if eligible(resource) && completed.insert(resource.root.identity()) {
                    writes.push((name.clone(), fields.to_vec(), resource.clone()));
                }
            });
        }
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetWriteComplete,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        for (name, fields, mut data) in writes {
            if let Binding::OwnedValue(owner) = &self.env[&name] {
                let mut resource = returns::Returned::OwnedValue(owner.clone());
                for field in fields {
                    let returns::Returned::OwnedValue(mut owner) = resource else { unreachable!() };
                    resource = if owner.value.ty.has_variants() {
                        unsafe { self.world.call(self.load_owned_value(&mut owner, span)).await }?;
                        unsafe { self.world.call(self.unpack_variant(&owner, field)).await }?
                    } else {
                        self.unpack_record(&owner, field)
                    };
                }
                let returns::Returned::Dataset(projected) = resource else { unreachable!() };
                data.place = projected.place;
                data.root.regs = projected.root.regs;
                data.length.regs = projected.length.regs;
            }
            let data = unsafe { self.world.call(self.refresh_application_data(data, span)).await }?;
            self.complete_application_elements(&DataReference::direct(data.clone()), data.elements.known(), &[], span, origin.clone())?;
        }
        Ok(())
    }
    pub(super) fn complete_application_elements(
        &mut self, data: &DataReference, elements: Option<&returns::Returned>, snapshots: &[&Value], span: Span,
        origin: Arc<GeneratedOrigin>,
    ) -> Result<()> {
        let Some(elements)=elements else{return Ok(());};
        if data.root.ty.components[0].0 != Scalar::U32 { return Ok(()); }
        let ty = data.element_layout().1;
        let model = self.returned_storage(elements);
        let mut ranges = model.proof.element_ranges(0, ty).into_iter()
            .map(|range| (range.offset, range)).collect::<BTreeMap<_, _>>();
        let mut candidates = vec![data.length.clone()];
        for &source in model.proof.origins.iter().flatten() {
            if let ValueIdentity::Register(reg) = source && self.registers[reg] == Scalar::U32 {
                candidates.push(Value { ty: Ty::scalar(Scalar::U32), regs: vec![reg],
                    proof: bounds::Proof::default().with_identity(source),
                    uniform: false, literal: false, constant: None });
            }
        }
        for binding in self.env.values() {
            if let Binding::Value(value) = binding {
                for (component, &(scalar, _)) in value.ty.components.iter().enumerate() {
                    if scalar == Scalar::U32 { candidates.push(self.value_component(value, component)); }
                }
            }
            candidates.extend(binding.application_resources().into_iter().map(|resource| resource.length.clone()));
        }
        for value in snapshots {
            for (component, &(scalar, _)) in value.ty.components.iter().enumerate() {
                if scalar == Scalar::U32 { candidates.push(self.value_component(value, component)); }
            }
        }
        let mut parameters: Vec<Value> = Vec::new();
        let mut fields = Vec::new();
        for (component, &(scalar, offset)) in ty.components.iter().enumerate() {
            let mut limits = BTreeSet::new();
            if scalar == Scalar::U32 {
                let domains = model.proof.domains(component);
                let origins = model.component_origins(component);
                for length in &candidates {
                    let sources = length.component_origins(0).into_iter().filter(|id| !matches!(id,
                        ValueIdentity::ElementComponent(_) | ValueIdentity::ElementParent(..)
                            | ValueIdentity::ElementIndex(_) | ValueIdentity::DatasetContent(..))).collect::<Vec<_>>();
                    let strict = Proof::below_length((&origins, &domains), (length.proof.origins.first().map(Vec::as_slice).unwrap_or_default(), &length.proof.domains(0)), true)
                        || domains.contains(&IndexDomain::ApplicationData(data.root.identity()))
                            && length.regs == data.length.regs;
                    let same = sources.iter().any(|source| origins.contains(source));
                    let distance = domains.iter().filter_map(|domain| match domain {
                        IndexDomain::Upper(limit, distance) if sources.contains(limit) => Some(*distance),
                        _ => None,
                    }).chain(strict.then_some(1)).chain(same.then_some(0)).max();
                    if let Some(distance) = distance {
                        let parameter = parameters.iter().position(|value| value.regs == length.regs).unwrap_or_else(|| {
                            let index = parameters.len(); parameters.push(length.clone()); index
                        });
                        limits.insert((parameter, distance));
                    }
                }
            }
            let range = ranges.remove(&offset);
            let known_bits = range.as_ref().map_or((0,0), |range| range.known_bits);
            let intervals = range.map(|range| range.intervals);
            if intervals.is_some() || !limits.is_empty() {
                fields.push(crate::application::ElementCompletion {
                    offset, intervals: intervals.unwrap_or_else(|| vec![(0, scalar.bit_mask())]), known_bits,
                    limits: limits.into_iter().collect(),
                });
            }
        }
        if fields.is_empty() { return Ok(()); }
        let completions = &mut self.functions.resources.completions;
        let index = completions.iter().position(|item| *item == fields).unwrap_or_else(|| {
            let index = completions.len(); completions.push(fields); index
        });
        let frame = self.application_frame(span)?;
        let offset = self.application_reference_offset(data);
        let mut arguments = vec![argument(frame), argument(data.root.clone()), argument(offset)];
        arguments.extend(parameters.into_iter().map(argument));
        self.code.push(Inst::generated(runtime_call(&format!("complete_{index}"), arguments,
            None, Vec::new(), None, None).kind, origin));
        Ok(())
    }
    pub(super) async fn retire_application_elements(&mut self, data: &DataReference, span: Span) -> Result<()> {
        let ty = data.element_layout().1;
        let mut ended = unsafe { self.world.call(self.unknown_element_model(ty, (&data.residency, &data.projection))).await }?;
        ended.restrict_owned(&owned_presence::Presence::None);
        unsafe { self.world.call(self.apply_application_dataset_model(data, ElementFacts::Known(ended))).await }?;
        unsafe { self.world.call(self.cleanup_group(Vec::new(), async |builder| unsafe { builder.world.call(builder.drop_application_elements(data, &view::Borrow::default(), span)).await })).await }
    }

    pub(super) async fn drop_task_completion_dataset(
        &mut self, ty: &Ty, descriptor: &[Reg], guards: &[owned_presence::Presence], span: Span,
    ) -> Result<()> {
        let root = self.application_root(descriptor[0]);
        let residency = residency::Residency::cpu();
        let mut model = unsafe { self.world.call(self.unknown_element_model(ty, (&residency, &[]))).await }?;
        owned_borrows::each_data(&mut model, &mut Vec::new(), &mut |data, _| data.residency = residency.clone());
        outputs::apply_ownership(&mut model, ty, 0, guards);
        let data = DataReference::direct(Box::new(DataPayload {
            root, place: None, initialization: None, ty: ty.clone(),
            length: self.application_root(descriptor[1]), ownership: Ownership::Borrowed,
            access: Access::InOut, count: None, residency,
            layout: DataLayout::Parameter {
                parameter: 0, stride: self.application_root(descriptor[2]), offset: self.application_root(descriptor[3]),
            },
            elements: ElementFacts::Known(model), prefixes: BTreeMap::new(),
        }));
        unsafe { self.world.call(self.drop_application_elements_with(&data, span, async |builder, data, place| {
            let mut element = unsafe { builder.world.call(builder.materialize_element(data, None, &place)).await }?;
            owned_borrows::set_storage(&mut element, place.clone());
            unsafe { builder.world.call(builder.drop_owned_tree_with(element, &view::Borrow::default(), span, Self::drop_task_owned_dataset)).await }?;
            let status = builder.reg(Scalar::U64);
            builder.emit(InstKind::LoadLocal(status, builder.pending_error.unwrap().slot()));
            let arguments = builder.error_position_arguments(&place, status, span)?;
            builder.emit(runtime_call("error_consume", arguments, None, Vec::new(), None, None).kind);
            Ok(())
        })).await }
    }

    pub(super) async fn application_resource_storage(&self,input:&Data,resource:&view::Borrow)->Result<Option<Vec<(effects::StorageWrite,Ty,DataLayout)>>> {unsafe { self.world.call(ApplicationLoads(self).resource_storage(input,resource)).await }}
    pub(super) fn application_element(
        data: &Data,
        index: &Value,
        index_range: (u32, u32),
    ) -> Place {
        let position = ownership::ApplicationIndex::of(index);
        let index_value = match position {
            ownership::ApplicationIndex::Constant(at) => bounds::Operand::Constant(Scalar::U32, Bits::from(at)),
            ownership::ApplicationIndex::Value(id, _) => bounds::Operand::Value(id),
        };
        Place::ApplicationElement {
            ty: data.ty.clone(),
            handle: data.root.regs[0],
            identity: data.root.identity(),
            sources: vec![data.root.identity()],
            resource: view::resource_borrow(data),
            initialization: data
                .initialization.clone()
                .map(|scope| (scope, position)),
            index: index.regs[0],
            index_value,
            parent: data.place.clone().map(Box::new),
            element_layout: address::Layout::of(&data.ty),
            index_range,
            stride: match &data.layout {DataLayout::Declared=>DataStride::Declared(ir::align_up(data.ty.size,data.ty.align)),DataLayout::Parameter{stride,offset,..}=>DataStride::Parameter{stride:stride.regs[0],offset:offset.regs[0]}},
            words: data
                .ty
                .components
                .iter()
                .map(|(_, offset)| *offset)
                .collect(),
            access: data.access,
        }
    }
    pub(super) async fn application_index(
        &mut self,
        data: DataReference,
        index: &Expr,
    ) -> Result<Option<Place>> {
        self.application_cpu(&data, index.span())?;
        let held = self.hold_binding(Binding::DataReference(data.clone()));
        let value = unsafe { self.world.call(self.expr(index)).await }?;
        self.env.remove(&held);
        let Some(mut value) = value else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, index.span())?;
        if !self.index_in(
            &value,
            IndexDomain::ApplicationData(data.root.identity()),
            data.length.minimum(),
            None,
        ) {
            return self.fail(
                index.span(),
                "B137",
                "dataset 索引需要由本资源的长度条件或有界游标证明",
            );
        }
        if data.element_layout().1.owns_resources() {
            self.observe_bounds(&value);
            if value.proof.domains(0).iter().any(|domain| matches!(domain, IndexDomain::Computed(_))) {
                let scope = self.path_proof(&value.ty, bounds::Proof::capture(&value));
                let identity = scope.origins.first().into_iter().flatten().copied()
                    .fold(value.identity(), std::cmp::min);
                value.proof = std::mem::take(&mut value.proof).with_identity(identity);
            }
        }
        Ok(Some(data.element(&value, (value.minimum(), value.maximum()))))
    }
    fn application_cpu_source(&mut self,identity:ValueIdentity,span:Span)->Result<()> {
        if let Some(data) = self
            .env
            .values()
            .find_map(|binding| match binding {
                Binding::ApplicationData(data)
                    if data.root.proof.known_identity(0) == Some(identity)
                        && data.ownership != Ownership::Moved =>
                {
                    Some(data)
                }
                _ => None,
            })
            .cloned()
        {
            self.application_cpu(&data, span)?;
        }
        Ok(())
    }
    fn application_root_address(&mut self,root:Reg,identity:ValueIdentity,span:Span)->Result<[Reg;2]> {
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetRootAddress,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        if self.registers[root] == Scalar::U64 {
            self.application_cpu_source(identity,span)?;
            let shift=self.constant_with_origin(Scalar::U64,32,Some(origin.clone())).regs[0];let high=self.reg(Scalar::U64);
            self.code.push(Inst::generated(InstKind::Binary{dst:high,op:BinaryOp::ShiftRightLogical,a:root,b:shift},origin.clone()));
            let address=[self.reg(Scalar::U32),self.reg(Scalar::U32)];
            self.code.push(Inst::generated(InstKind::Cast{dst:address[0],src:root},origin.clone()));
            self.code.push(Inst::generated(InstKind::Cast{dst:address[1],src:high},origin));
            Ok(address)
        } else {
            let zero=self.constant_with_origin(Scalar::U64,0,Some(origin.clone()));
            let stride=self.constant_with_origin(Scalar::U64,0,Some(origin.clone()));let count=self.constant_with_origin(Scalar::U32,1,Some(origin));
            self.application_address(root,identity,zero,0,stride,count,span)
        }
    }
    pub(super) fn application_address(
        &mut self,
        handle: Reg,
        identity: ValueIdentity,
        offset: Value,
        bytes: u32,
        stride: Value,
        count: Value,
        span: Span,
    ) -> Result<[Reg; 2]> {
        self.application_cpu_source(identity,span)?;
        let handle = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![handle],
            uniform: false,
            constant: None,
        };
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetAddress,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        let bytes = self.constant_with_origin(Scalar::U32, Bits::from(bytes), Some(origin.clone()));
        let frame=self.application_frame(span)?;
        let address=[self.reg(Scalar::U32),self.reg(Scalar::U32)];
        self.code.push(Inst::generated(runtime_call("address",
            vec![frame,handle,offset,bytes,stride,count].into_iter().map(argument).collect(),
            Some(pointer_type()),address.to_vec(),None,None).kind,origin));
        Ok(address)
    }
    pub(super) fn require_application_reference_initialized(&mut self,data:&DataReference,span:Span)->Result<()>{
        let(offset,ty)=data.element_layout();
        if !ty.components.iter().all(|(_,word)|self.application_field_initialized(&data.data,offset+word))
            && !self.defer_position_initialization(ty.components.iter().map(|(_,word)|data.data.initialization.as_ref().unwrap().application_word(None,offset+word)).collect(),"B142",span)? {
            return self.fail(span,"B142","读取了尚未完整初始化的数据集字段");
        }
        Ok(())
    }
    fn application_reference_offset(&mut self,data:&DataReference)->Value {
        let(field,_)=data.element_layout();
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetFieldLayout,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        let offset=match &data.data.layout {
            DataLayout::Declared=>self.constant_with_origin(Scalar::U32,0,Some(origin.clone())),
            DataLayout::Parameter{offset,..}=>offset.clone(),
        };
        if field==0{return offset;}
        let number=self.constant_with_origin(Scalar::U32,Bits::from(field),Some(origin.clone()));let dst=self.reg(Scalar::U32);
        self.code.push(Inst::generated(InstKind::Binary{dst,op:BinaryOp::IAdd,a:offset.regs[0],b:number.regs[0]},origin));
        Value{ty:Ty::scalar(Scalar::U32),regs:vec![dst],proof:bounds::Proof::default(),literal:false,uniform:offset.uniform,constant:None}
    }
    pub(super) fn application_reference_layout(&mut self,data:&DataReference)->(Value,Value) {
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetFieldLayout,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        let stride=match &data.data.layout {
            DataLayout::Declared=>self.constant_with_origin(Scalar::U32,Bits::from(ir::align_up(data.data.ty.size,data.data.ty.align)),Some(origin)),
            DataLayout::Parameter{stride,..}=>stride.clone(),
        };
        (stride,self.application_reference_offset(data))
    }
    pub(super) fn domain_dataset_parameter_type()->Ty{
        let mut ty=Ty::vector(Scalar::U32,2);ty.name="__blink_domain_dataset".into();ty
    }
    fn application_domain_argument(&mut self,data:&DataReference)->CallArgument{
        let offset=self.application_reference_offset(data);
        CallArgument::Value{ #[cfg(feature = "compiler")] origin: None,dataset_borrow:None, ty:Self::domain_dataset_parameter_type(),values:vec![data.root.regs[0],offset.regs[0]]}
    }
    pub(super) fn borrowed_dataset_parameter_type(access: Access, execution: crate::execution::Requirement)->Ty {
        if matches!(access,Access::Read|Access::Write|Access::Atomic|Access::InOut) && execution == crate::execution::Requirement::Invocation {
            let mut ty = Ty::scalar(Scalar::U64);
            ty.name = "__blink_address_dataset".into();
            ty.components = vec![(Scalar::U64,0),(Scalar::U32,8),(Scalar::U64,16),(Scalar::U32,24)];
            ty.size = 32; ty.align = 8;
            ty
        } else {
            let mut ty=Ty::vector(Scalar::U32,4);ty.name="__blink_borrowed_dataset".into();ty
        }
    }
    pub(super) fn application_reference_value(&mut self,data:&DataReference,ty:Ty,span:Span)->Result<Value> {
        let(mut stride,offset)=self.application_reference_layout(data);
        let root = if ty.components[0].0 == Scalar::U64 {
            if data.root.ty.components[0].0 == Scalar::U64 { data.root.clone() }
            else {
                let origin = Arc::new(GeneratedOrigin {
                    reason: GenerationReason::DatasetStorageAddress,
                    trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
                });
                let frame=self.application_frame(span)?;
                let reg=self.reg(Scalar::U64);
                let ty=Ty::scalar(Scalar::U64);
                self.code.push(Inst::generated(runtime_call("storage_address",vec![argument(frame),argument(data.root.clone())],Some(ty.clone()),vec![reg],None,None).kind,origin));
                let mut value=Value{ty,regs:vec![reg],proof:bounds::Proof::default(),literal:false,uniform:false,constant:None};
                value.proof.set_view_borrow(0,self.data_value_borrow(&data.data));
                value
            }
        } else { data.root.clone() };
        if ty.components[2].0 == Scalar::U64 && stride.ty.components[0].0 != Scalar::U64 {
            let origin = Arc::new(GeneratedOrigin {
                reason: GenerationReason::DatasetBorrowStrideCast,
                trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
            });
            let dst=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Cast{dst,src:stride.regs[0]},origin));
            stride=Value{ty:Ty::scalar(Scalar::U64),regs:vec![dst],proof:stride.proof,literal:false,uniform:stride.uniform,constant:stride.constant};
        }
        let mut root=root;
        root.proof.set_view_borrow(0,view::Borrow::projection(self.data_value_borrow(&data.data),data.projection.clone()));
        let mut proof=bounds::Proof::default();
        for value in [&root,&data.length,&stride,&offset]{proof.append(value);}
        Ok(Value{ty,regs:vec![root.regs[0],data.length.regs[0],stride.regs[0],offset.regs[0]],proof,literal:false,uniform:false,constant:None})
    }
    pub(super) fn application_stride_parts(&mut self,stride:&Value)->[Reg;2]{
        let origin=Arc::new(GeneratedOrigin{reason:GenerationReason::DatasetBorrowStrideCast,trigger:self.source_site.clone().map(GenerationAnchor::Syntax)});
        let wide=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Cast{dst:wide,src:stride.regs[0]},origin.clone()));
        let width=self.constant_with_origin(Scalar::U64,32,Some(origin.clone())).regs[0];
        let high=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Binary{dst:high,op:BinaryOp::ShiftRightLogical,a:wide,b:width},origin.clone()));
        let parts=[self.reg(Scalar::U32),self.reg(Scalar::U32)];
        self.code.push(Inst::generated(InstKind::Cast{dst:parts[0],src:wide},origin.clone()));
        self.code.push(Inst::generated(InstKind::Cast{dst:parts[1],src:high},origin));parts
    }
    pub(super) fn offset_application_address(&mut self,address:[Reg;2],index:Reg,stride:Reg,offset:Reg)->[Reg;2]{
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetOffsetAddress,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        let cast=|builder:&mut Self,src|{let dst=builder.reg(Scalar::U64);builder.code.push(Inst::generated(InstKind::Cast{dst,src},origin.clone()));dst};
        let low=cast(self,address[0]);let high=cast(self,address[1]);
        let width=self.constant_with_origin(Scalar::U64,32,Some(origin.clone())).regs[0];
        let shifted=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Binary{dst:shifted,op:BinaryOp::ShiftLeft,a:high,b:width},origin.clone()));
        let base=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Binary{dst:base,op:BinaryOp::Or,a:low,b:shifted},origin.clone()));
        let index=cast(self,index);let stride=cast(self,stride);let offset=cast(self,offset);
        let scaled=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Binary{dst:scaled,op:BinaryOp::IMul,a:index,b:stride},origin.clone()));
        let at=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Binary{dst:at,op:BinaryOp::IAdd,a:scaled,b:offset},origin.clone()));
        let pointer=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Binary{dst:pointer,op:BinaryOp::IAdd,a:base,b:at},origin.clone()));
        let high=self.reg(Scalar::U64);self.code.push(Inst::generated(InstKind::Binary{dst:high,op:BinaryOp::ShiftRightLogical,a:pointer,b:width},origin.clone()));
        let result=[self.reg(Scalar::U32),self.reg(Scalar::U32)];
        self.code.push(Inst::generated(InstKind::Cast{dst:result[0],src:pointer},origin.clone()));self.code.push(Inst::generated(InstKind::Cast{dst:result[1],src:high},origin));result
    }
    pub(super) fn application_memory(&mut self,handle:Reg,identity:ValueIdentity,index:Reg,stride:DataStride,write:bool,relative:Option<Reg>,components:&[(Scalar,u32)],span:Span)->Result<([Reg;2],Reg,u32)>{
        let layout_origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetAccessLayout,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        if self.registers[handle] == Scalar::U64 {
            let address=self.application_root_address(handle,identity,span)?;
            let (step,field)=match stride {
                DataStride::Declared(step)=>(self.constant_with_origin(Scalar::U64,Bits::from(step),Some(layout_origin.clone())).regs[0],self.constant_with_origin(Scalar::U32,0,Some(layout_origin.clone())).regs[0]),
                DataStride::Parameter{stride,offset}=>(stride,offset),
            };
            let address=self.offset_application_address(address,index,step,field);
            return Ok((address,self.constant_with_origin(Scalar::U32,0,Some(layout_origin)).regs[0],0));
        }
        let address = if write {
            let origin = Arc::new(GeneratedOrigin {
                reason: GenerationReason::DatasetWriteAddress,
                trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
            });
            let (step, field) = match stride {
                DataStride::Declared(step) => (self.constant_with_origin(Scalar::U32, Bits::from(step), Some(origin.clone())).regs[0], self.constant_with_origin(Scalar::U32, 0, Some(origin.clone())).regs[0]),
                DataStride::Parameter { stride, offset } => (stride, offset),
            };
            let widen = |builder: &mut Self, src| {
                let dst = builder.reg(Scalar::U64);
                builder.code.push(Inst::generated(InstKind::Cast { dst, src }, origin.clone()));
                dst
            };
            let row = widen(self, index);
            let step = widen(self, step);
            let field = widen(self, field);
            let scaled = self.reg(Scalar::U64);
            self.code.push(Inst::generated(InstKind::Binary { dst: scaled, op: BinaryOp::IMul, a: row, b: step }, origin.clone()));
            let mut start = self.reg(Scalar::U64);
            self.code.push(Inst::generated(InstKind::Binary { dst: start, op: BinaryOp::IAdd, a: scaled, b: field }, origin.clone()));
            if let Some(relative) = relative {
                let relative = widen(self, relative);
                let dst = self.reg(Scalar::U64);
                self.code.push(Inst::generated(InstKind::Binary { dst, op: BinaryOp::IAdd, a: start, b: relative }, origin.clone()));
                start = dst;
            }
            let mut address = None;
            for &(scalar, word) in components {
                let word = self.constant_with_origin(Scalar::U64, Bits::from(word), Some(origin.clone())).regs[0];
                let dst = self.reg(Scalar::U64);
                self.code.push(Inst::generated(InstKind::Binary { dst, op: BinaryOp::IAdd, a: start, b: word }, origin.clone()));
                let offset = Value { ty: Ty::scalar(Scalar::U64), regs: vec![dst], proof: bounds::Proof::default(), literal: false, uniform: false, constant: None };
                let stride=self.constant_with_origin(Scalar::U64,0,Some(origin.clone()));let count=self.constant_with_origin(Scalar::U32,1,Some(origin.clone()));
                address = Some(self.application_address(handle, identity, offset, scalar.size(), stride, count, span)?);
            }
            address.unwrap()
        } else {
            let zero = self.constant_with_origin(Scalar::U64, 0, Some(layout_origin.clone()));
            let stride=self.constant_with_origin(Scalar::U64,0,Some(layout_origin.clone()));let count=self.constant_with_origin(Scalar::U32,1,Some(layout_origin.clone()));
            self.application_address(handle, identity, zero, 0, stride, count, span)?
        };
        Ok(match stride {
            DataStride::Declared(stride)=>(address,index,stride),
            DataStride::Parameter{stride,offset}=>{
                let address=self.offset_application_address(address,index,stride,offset);
                (address,self.constant_with_origin(Scalar::U32,0,Some(layout_origin)).regs[0],0)
            }
        })
    }
    pub(super) fn application_projection_address(&mut self,data:&DataReference,span:Span)->Result<[Reg;2]>{
        let address=self.application_root_address(data.root.regs[0],data.root.identity(),span)?;
        let(_,offset)=self.application_reference_layout(data);
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetProjectionAddress,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        let zero=self.constant_with_origin(Scalar::U32,0,Some(origin)).regs[0];
        Ok(self.offset_application_address(address,zero,zero,offset.regs[0]))
    }
    pub(super) fn read_application_place(
        &mut self,
        ty: &Ty,
        handle: Reg,
        identity: ValueIdentity,
        index: Reg,
        stride: DataStride,
        words: &[u32],
        span: Span,
        element_read_origin: Option<Arc<GeneratedOrigin>>,
    ) -> Result<Value> {
        let (address,index,stride) = self.application_memory(handle,identity,index,stride,false,None,&[],span)?;
        let mut regs = Vec::new();
        for (&(scalar, _), &word) in ty.components.iter().zip(words) {
            let dst = self.reg(scalar);
            let kind=InstKind::ViewLoad{dst,address,index,stride:ElementStride::Constant(u64::from(stride)),word};
            if let Some(origin)=&element_read_origin {self.code.push(Inst::generated(kind,origin.clone()));}
            else {self.emit(kind);}
            regs.push(dst);
        }
        Ok(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: ty.clone(),
            regs,
            uniform: false,
            constant: None,
        })
    }
    pub(super) fn write_application_place(
        &mut self,
        handle: Reg,
        identity: ValueIdentity,
        index: Reg,
        stride: DataStride,
        words: &[u32],
        value: Value,
        span: Span,
        generation: Option<Arc<GeneratedOrigin>>,
    ) -> Result<()> {
        if words.iter().copied().collect::<BTreeSet<_>>().len() != words.len() {
            return self.fail(span, "B143", "赋值位置包含重复分量");
        }
        let zero = self.constant_with_origin(Scalar::U32, 0, generation.clone()).regs[0];
        for ((&(scalar, _), &word), value) in value.ty.components.iter().zip(words).zip(value.regs) {
            let (address,index,stride) = self.application_memory(handle,identity,index,stride,true,None,&[(scalar,word)],span)?;
            let kind = InstKind::StoreIndexed {
                source: IndexedSource::View {
                    address,
                    index,
                    stride:ElementStride::Constant(u64::from(stride)),
                },
                offset: zero,
                word,
                value,
            };
            if let Some(origin) = &generation { self.code.push(Inst::generated(kind, origin.clone())); }
            else { self.emit(kind); }
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) async fn target_call(
        &mut self,
        target: Target,
        call: &Expr,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        if !self.application_entry() {
            return self.fail(
                span,
                "B266",
                "target 调用由 CPU 主流程发起，元素实例与协作成员不能选择发起位置",
            );
        }
        let (name, explicit, args) = match call.kind() {
            ExprKind::Call(name, args) => (name, None, args),
            ExprKind::Specialize(name, explicit, args) => (name, Some(explicit), args),
            _ => {
                return self.fail(
                    span,
                    "B266",
                    "target 包围一次实际函数调用，例如 target(gpu, step(data))",
                );
            }
        };
        if target == Target::Cpu {
            return unsafe { self.world.call(self.evaluate_returned(call, None)).await };
        }
        let key = self
            .world
            .resolve(
                name,
                self.scope.module(),
                self.scope.imports(),
                true,
            ).unique()
            .ok_or_else(|| Diagnostic {
                source: self.scope.source().clone(),
                span,
                code: "B158",
                message: format!("未知或未导出的函数 {name}"),
            })?;
        if self.world.functions[&key].execution != crate::execution::Requirement::Domain {
            return self.fail(
                span,
                "B266",
                "当前 GPU target 消费整域函数；普通值调用的跨目标返回通道尚未实现",
            );
        }
        let Some(arguments) = unsafe { self.world.call(self.function_arguments(&key, &args, span)).await }? else {
            return Ok(None);
        };
        let explicit=match explicit.as_ref() {
            Some(arguments)=>Some(unsafe { self.world.call(self.scope.bindings().arguments(self.world,arguments,
                (self.scope.source(),self.scope.module(),self.scope.imports()),call.span())).await }?),
            None=>None,
        };
        let instance = unsafe { self.world.call(self.specialized_arguments(key, &arguments, explicit.as_deref(), span)).await }?;
        let input = DomainInput::Declaration { instance: &instance, signature: None };
        unsafe { self.world.call(self.application_execute(input, arguments, target, span)).await }
    }
    pub(super) async fn application_execute(
        &mut self,
        input: DomainInput<'_>,
        prepared: Vec<Argument>,
        target: Target,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        let frame = self.application_frame(span)?;
        let callee = &input.owner().definition;
        let bound_parameters=unsafe { self.world.call(input.parameters(self.world)).await }?;
        if input.target() != Target::Compute && input.target() != target {
            return self.fail(span, "B266", "函数声明的目标能力与本次调用位置不一致");
        }
        use function_inputs::Parameter;
        let mut data = Vec::new();
        let mut held = Vec::new();
        let mut arguments = Vec::with_capacity(prepared.len());
        for (parameter, value) in bound_parameters.iter().zip(prepared) {
            let argument = match function_inputs::parameter_form(parameter) {
                Parameter::Dataset => {
                    let value = match value {
                        Argument::ApplicationData(data) => DataReference::direct(data),
                        Argument::BorrowedData(reference) => {
                            reference
                        }
                        Argument::Scoped { value:returns::Returned::Dataset(data), owner } => {
                            held.push(owner);
                            DataReference::direct(data)
                        }
                        _ => return self.fail(span, "B224", "整域调用需要当前应用持有的实际 dataset"),
                    };
                    if !value.permits(parameter.access) {
                        return self.fail(span, "B227", "dataset借用的访问模式不能转换为被调参数权限");
                    }
                    let path=self.residency_path();
                    if !self
                        .residency
                        .require(&value.residency, target, self.scope.source(), span,&path)
                    {
                        return self.fail(
                            span,
                            "B266",
                            "实际存储与调用位置不一致；先显式 upload／download",
                        );
                    }
                    if parameter.access.reads() {
                        self.require_application_reference_initialized(&value, span)?;
                    }
                    let slot = data.len();
                    data.push(value);
                    DomainCallArgument::Dataset(slot)
                }
                Parameter::Texture => {
                    let Argument::Texture(resource) = value else {
                        return self.fail(span, "B239", "整域纹理参数需要实际只读纹理资源");
                    };
                    unsafe { self.world.call(self.texture_parameter(parameter, &resource,(&callee.source,&callee.module,&callee.imports),span)).await }?;
                    let address = self.reg(Scalar::U64);
                    self.emit(InstKind::TextureBindingAddress { dst: address, texture: resource.source });
                    DomainCallArgument::Texture(CallArgument::Value { origin: None, dataset_borrow: None,
                        ty: Ty::scalar(Scalar::U64), values: vec![address] })
                }
                _ => {
                    let Argument::Value(value) = value else {
                        return self.fail(span, "B266", "当前整域统一参数需要普通值");
                    };
                    DomainCallArgument::Uniform(value)
                }
            };
            arguments.push(argument);
        }
        for value in &data {
            self.application_handle_available(value.root.identity(), span, &[],&self.path)?;
        }
        let interface = unsafe { self.world.call(self.prepare_domain(input, target, &bound_parameters)).await }?;
        if let DomainInput::Declaration { .. } = input {
            for &(left, right) in &interface.length_requirements {
                if !self.lengths_equal(&data[left].length, &data[right].length) {
                    return self.fail(span, "B226", "整域调用前未证明same_length数据集长度关系");
                }
            }
        }
        for (value, buffer) in data.iter().zip(&interface.buffers) {
            self.require_same(&buffer.ty, value.element_layout().1, span)?;
            if buffer.count.is_some() && buffer.count != value.count {
                return self.fail(span, "B167", "调用方没有交付函数需要的数据形状");
            }
        }
        let uniform_values = arguments.iter().filter_map(|argument| match argument {
            DomainCallArgument::Uniform(value) => Some(value),
            _ => None,
        });
        for (value, uniform) in uniform_values.zip(&interface.uniforms) {
            self.require_same(&uniform.ty, &value.ty, span)?;
        }
        if let DomainInput::Declaration { .. } = input {
            for requirement in &interface.element_requirements {
                let (binding, offset) = match requirement {
                    ElementFact::Range(range) => (range.binding, range.offset),
                    ElementFact::Index { binding, offset, .. } => (*binding, *offset),
                };
                let source = &data[binding];
                if source.length.maximum() == 0 { continue; }
                let shallow;
                let elements=match source.elements.known(){Some(elements)=>elements,None=>{shallow=unsafe { self.world.call(self.unknown_element_model(&source.ty,(&source.residency,&[]))).await }?;&shallow}};
                let model = self.returned_storage(elements);
                let word = source.element_layout().0 + offset;
                let component = source.data.ty.components.iter().position(|(_, at)| *at == word).unwrap();
                let value = self.value_component(&model, component);
                let proven = match requirement {
                    ElementFact::Range(range) => value.proof.holds_range(0, range.scalar, &range.intervals,range.known_bits),
                    ElementFact::Index { target, .. } => {
                        let target = &data[*target];
                        self.index_in(&value, IndexDomain::ApplicationData(target.root.identity()), target.length.minimum(), None)
                    },
                };
                if !proven { return self.fail(span, "B226", "整域调用前尚未证明共同字段输入前提"); }
            }
        }
        for (&binding, written) in &interface.write_regions {
            let reference = &data[binding];
            let ty = &interface.buffers[binding].ty;
            let components = ty.components.iter().enumerate().filter_map(|(i, (scalar, word))|
                written.iter().any(|&(start, end)| u64::from(*word) < u64::from(end)
                    && u64::from(start) < u64::from(*word) + u64::from(scalar.size())).then_some(i)
            ).collect::<Vec<_>>();
            let offset = reference.element_layout().0;
            let words = components.iter().map(|&i| offset + ty.components[i].1).collect::<Vec<_>>();
            let output = interface.elements.get(&binding).map(|proof| proof.project(&components, ty.components.len())
                .element_facts(|target| {
                    let target = &data[target];
                    let mut bound = bounds::Proof::default();
                    bound.mark_domain(0, IndexDomain::ApplicationData(target.root.identity()));
                    bound.mark_domain(0, IndexDomain::Fixed(target.length.maximum()));
                    for identity in target.length.component_origins(0) {
                        bound.mark_domain(0, IndexDomain::Upper(identity, 1));
                    }
                    bound
                }));
            let update = match &output {
                Some(proof) => ElementUpdate::WholeFacts(proof),
                None => ElementUpdate::Unknown,
            };
            unsafe { self.world.call(self.update_application_contents(reference, &words, update)).await }?;
        }
        self.initialize_application_domain(&interface, &data);
        let mut physical = Vec::with_capacity(arguments.len() + 1);
        physical.push(argument(frame));
        physical.extend(arguments.into_iter().map(|prepared| match prepared {
            DomainCallArgument::Dataset(slot) => self.application_domain_argument(&data[slot]),
            DomainCallArgument::Uniform(value) => argument(value),
            DomainCallArgument::Texture(value) => value,
        }));
        self.call_domain(input.name(), target, physical);
        for relation in &interface.prefix_guarantees {
            let count=&data[relation.count.binding];
            let word=count.element_layout().0+relation.count.word;
            let domain = prefix::domain(&data[relation.domain]);
            let facts = std::iter::once((word, &domain));
            unsafe { self.world.call(self.apply_application_prefixes(count, facts)).await }?;
        }
        for name in held {
            unsafe { self.world.call(self.finish_scoped_argument(&name, span)).await }?;
        }
        Ok(Some(returns::Returned::Unit))
    }
    #[inline(never)]
    pub(super) async fn application_parallel(&mut self, statement: &Stmt, element:&str, domain:&ParallelDomain, body:&Block) -> Result<()> {
        if !self.application_entry() {
            return self.fail(
                statement.span(),
                "B131",
                "并行域由主流程或整域函数发起；元素实例中的嵌套域尚未实现",
            );
        }
        let mut names = BTreeSet::new();
        captured_names(statement, &mut names);
        let mut params = Vec::new();
        let mut args = Vec::new();
        for name in names {
            let Some(binding) = self.env.get(&name) else {
                continue;
            };
            let (ty, access, arg) = if matches!(binding, Binding::ApplicationData(_) | Binding::DataReference(_)) {
                let data = unsafe { self.world.call(self.application_named_reference(&name, statement.span())).await }?.unwrap();
                let access = if data.access == Access::Atomic {
                    Access::Atomic
                } else if data.access == Access::Read {
                    Access::Read
                } else if self.application_initialized(&data) {
                    Access::InOut
                } else {
                    Access::Write
                };
                let mut ty = data.element_layout().1.syntax();
                ty.dataset = true;
                ty.count = data.count.map(Shape::Literal);
                (ty, access, Argument::BorrowedData(data))
            } else {
                let value = unsafe { self.world.call(self.name_expr(&name, statement.span())).await }?;
                (value.ty.syntax(), Access::Value, Argument::Value(value))
            };
            params.push(Param {
                name,
                ty,
                access,
                span: statement.span(),
            });
            args.push(arg);
        }
        self.check_argument_aliases(|index|(params[index].access,params[index].ty.dataset), &args, |_| statement.span())?;
        // 生成域按实际 dataset 的捕获顺序编号。
        let captured = args.iter().zip(&params).filter_map(|(argument, parameter)| {
            match argument { Argument::BorrowedData(data) => Some((parameter.access, data)), _ => None }
        }).collect::<Vec<_>>();
        let mut prefix_requirements = Vec::new();
        fn count_fields(
            ty: &Ty, base: u32, facts: &prefix::Fields, visit: &mut impl FnMut(u32, &view::Borrow),
        ) {
            if facts.range(base..base + ty.size).next().is_none() {
                return;
            }
            for (_, relative, field) in &ty.fields {
                let word = base + relative;
                if let Some(fact) = facts.get(&word) && field.as_ref() == &Ty::scalar(Scalar::U32) {
                    visit(word, fact);
                } else if !field.fields.is_empty() {
                    count_fields(field, word, facts, visit);
                }
            }
        }
        fn domain_regions(
            resource: &view::Borrow, ty: &Ty, requirement: PrefixRelation, known: &view::Borrow,
            path: &bounds::Clause, selected: &mut Vec<usize>, out: &mut Vec<PrefixRelation>,
        ) {
            let projected = view::Borrow::projection(resource.clone(), selected.clone());
            if prefix::covers(known, &projected, path) {
                out.push(requirement);
                return;
            }
            let base = requirement.domain_region.map_or(0, |(start, _)| start);
            for (index, (_, offset, field)) in ty.fields.iter().enumerate() {
                selected.push(index);
                let region = PrefixRelation {
                    domain_region: Some((base + offset, field.size)),
                    ..requirement
                };
                domain_regions(resource, field, region, known, path, selected, out);
                selected.pop();
            }
        }
        for (binding, (access, count)) in captured.iter().enumerate() {
            if count.count != Some(1) || !access.reads() || count.prefixes.is_empty() {
                continue;
            }
            let (base, ty) = count.element_layout();
            count_fields(ty, base, &count.prefixes, &mut |word, known| {
                let count = PrefixCount { binding, word: word - base };
                for (domain, (_, data)) in captured.iter().enumerate() {
                    if domain != binding {
                        domain_regions(&prefix::domain(data), data.element_layout().1,
                            PrefixRelation { domain, domain_region: None, count },
                            known, &self.path, &mut Vec::new(), &mut prefix_requirements);
                    }
                }
            });
        }
        prefix_requirements.sort();
        prefix_requirements.dedup();

        let mut length_requirements = Vec::new();
        let mut groups: Vec<usize> = Vec::new();
        for (binding, (_, data)) in captured.iter().enumerate() {
            let representative = groups.iter().copied().find(|&previous| {
                self.lengths_equal(&data.length, &captured[previous].1.length)
            });
            if let Some(previous) = representative {
                length_requirements.push((previous, binding));
            } else {
                groups.push(binding);
            }
        }
        length_requirements.sort_unstable();
        let mut element_inputs = Vec::new();
        for (binding, (access, source)) in captured.iter().enumerate() {
            if !access.reads() || *access == Access::Atomic { continue; }
            let materialized;
            let elements = match source.elements.known() {
                Some(model) => model,
                None => {
                    materialized = unsafe { self.world.call(self.element_model(
                        &source.ty, &source.elements, (&source.residency, &[]), statement.span(),
                    )).await }?;
                    &materialized
                }
            };
            let storage;
            let model = match elements.view_value() {
                Some(value) => value,
                None => {
                    storage = self.returned_storage(elements);
                    &storage
                }
            };
            let (offset, ty) = source.element_layout();
            for &(scalar, word) in &ty.components {
                if scalar == Scalar::Bool { continue; }
                let component = source.data.ty.components.iter()
                    .position(|item| *item == (scalar, offset + word)).unwrap();
                let intervals = model.component_ranges(component).0;
                if scalar == Scalar::U32 {
                    for (target, (_, data)) in captured.iter().enumerate() {
                        let length = std::iter::once_with(|| (
                            Cow::Owned(data.length.component_origins(0)),
                            data.length.proof.domains(0),
                        ));
                        if self.index_known(&model, component, IndexDomain::ApplicationData(data.root.identity()),
                            data.length.minimum(), || intervals.last().map(|range| range.1), length) {
                            element_inputs.push(ElementFact::Index { binding, offset: word, target });
                        }
                    }
                }
                if let Some(range) = model.element_range_fact(binding, component, word, intervals) {
                    element_inputs.push(ElementFact::Range(range));
                }
            }
        }
        element_inputs.sort();
        element_inputs.dedup();
        let name = format!(
            "{}@parallel{}:{}",
            self.calls.last().unwrap(),
            statement.span().start,
            self.functions.units.len()
        );
        let owner=self.scope.function().unwrap().clone();
        let parameters=Arc::<[Param]>::from(params);
        let site=self.source_at(statement.span(),SourceSyntax::Statement);
        unsafe { self.world.call(self.application_execute(
            DomainInput::Parallel{owner:&owner,name:&name,parameters:&parameters,
                length_requirements:&length_requirements,prefix_requirements:&prefix_requirements,
                element_requirements:&element_inputs,
                statement,element,domain,body,site:&site},
            args,Target::Cpu,statement.span(),
        )).await }?;
        Ok(())
    }
    pub(super) fn application_cpu(&mut self, data: &Data, span: Span) -> Result<()> {
        let path=self.residency_path();
        if !self
            .residency
            .require(&data.residency, Target::Cpu, self.scope.source(), span,&path)
        {
            return self.fail(
                span,
                "B266",
                "CPU 访问需要 CPU 存储；先显式 download，条件分支也须统一存储位置",
            );
        }
        Ok(())
    }
    pub(super) async fn application_transfer(
        &mut self,
        name: &str,
        args: &Expressions,
        span: Span,
    ) -> Result<Option<Value>> {
        self.application_frame(span)?;
        if args.len() != 1 {
            return self.fail(span, "B266", "upload／download 需要一个拥有的 dataset 名称");
        }
        let Some(data) = unsafe { self.world.call(self.application_reference(&args.at(0))).await }? else {
            return self.fail(span, "B266", "upload／download 需要应用拥有的 dataset");
        };
        if data.ownership != Ownership::Owned {
            return self.fail(
                span,
                "B227",
                "只有资源所有者可以转移 CPU／GPU 存储，函数借用不能转移",
            );
        }
        self.view_uses.invocation(span);
        let borrow=unsafe { self.world.call(self.view_uses.all(self,&[])).await }?;self.application_movable(&data,&borrow,span)?;
        let (source, target) = if name == "upload" {
            (Target::Cpu, residency::Residency::gpu())
        } else {
            (Target::Gpu, residency::Residency::cpu())
        };
        let path=self.residency_path();
        if !self
            .residency
            .require(&data.residency, source, self.scope.source(), span,&path)
        {
            return self.fail(
                span,
                "B266",
                "存储位置与传输方向不一致；条件分支须分别完成显式传输",
            );
        }
        let value = data.root.clone();
        let origin = Arc::new(GeneratedOrigin {
            reason: if source == Target::Cpu { GenerationReason::DatasetUpload }
                else { GenerationReason::DatasetDownload },
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        self.application_call(name, vec![value], None, span, Some(origin))?;
        let update = &mut |alias: &mut Data| {
            if alias.ownership != Ownership::Moved
                && alias.root.proof.known_identity(0) == Some(data.root.identity())
            {
                alias.residency = target.clone();
            }
        };
        for binding in self.env.values_mut() {
            match binding {
                Binding::ApplicationData(alias) => update(alias),
                Binding::OwnedValue(owner) => owner.resources_mut(update),
                _ => {}
            }
        }
        Ok(None)
    }
    pub(super) async fn println(&mut self, args: &Expressions, span: Span) -> Result<Option<Value>> {
        unsafe { self.world.call(self.println_value(args.len(),span,args.first().map(|a|a.span()).unwrap_or(span),
            async |builder|unsafe { builder.world.call(builder.expr(&args.at(0))).await })).await }
    }
    pub(super) async fn println_value(&mut self,count:usize,span:Span,argument_span:Span,
        argument:impl std::ops::AsyncFnOnce(&mut Self)->Result<Option<Value>>)->Result<Option<Value>> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B260", "println 是 CPU 标准输出操作，要求显式 on cpu");
        }
        if count != 1 {
            return self.fail(span, "B261", "println 需要一个整数、f32、f64 或 bool 标量");
        }
        let Some(value) = argument(self).await? else {
            return Ok(None);
        };
        if !value.ty.components.first().is_some_and(|(scalar, _)| value.ty == Ty::scalar(*scalar))
        {
            return self.fail(
                argument_span,
                "B261",
                "println 接受整数、浮点或 bool 标量",
            );
        }
        let dst = self.reg(Scalar::Bool);
        self.emit(InstKind::Print {
            dst,
            value: value.regs[0],
        });
        Ok(Some(Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::scalar(Scalar::Bool),
            regs: vec![dst],
            uniform: false,
            constant: None,
        }))
    }
}

// Only whole binding assignment needs mutable handle/length storage; payload writes do not.
pub(super) fn rebound(body: &Block, name: &str) -> bool {
    fn root(expr: &Expr, name: &str) -> bool {
        match expr.kind() {
            ExprKind::Name(root) => root == name,
            ExprKind::Member(base, _) => root(&base, name),
            _ => false,
        }
    }
    body.iter().any(|s| match s.kind() {
        StmtKind::Assign(lhs, _, _) => root(&lhs, name),
        StmtKind::If(_, yes, no) => {
            rebound(&yes, name) || rebound(&no, name)
        }
        StmtKind::Match(_, arms) => arms.iter().any(|arm| rebound(&arm.body(),name)),
        StmtKind::For(_, _, _, body) | StmtKind::While(_, body) => rebound(&body, name),
        _ => false,
    })
}

fn captured_names(statement: &Stmt, names: &mut BTreeSet<String>) {
    fn name(name: &str, bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        if !bound.contains(name) {
            names.insert(name.to_owned());
        }
    }
    fn expr(e: &Expr, bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        match e.kind() {
            ExprKind::Name(n) => name(n, bound, names),
            ExprKind::Invoke(callee, args) => {
                expr(&callee, bound, names);
                for arg in args { expr(&arg, bound, names); }
            }
            ExprKind::Call(callee, args) => {
                name(callee, bound, names);
                for arg in args { expr(&arg, bound, names); }
            }
            ExprKind::Specialize(_, _, args)
            | ExprKind::Construct(_, args)
            | ExprKind::Cooperate(_, _, args) => {
                for a in args {
                    expr(&a, bound, names);
                }
            }
            ExprKind::Target(_, e) | ExprKind::Unary(_, e) | ExprKind::Member(e, _) => {
                expr(&e, bound, names)
            }
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
                expr(&a, bound, names);
                expr(&b, bound, names);
            }
            ExprKind::Select(a, b, c) => {
                expr(&a, bound, names);
                expr(&b, bound, names);
                expr(&c, bound, names);
            }
            _ => {}
        }
    }
    fn block(body: impl IntoIterator<Item=Stmt>, bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        let mut bound = bound.clone();
        for s in body {
            match s.kind() {
                StmtKind::Parallel(iterator, domain, body) => {
                    name(&domain.dataset(), &bound, names);
                    if let Some(e) = &domain.count() {
                        expr(e, &bound, names);
                    }
                    if let Some(e) = &domain.selection() {
                        expr(e, &bound, names);
                    }
                    let mut nested = bound.clone();
                    nested.insert(iterator.to_string());
                    block(&body, &nested, names);
                }
                StmtKind::Let(_, local, e) => {
                    expr(&e, &bound, names);
                    bound.insert(local.to_string());
                }
                StmtKind::Declare(_, local) => {
                    bound.insert(local.to_string());
                }
                StmtKind::Expr(e) | StmtKind::Return(Some(e)) | StmtKind::Seek(_, e) => {
                    expr(&e, &bound, names)
                }
                StmtKind::Assign(a, _, b) => {
                    expr(&a, &bound, names);
                    expr(&b, &bound, names);
                }
                StmtKind::If(e, a, b) => {
                    expr(&e, &bound, names);
                    block(&a, &bound, names);
                    block(&b, &bound, names);
                }
                StmtKind::Match(e, arms) => {
                    expr(&e,&bound,names);
                    for arm in arms {let mut nested=bound.clone();nested.extend(arm.binding().into_iter().map(str::to_owned));block(&arm.body(),&nested,names);}
                }
                StmtKind::While(e, body) => {
                    expr(&e, &bound, names);
                    block(&body, &bound, names);
                }
                StmtKind::For(iterator, a, b, body) => {
                    expr(&a, &bound, names);
                    if let Some(b) = b {
                        expr(&b, &bound, names);
                    }
                    let mut nested = bound.clone();
                    nested.insert(iterator.to_string());
                    block(&body, &nested, names);
                }
                _ => {}
            }
        }
    }
    block(std::iter::once(statement.clone()), &BTreeSet::new(), names);
}

// 函数出口已要求完整拥有值；局部条件与Place一样不进入模块接口。


// slots与初始化位置属于当前编译调用；实际原生接口只交接已初始化资源。



// Common element metadata belongs to the original logical Data.
type ElementBindings = BTreeMap<ValueIdentity,bounds::Proof>;



pub(super) fn model_data<'a>(model:Option<&'a returns::Returned>,path:&[usize])->Option<&'a Data> {
    match model? {
        returns::Returned::Dataset(data) if path.is_empty()=>Some(data),
        returns::Returned::OwnedValue(value)=>view::owned_data_at(value,path),
        _=>None,
    }
}
pub(super) fn model_data_mut<'a>(model:Option<&'a mut returns::Returned>,path:&[usize])->Option<&'a mut Data> {
    match model? {
        returns::Returned::Dataset(data) if path.is_empty()=>Some(data),
        returns::Returned::OwnedValue(value)=>view::owned_data_at_mut(value,path),
        _=>None,
    }
}
pub(super) fn binding_element_count(binding:&Binding)->usize {
    match binding {
        Binding::ApplicationData(_)|Binding::DataReference(_)=>1,
        Binding::OwnedValue(value)=>view::resource_count(&value.value.ty),
        _=>0,
    }
}
fn binding_element_data<'a>(binding:&'a Binding,resource:usize)->Option<&'a Data> {
    match binding {
        Binding::ApplicationData(data) if resource==0=>Some(data),
        Binding::DataReference(data) if resource==0=>Some(&data.data),
        Binding::OwnedValue(value)=>view::resource_path(&value.value.ty, resource)
            .and_then(|path|view::owned_data_at(value,&path)),
        _=>None,
    }
}
pub(super) fn binding_element_data_mut(binding:&mut Binding,resource:usize)->Option<&mut Data>{
    match binding{
        Binding::ApplicationData(data)if resource==0=>Some(data),
        Binding::DataReference(data)if resource==0=>Some(&mut data.data),
        Binding::OwnedValue(value)=>{let path=view::resource_path(&value.value.ty, resource)?;view::owned_data_at_mut(value,&path)},
        _=>None,
    }
}
pub(super) fn each_binding_data(binding:&mut Binding,visit:&mut impl FnMut(usize,&[usize],&mut Data)) {
    match binding {
        Binding::ApplicationData(data)=>visit(0,&[],data),
        Binding::DataReference(data)=>visit(0,&[],&mut data.data),
        Binding::OwnedValue(value)=>{
            for(resource,path)in view::resource_paths(&value.value.ty).into_iter().enumerate(){
                if let Some(data)=view::owned_data_at_mut(value,&path){visit(resource,&path,data);}
            }
        }
        _=>{},
    }
}
pub(super) fn element_scope_bindings(bindings:&ElementBindings,width:usize,parent_stored:bool)->ElementBindings {
    fn parent(id:ValueIdentity)->ValueIdentity {
        match id {
            ValueIdentity::ElementComponent(component)=>ValueIdentity::ElementParent(0,component),
            ValueIdentity::ElementParent(depth,component)=>ValueIdentity::ElementParent(depth+1,component),
            ValueIdentity::ElementIndex(depth)=>ValueIdentity::ElementIndex(depth+1),
            other=>other,
        }
    }
    let shifts=bindings.keys().copied().filter(|id|matches!(id,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)|ValueIdentity::ElementIndex(_)))
        .map(|id|(id,parent(id))).collect::<BTreeMap<_,_>>();
    let mut out=bindings.iter().map(|(id,proof)|{
        if parent_stored&&!matches!(id,ValueIdentity::DatasetContent(..)){(parent(*id),proof.output_bound(&shifts))}
        else{(*id,proof.clone())}
    }).collect::<ElementBindings>();
    let ordinal=ValueIdentity::ElementIndex(0);out.insert(ordinal,bounds::Proof::default().with_identity(ordinal));
    for component in 0..width {
        let id=ValueIdentity::ElementComponent(component);
        out.insert(id,bounds::Proof::default().with_identity(id));
    }
    out
}
pub(super) fn element_input_bindings(builder:&mut Builder<'_>,definition:outputs::Contract<'_>,env:&BTreeMap<String,Binding>,paths:&BTreeSet<residency::Input>,declared:Option<&BTreeMap<residency::Input,(usize,usize)>>)->(ElementBindings,view::Borrow) {
    fn collect(builder:&Builder<'_>,data:&Data,parent:view::Borrow,mut bindings:ElementBindings,parameter:usize,resource:usize,next:&mut usize,out:&mut ElementBindings,memory:&mut view::Borrow,reads:bool,input:&residency::Input,paths:&BTreeSet<residency::Input>,declared:Option<&BTreeMap<residency::Input,(usize,usize)>>){
        let identity=match declared{Some(declared)=>declared.get(input).copied(),None=>Some((resource,*next))};
        let model=data.elements.known().unwrap();
        let value=builder.returned_storage(model);let width=value.regs.len();
        for component in 0..width{
            let id=ValueIdentity::ElementComponent(component);
            bindings.insert(id,bounds::Proof::default().with_identity(id));
        }
        builder.bind_element_resource_borrows(data,&parent,&mut bindings);
        let mut proof=bounds::Proof::capture(&value);
        let model_values=proof.scalar_identities().into_iter().filter(|id|matches!(id,ValueIdentity::ElementComponent(_)))
            .map(|id|(id,bounds::Proof::default())).collect();
        let mut references=view_contracts::components(&value.ty);
        for fields in view::resource_paths(&value.ty){if let Some(data)=model_data(Some(model),&fields){
            references.insert(value.regs.iter().position(|reg|*reg==data.root.regs[0]).unwrap());
        }}
        for component in 0..width{
            let borrow=proof.view_borrow(component).at_call(&bindings);
            proof.set_view_borrow(component,borrow.clone());
            let mut input=proof.project(&[component],width);
            if !references.contains(&component){input=input.substitute_values(&model_values);}
            let input={let mut component_proof=input;component_proof.set_view_borrow(0,borrow);component_proof};
            if let Some((resource,start))=identity{out.insert(ValueIdentity::DatasetContent(parameter,resource,start+component),input);}
        }
        if reads{memory.union(&builder.address_contents(&value.ty,&proof));}
        *next+=width;
        for fields in view::resource_paths(&value.ty){
            let mut selected=input.clone();selected.fields.extend(fields.iter().copied().map(residency::Part::Field));selected.fields.push(residency::Part::Elements);
            if paths.contains(&selected){
                let child=model_data(Some(model),&fields).unwrap();
                let nested=element_scope_bindings(&bindings,child.ty.components.len(),true);
                collect(builder,child,view::Borrow::element(parent.clone(),fields,Some(bounds::Operand::Value(ValueIdentity::ElementIndex(0)))),nested,parameter,resource,next,out,memory,reads&&child.length.maximum()!=0,&selected,paths,declared);
            }
        }
    }
    let mut out=ElementBindings::new();let mut memory=view::Borrow::default();
    for(parameter,p)in definition.parameters().enumerate(){
        if let Some(binding)=env.get(p.name()){
            for resource in 0..binding_element_count(binding){
                if let Some(data)=binding_element_data(binding,resource){
                    let mut projected;
                    let data=if let Binding::DataReference(reference)=binding {
                        projected=data.clone();
                        projected.root.proof.set_view_borrow(0,view::Borrow::projection(builder.data_value_borrow(data),reference.projection.clone()));
                        projected.ty=reference.element_layout().1.clone();
                        projected.elements=projected.elements.map(&mut |model|{
                            let mut model=*model;
                            for field in &reference.projection{model=match model{returns::Returned::OwnedValue(parent)=>builder.unpack_record(&parent,*field),returns::Returned::Value(value)=>returns::Returned::value(builder.record_component(&value,*field)),_=>unreachable!()};}
                            Box::new(model)
                        },&mut |when|when);
                        &projected
                    }else{data};
                    let fields=match binding{Binding::OwnedValue(value)=>view::resource_path(&value.value.ty, resource).unwrap(),_=>Vec::new()};
                    let input=residency::Input{parameter,fields:fields.into_iter().map(residency::Part::Field).chain([residency::Part::Elements]).collect()};
                    collect(builder,data,builder.data_value_borrow(data),builder.element_owner_bindings(data),parameter,resource,&mut 0,&mut out,&mut memory,p.access()!=Access::Write&&data.length.maximum()!=0,&input,paths,declared);
                }
            }
        }
    }
    (out,memory)
}
pub(super) fn bind_element_input(builder:&mut Builder<'_>,model:&mut returns::Returned,resource:usize,next:&mut usize,inputs:&mut ElementBindings,input:&residency::Input){
        let value=builder.returned_storage(model);let width=value.regs.len();let mut bindings=inputs.clone();
        let parameter=input.parameter;let depth=input.depth();
        let (resource,start)=*builder.view_model.element_inputs.entry(input.clone()).or_insert((resource,*next));
        for component in 0..width{
            let id=ValueIdentity::DatasetContent(parameter,resource,start+component);
            inputs.insert(id,bounds::Proof::default().with_identity(id));
            builder.view_model.parameters.insert(id,id);
            bindings.insert(ValueIdentity::ElementComponent(component),bounds::Proof::default()
                .with_identity(ValueIdentity::ElementComponent(component)).with_identity(id));
        }
        native_function::translate_in_place(model,&bindings,true);
        if let Some(value)=model.view_value_mut(){
            let indices=(0..depth).rev().map(|depth|Some(bounds::Operand::Value(ValueIdentity::ElementIndex(depth)))).collect::<Vec<_>>();
            value.proof=value.proof.clone().view_parameters(&value.ty,&indices,|component|ValueIdentity::DatasetContent(parameter,resource,start+component));
        }
        *next=start+width;
        if builder.scope.parameters().nth(parameter).unwrap().access()!=Access::Write{
            for fields in view::resource_paths(&value.ty){if let Some(data)=model_data_mut(Some(model),&fields) && matches!(data.elements,ElementFacts::Unknown){data.elements=ElementFacts::Input(bounds::Condition::default());}}
        }
}
pub(super) fn bind_parameter_elements(builder:&mut Builder<'_>,definition:outputs::Contract<'_>,paths:&BTreeSet<residency::Input>){
    for(parameter,p)in definition.parameters().enumerate(){
        if let Some(mut binding)=builder.env.remove(p.name()){
            each_binding_data(&mut binding,&mut |resource,path,data|{
                let root=residency::Input{parameter,fields:path.iter().copied().map(residency::Part::Field).chain([residency::Part::Elements]).collect()};
                let mut next=0;let mut inputs=ElementBindings::new();
                for input in paths.iter().filter(|input|input.parameter==parameter&&input.fields.starts_with(&root.fields)){
                    let mut selected=&mut *data;let mut fields=Vec::new();
                    for part in &input.fields[root.fields.len()..]{match part{
                        residency::Part::Field(field)=>fields.push(*field),
                        _=>{selected=model_data_mut(selected.elements.known_mut(),&fields).unwrap();fields.clear();},
                    }}
                    let Some(elements)=selected.elements.known_mut()else{continue;};
                    bind_element_input(builder,elements,resource,&mut next,&mut inputs,input);
                    if p.access()!=Access::Write && selected.length.maximum()!=0 {
                        residency::visit(elements,&mut input.clone(),&mut |data,input|data.residency=residency::Residency::input(input.model()));
                    }
                }
            });
            builder.env.insert(p.name().to_owned(),binding);
        }
    }
}
impl Builder<'_> {
    pub(super) async fn unknown_element_model(&self,ty:&Ty,location:(&residency::Residency,&[usize]))->Result<Box<returns::Returned>>{
        let mut proof=bounds::Proof::default();
        for(component,(scalar,_))in ty.components.iter().enumerate(){
            proof.append(&Value{ty:Ty::scalar(*scalar),regs:vec![component],proof:bounds::Proof::default().with_identity(ValueIdentity::ElementComponent(component)),uniform:false,literal:false,constant:None});
        }
        for component in view_contracts::components(ty){proof.set_view_borrow(component,view::Borrow::source(view::Source::Unresolved));}
        let mut model=unsafe { self.world.call(self.template_returned(Value{ty:ty.clone(),regs:(0..ty.components.len()).collect(),proof,uniform:false,literal:false,constant:None},true)).await }?;
        let parent=location.0.scope_indices(1,0);
        let index=ValueIdentity::ElementIndex(0);let selection=BTreeMap::from([(index,bounds::Proof::default().with_identity(index))]);
        let mut input=residency::Input{parameter:0,fields:std::iter::once(residency::Part::Selected(Some(bounds::Operand::Value(index)))).chain(location.1.iter().copied().map(residency::Part::Field)).collect()};
        residency::visit(&mut model,&mut input,&mut|data,input|data.residency=parent.project(&input.fields,&selection));
        Ok(Box::new(model))
    }
    pub(super) fn owned_resource_borrow(&self,data:&Data)->view::Borrow {
        fn collect(builder:&Builder<'_>,data:&Data,parent:view::Borrow,mut bindings:ElementBindings,out:&mut view::Borrow){
            out.union(&parent);if data.length.maximum()==0{return;}
            builder.bind_element_resource_borrows(data,&parent,&mut bindings);
            for fields in view::resource_paths(&data.ty){if let Some(child)=model_data(data.elements.known(),&fields){
                let borrow=view::resource_borrow(child).at_call(&bindings);
                let nested=element_scope_bindings(&bindings,child.ty.components.len(),true);
                collect(builder,child,borrow,nested,out);
            }}
        }
        let mut out=view::Borrow::default();collect(self,data,view::resource_borrow(data),self.element_owner_bindings(data),&mut out);out
    }
    pub(super) fn element_owner_bindings(&self,data:&Data)->ElementBindings {
        fn aliases(borrow:&view::Borrow,out:&mut ElementBindings){
            for source in &borrow.sources{match source{
                view::Source::Data(id@(ValueIdentity::Register(_)|ValueIdentity::ResourceParameter(..)))=>{out.entry(*id).or_insert_with(||bounds::Proof::default().with_identity(*id));},
                view::Source::Parameter(id@ValueIdentity::DatasetContent(..),_)=>{out.entry(*id).or_insert_with(||{let mut component_proof=bounds::Proof::default().with_identity(*id);component_proof.set_view_borrow(0,view::Borrow::source(view::Source::Parameter(*id,Vec::new())));component_proof});},
                view::Source::Element{parent,..}|view::Source::Projection{parent,..}=>aliases(parent,out),
                view::Source::Position(origin)=>aliases(&origin.resources(),out),_=>{},
            }}
        }
        fn owned(data:&Data,out:&mut ElementBindings){
            aliases(&data.root.proof.view_borrow(0),out);
            if let Some(value)=data.elements.known().and_then(returns::Returned::view_value){aliases(&value.proof.typed_view_borrow(&value.ty),out);for id in value.proof.scalar_identities(){if matches!(id,ValueIdentity::MemoryValue(..)|ValueIdentity::LoopValue(..)){out.entry(id).or_insert_with(||bounds::Proof::default().with_identity(id));}}}
            for fields in view::resource_paths(&data.ty){if let Some(child)=model_data(data.elements.known(),&fields){owned(child,out);}}
        }
        let mut out=ElementBindings::new();owned(data,&mut out);
        let ordinal=ValueIdentity::ElementIndex(0);out.insert(ordinal,bounds::Proof::default().with_identity(ordinal));
        for value in [&data.root,&data.length]{
            let proof=bounds::Proof::capture(value);
            for id in value.component_origins(0){out.insert(id,proof.clone());}
        }
        for (name,binding) in &self.env{
            let parameter=self.scope.parameters().any(|p|p.name()==*name);
            let value=match binding{Binding::Value(value)=>Some(value),Binding::OwnedValue(value)=>Some(&value.value),_=>None};
            let captured=if let Some(value)=value{
                Some((bounds::Proof::capture(value),if parameter{(0..value.regs.len()).collect()}else{view_contracts::components(&value.ty)}))
            }else if parameter && let Binding::Owned(owner)=binding{
                Some((owner.proof.clone(),(0..owner.proof.width()).collect()))
            }else{None};
            if let Some((proof,components))=captured{for component in components{
                let part=proof.project(&[component],proof.width());
                for &id in &proof.origins[component]{out.entry(id).or_insert_with(||part.clone());}
            }}
            let mut capture_source = |source: &Data| {
                for value in [&source.root, &source.length] {
                    let proof = bounds::Proof::capture(value);
                    for id in value.component_origins(0) {
                        out.entry(id).or_insert_with(|| proof.clone());
                    }
                }
            };
            view::visit_data(binding, &mut capture_source);
            if let Binding::DataReference(reference) = binding {
                capture_source(&reference.data);
            }
        }
        native_function::element_inputs(self,&mut out);
        out
    }
    pub(super) fn bind_element_resource_borrows(&self,data:&Data,parent:&view::Borrow,bindings:&mut ElementBindings) {
        let shifts=bindings.keys().filter_map(|id|match id{ValueIdentity::ElementIndex(depth)=>Some((*id,ValueIdentity::ElementIndex(depth+1))),_=>None}).collect();
        let parent=parent.output_bound(&shifts);
        let index=bounds::Operand::Value(ValueIdentity::ElementIndex(0)).at_call(bindings);
        for fields in view::resource_paths(&data.ty) {
            if let Some(resource)=model_data(data.elements.known(),&fields) {
                let id=ValueIdentity::ElementComponent(resource.root.regs[0]);
                let proof=bindings.entry(id).or_default();
                let mut borrow=proof.view_borrow(0);
                borrow.union(&view::Borrow::element(parent.clone(),fields,index));
                borrow.union(&view::resource_aliases(resource));
                proof.set_view_borrow(0,borrow);
            }
        }
    }
    fn element_index_operand(&self,index:bounds::Operand,context:(&bounds::Clause,&BTreeMap<String,Binding>))->bounds::Operand {
        match index {
            bounds::Operand::Value(id)=>{
                let proof=bounds::scoped_proof(&Ty::scalar(Scalar::U32),bounds::Proof::default().with_identity(id),context.0,context.1);
                bounds::Operand::Value(proof.origins.iter().flatten().copied().fold(id,std::cmp::min))
            },
            constant=>constant,
        }
    }
    fn element_resource_scopes(&self,resource:&view::Borrow,owner:&view::Borrow,context:(&bounds::Clause,&BTreeMap<String,Binding>))->Vec<(Vec<Vec<usize>>,bool,bounds::Condition)> {
        fn collect(builder:&Builder<'_>,borrow:&view::Borrow,owner:&view::Borrow,path:&bounds::Clause,env:&BTreeMap<String,Binding>)->Vec<(Vec<Vec<usize>>,bool,bounds::Condition)> {
            let mut paths=Vec::new();let selected=borrow.restricted(path);let root=owner.restricted(path);
            let shared=selected.sources.entries().any(|(source,when)|matches!(source,view::Source::Data(_)|view::Source::Parameter(..)|view::Source::Element{..})&&root.overlaps_source(source,&when))
                ||selected.calls.entries().any(|(call,when)|when.overlaps(&root.calls.when(call)))
                ||borrow.inputs.iter().any(|input|{
                    let when = bounds::Clause::both(&borrow.inputs.when(input), &owner.inputs.when(input));
                    when == bounds::Clause::default() || when.overlaps(path)
                });
            if shared{paths.push((Vec::new(),selected==root,bounds::Condition::constant(true)));}
            for(source,when)in borrow.sources.entries(){
                let truth=bounds::Condition::guard(when).truth_in(path);
                if truth==Some(false){continue;}
                match source {
                    view::Source::Element{parent,fields,index}=>for(mut nested,complete,choice)in collect(builder,parent,owner,path,env){
                        let choice=choice.scope_element_indices(1).logical(index.map(|index|bounds::Condition::element_index(0,builder.element_index_operand(index,(path,env)))).unwrap_or_default(),false);
                        nested.push(fields.clone());paths.push((nested,complete&&truth==Some(true),choice));
                    },
                    view::Source::Projection{parent,..}=>for(nested,complete,choice)in collect(builder,parent,owner,path,env){paths.push((nested,complete&&truth==Some(true),choice));},
                    _=>{},
                }
            }
            paths
        }
        collect(self,resource,owner,context.0,context.1)
    }
    // These names belong to this target's existing element input domain. They
    // describe a stable field of that owner, not an emitted or global read.
    pub(super) async fn application_element_snapshot(&self,source:&effects::DataProjection<'_>,input:&Data,target:&effects::DataProjection<'_>,context:(&effects::LoopWrites,&bounds::Clause))->Result<bounds::Proof> {
        let(writes,path)=context;let(ty,parts)=&source.value;
        let mut proof=bounds::Proof::default().with_external_borrows(ty);
        let address_changed=writes.external||matches!(input.place.as_ref().map(Place::storage),Some(Place::Local(_,slots,..))if slots.iter().any(|slot|writes.locals.contains(slot)));
        if address_changed{return Ok(proof);}
        let loads=ApplicationLoads(self);let locations=unsafe { self.world.call(loads.resource_storage(input,&source.resource)).await }?;
        let scopes=self.element_resource_scopes(&target.resource,&source.resource,(path,&self.env));
        proof.origins.resize_with(ty.components.len(),Vec::new);
        for(output,choices)in parts.iter().enumerate(){
            let mut merged=None::<bounds::Proof>;
            for &component in choices {
                let(scalar,word)=source.base.components[component];
                let written=match &locations {
                    Some(locations)=>locations.iter().any(|(location,element,layout)|match location {
                        effects::StorageWrite::Frame{root,..}=>{
                            let range=match(layout,source.index){
                                (DataLayout::Declared,Some(bounds::Operand::Constant(Scalar::U32,index)))=>{
                                    let start=u128::from(index)*u128::from(ir::align_up(element.size,element.align))+u128::from(word);
                                    Some(start..start+u128::from(scalar.size()))
                                },
                                _=>None,
                            };
                            loads.storage_writes(writes,Some(*root),range)
                        },
                        effects::StorageWrite::Descendants{..}=>loads.storage_writes(writes,None,None),
                    }),
                    None=>loads.storage_writes(writes,None,None),
                };
                for(scope,complete,parents)in &scopes {
                    let selected=match source.index {
                        Some(index)if scope.is_empty()=>target.index.is_some_and(|target|bounds::Condition::index_equal(index,target).truth_in(path)==Some(true)),
                        Some(index)=>bounds::Condition::element_index(scope.len()-1,index).truth_in(&parents.assume(path,true))==Some(true),
                        None=>false,
                    };
                    let candidate=if !written&&*complete&&selected {
                        let identity=if scope.is_empty(){ValueIdentity::ElementComponent(component)}else{ValueIdentity::ElementParent(scope.len()-1,component)};
                        bounds::Proof::default().with_identity(identity)
                    }else{bounds::Proof::default()};
                    if let Some(merged)=&mut merged{merged.join(&candidate,1,None);}else{merged=Some(candidate);}
                }
            }
            if let Some(merged)=merged{proof.origins[output]=merged.origins.first().cloned().unwrap_or_default();}
        }
        Ok(proof)
    }
    pub(super) fn application_element_scopes(&self,sources:&[ValueIdentity],resource:&view::Borrow,data:&Data,context:(&bounds::Clause,&BTreeMap<String,Binding>))->Vec<(Vec<Vec<usize>>,bool,bounds::Condition)> {
        if sources==[data.root.identity()]{return vec![(Vec::new(),true,bounds::Condition::constant(true))];}
        let mut paths=BTreeMap::new();
        if sources.contains(&data.root.identity()){paths.insert(Vec::new(),(true,bounds::Condition::constant(true)));}
        let owner=view::resource_borrow(data);
        let owners=std::iter::once(&owner).chain(self.loop_contexts.iter().filter_map(|context|context.view_resources.get(&data.root.identity())));
        for owner in owners{for(path,complete,choice)in self.element_resource_scopes(resource,owner,context){paths.entry(path).and_modify(|(old,selection)|{*old|=complete;*selection=selection.clone().logical(choice.clone(),true);}).or_insert((complete,choice));}}
        paths.into_iter().map(|(path,(complete,choice))|(path,complete,choice)).collect()
    }
    pub(super) async fn assign_application_element(&mut self,data:DataReference,index:&Expr,projection:&[Expr],op:&str,rhs:&Expr,span:Span)->Result<()> {
        let Some(mut place)=unsafe { self.world.call(self.application_index(data.clone(),index)).await }?else{return Ok(())};
        for part in projection {
            match part.kind() {
                ExprKind::Member(_,field)=>{
                    let (ty,components)=member_type(place.ty(),field,self.scope.source(),part.span())?;
                    place=place.project(ty,&components);
                }
                ExprKind::Index(_,index)=>{
                    let held=self.hold_binding(Binding::DataReference(data.clone()));
                    let value=unsafe { self.world.call(self.expr(&index)).await };
                    self.env.remove(&held);
                    let Some(value)=value? else{return Ok(());};
                    place=self.array_index(place,value,index.span())?;
                }
                _=>unreachable!(),
            }
        }
        unsafe { self.world.call(self.assign_owned_storage(place,op,rhs,span)).await }
    }
    // These are the original already-evaluated metadata positions, not a second element table.
    fn element_selection(
        &self, place:Option<&Place>, first:usize, count:usize,
        mut bind:impl FnMut(ValueIdentity,usize),
    )->Option<owned_presence::Presence> {
        let mut current=place;let mut selected=owned_presence::Presence::All;
        for depth in first..first+count{
            let Place::ApplicationElement{index,index_value,parent,..}=current.map(Place::storage)?else{return None;};
            let actual=match *index_value{
                bounds::Operand::Constant(_,at)=>ownership::ApplicationIndex::Constant(at as u32),
                bounds::Operand::Value(id)=>{
                    bind(id,depth);
                    ownership::ApplicationIndex::Value(id,*index)
                },
            };
            selected=selected.and(owned_presence::Presence::indices(Comparison::Equal,ownership::ApplicationIndex::Value(ValueIdentity::ElementIndex(depth),depth),actual));current=parent.as_deref();
        }
        Some(selected)
    }
    async fn assign_application_element_model(&mut self,data:&DataReference,place:&Place,paths:&[Vec<usize>],value:Option<&returns::Returned>)->Result<()> {
        let resource=view::resource_borrow(&data.data);let one=self.constant(Scalar::U32,1);
        let names=self.env.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let mut binding=self.env.remove(&name).unwrap();
            for resource_index in 0..binding_element_count(&binding){
                let Some(target)=binding_element_data_mut(&mut binding,resource_index)else{continue;};
                let scopes=self.application_element_scopes(&data.sources,&resource,target,(&self.path,&self.env));let unique=scopes.len()==1;
                for(scope,complete_scope,_)in scopes {
                    let count=scope.len()+1;
                    let precise=unique&&complete_scope&&paths.len()==1;
                    let qualifier=if precise{bounds::Condition::constant(true)}else{bounds::Condition::default()};
                    let mut indices=BTreeMap::new();
                    let selection=self.element_selection(Some(place),0,count,|id,depth|{
                        if precise{indices.entry(id).or_insert(ValueIdentity::ElementIndex(depth));}
                    });
                    let selected=qualifier.logical(match &selection{Some(selection)=>selection.condition(),None=>bounds::Condition::default()},false);
                    let presence=selection.filter(|_|precise);
                    let mut complete_scope=bounds::Condition::constant(complete_scope);
                    let mut target=Some(&mut *target);
                    for fields in scope{target=match target{
                        Some(target)=>{unsafe { self.world.call(self.expand_current_element_facts(target)).await }?;complete_scope=complete_scope.clone().logical(bounds::Condition::comparison(Comparison::Equal,&target.length,&one),false);model_data_mut(target.elements.known_mut(),&fields)},
                        None=>None,
                    };}
                    let Some(target)=target else{continue;};unsafe { self.world.call(self.expand_current_element_facts(target)).await }?;
                    if value.is_none() && matches!(target.elements, ElementFacts::Unknown) {
                        target.elements = ElementFacts::Known(unsafe { self.world.call(self.unknown_element_model(&target.ty, (&target.residency, &[]))).await }?);
                    }
                    for projection in paths {
                        let mut fresh = match value {
                            Some(value) => self.capture_element_model(target, projection, value.clone(),presence.as_ref().map(|_|&indices)),
                            None => {
                                let mut current = target.elements.known().unwrap();
                                for &field in projection {
                                    let returns::Returned::OwnedValue(parent) = current else { unreachable!() };
                                    current = parent.payloads[field].as_deref().unwrap();
                                }
                                let mut inactive = Box::new(current.clone());
                                inactive.restrict_owned(&owned_presence::Presence::None);
                                inactive
                            }
                        };
                        let complete=bounds::Condition::constant(unique&&paths.len()==1&&matches!(place.storage(),Place::ApplicationElement{index_range:(0,0),..})).logical(complete_scope.clone(),false).logical(bounds::Condition::comparison(Comparison::Equal,&target.length,&one),false);
                        let truth=complete.truth_in(&self.path);
                        if projection.is_empty(){
                            target.elements=match(std::mem::take(&mut target.elements),truth){
                                (_,Some(true))=>ElementFacts::Known(fresh),
                                (ElementFacts::Known(previous),truth)=>{
                                    let incoming=fresh.clone();let old=previous.clone();
                                    if truth==Some(false){fresh.join(*previous,true,None);}else{let mut common=fresh.clone();common.join(*previous,true,None);fresh.join(*common,true,Some(&complete));}
                                    if let Some(presence) = &presence {
                                        let opposite = presence.clone().not();
                                        fresh.select_presence([(presence, incoming.as_ref()), (&opposite, old.as_ref())]);
                                    }
                                    residency::select_elements(&mut fresh,&incoming,&old,&selected,count);ElementFacts::Known(fresh)
                                },
                                (previous,Some(false))=>previous.common(ElementFacts::Known(fresh)),
                                (previous,None)=>{let incoming=ElementFacts::Known(fresh);incoming.clone().selected(previous.common(incoming),&complete)},
                            };
                            continue;
                        }
                        let mut stored=target.elements.known_mut();
                        let (last,prefix)=projection.split_last().unwrap();
                        for field in prefix{stored=stored.and_then(|stored|match stored{returns::Returned::OwnedValue(parent)=>parent.payloads[*field].as_deref_mut(),_=>None});}
                        let Some(returns::Returned::OwnedValue(parent))=stored else{continue;};
                        let stored=&mut parent.payloads[*last];
                        *stored=match(stored.take(),truth){
                            (_,Some(true))=>Some(fresh),
                            (Some(previous),truth)=>{
                                let incoming=fresh.clone();let old=previous.clone();
                                if truth==Some(false){fresh.join(*previous,true,None);}else{let mut common=fresh.clone();common.join(*previous,true,None);fresh.join(*common,true,Some(&complete));}
                                if let Some(presence) = &presence {
                                    let opposite = presence.clone().not();
                                    fresh.select_presence([(presence, incoming.as_ref()), (&opposite, old.as_ref())]);
                                }
                                    residency::select_elements(&mut fresh,&incoming,&old,&selected,count);Some(fresh)
                            },
                            (None,_)=>None,
                        };
                    }
                }
            }
            self.env.insert(name,binding);
        }
        Ok(())
    }
    fn owned_element_projections(ty:&Ty,wanted:&Ty,words:&[u32],indexed:bool,offset:u32,path:&mut Vec<usize>,into:&mut Vec<Vec<usize>>){
        let actual=ty.components.iter().map(|(_,word)|offset+word).collect::<Vec<_>>();
        if ty==wanted && (if indexed{actual.iter().all(|word|words.contains(word))}else{actual==words}){into.push(path.clone());return;}
        if !ty.owns_resources() || ty.has_variants(){return;}
        for field in 0..ty.aggregate_len(){let(at,child)=ty.aggregate_part(field);if child.owns_resources(){path.push(field);Self::owned_element_projections(child,wanted,words,indexed,offset+at,path,into);path.pop();}}
    }

    pub(super) fn task_dataset_output_reference(&self, target: &view::Borrow, ty: &Ty) -> Option<DataReference> {
        let target = target.restricted(&self.path);
        for source in &target.sources {
            let view::Source::Position(origin) = source else { continue; };
            let address::Root::Resource { borrow, index: None } = &origin.root else { continue; };
            for binding in self.env.values() {
                for index in 0..binding_element_count(binding) {
                    let Some(data) = binding_element_data(binding, index) else { continue; };
                    for (fields, _, _) in self.application_element_scopes(&[], borrow, data, (&self.path, &self.env)) {
                        let data = fields.iter().try_fold(data, |data, path| model_data(data.elements.known(), path));
                        let Some(data) = data else { continue; };
                        if address::Layout::of(&data.ty) != origin.base { continue; }
                        let mut projections = Vec::new();
                        Self::owned_element_projections(&data.ty, ty, &origin.words, false, 0, &mut Vec::new(), &mut projections);
                        if let Some(projection) = projections.into_iter().next() {
                            let mut reference = DataReference::direct(data.clone());
                            reference.sources.clear();
                            reference.projection = projection;
                            reference.data.root.proof.set_view_borrow(0, borrow.clone());
                            return Some(reference);
                        }
                    }
                }
            }
        }
        None
    }

    fn application_owned_references(&self,place:&Place)->Vec<(DataReference,Vec<Vec<usize>>)>{
        let Place::ApplicationElement{sources,resource,words,..}=place.storage()else{unreachable!()};
        let mut references = Vec::new();
        for binding in self.env.values() {
            let mut collect = |data: &Data| {
                for (scope, _, _) in self.application_element_scopes(sources, resource, data, (&self.path, &self.env)) {
                    let mut data = data;
                    for fields in scope {
                        data = model_data(data.elements.known(), &fields).unwrap();
                    }
                    let mut paths = Vec::new();
                    Self::owned_element_projections(&data.ty, place.ty(), words, matches!(place, Place::Indexed{..}), 0, &mut Vec::new(), &mut paths);
                    if !paths.is_empty() {
                        let mut data = data.clone();
                        data.root.proof.set_view_borrow(0, resource.clone());
                        references.push((DataReference::direct(data), paths));
                    }
                }
            };
            match binding {
                Binding::DataReference(reference) => collect(&reference.data),
                _ => view::visit_data(binding, collect),
            }
        }
        references
    }

    pub(super) fn borrowed_element_storage(&self, place: &Place) -> bool {
        let Place::ApplicationElement { sources, resource, .. } = place.storage() else {
            return false;
        };
        self.env.values().any(|binding| match binding {
            Binding::ApplicationData(data)
                if data.ownership == Ownership::Borrowed && data.access.writes()
                    && matches!(data.layout, DataLayout::Parameter { .. }) =>
            {
                !self.application_element_scopes(sources, resource, data, (&self.path, &self.env)).is_empty()
            }
            _ => false,
        })
    }

    pub(super) async fn application_owned_argument(&mut self, place: &Place, read: bool, span: Span) -> Result<returns::Returned> {
        let loaded = if read { Some(unsafe { self.world.call(self.read_storage_value(place, span, None)).await }?) } else { None };
        let references=self.application_owned_references(place);let mut values=Vec::new();
        for(mut data,paths)in references{for path in paths{data.projection=path;values.push(unsafe { self.world.call(self.materialize_element(&data,loaded.clone(),place)).await }?);}}
        let mut models=values.into_iter();
        let mut model=models.next().unwrap();for other in models{model.join(other,false,None);}
        owned_borrows::set_storage(&mut model, place.clone());
        Ok(model)
    }
    pub(super) async fn assign_application_owned_argument(&mut self,place:&Place,value:Option<&returns::Returned>)->Result<()> {
        for(data,paths)in self.application_owned_references(place){unsafe { self.world.call(self.assign_application_element_model(&data,place,&paths,value)).await }?;}
        Ok(())
    }
    fn capture_element_model(
        &self,data:&Data,projection:&[usize],mut model:returns::Returned,
        indices:Option<&BTreeMap<ValueIdentity,ValueIdentity>>,
    )->Box<returns::Returned>{
        model.bind_outputs(None,&BTreeMap::new(),&bounds::Clause::default());
        let value=self.returned_storage(&model);let mut bindings=self.element_owner_bindings(data);
        // The stored model uses the destination ordinal on the selected write arm.
        for (&source,&ordinal) in indices.into_iter().flatten(){
            let proof=bindings.entry(source).or_default();
            *proof=std::mem::take(proof).with_identity(ordinal);
        }
        let mut components=(0..data.ty.components.len()).collect::<Vec<_>>();let mut ty=&data.ty;
        for field in projection{let(offset,next)=ty.aggregate_part(*field);components=next.components.iter().map(|(_,at)|components[ty.components.iter().position(|(_,word)|*word==offset+at).unwrap()]).collect();ty=next;}
        for (component,stored) in components.iter().enumerate(){
            let id=ValueIdentity::ElementComponent(*stored);let proof=bounds::Proof::default().with_identity(id);
            bindings.insert(id,proof.clone());
            for source in value.component_origins(component){bindings.entry(source).or_insert_with(||proof.clone());}
        }
        for fields in view::resource_paths(&value.ty){if let Some(resource)=model_data(Some(&model),&fields){
            let(mut offset,mut ty)=(0,&value.ty);
            for field in fields{let(at,next)=if ty.has_variants(){(ty.variant_payload_offset(),ty.variants[field].ty.as_ref())}else{ty.aggregate_part(field)};offset+=at;ty=next;}
            let component=value.ty.components.iter().position(|(_,word)|*word==offset).unwrap();
            for source in resource.root.component_origins(0){
                let proof=bindings.entry(source).or_default();
                *proof=std::mem::take(proof).with_identity(ValueIdentity::ElementComponent(components[component]));
            }
        }}
        let original=model.clone();native_function::rebind_model(&mut model,components);
        native_function::translate_returned(&mut model,&original,&bindings,true);
        if let Some(value)=model.view_value_mut(){
            value.proof.origins.resize_with(value.regs.len(),Vec::new);
            for(component,origins)in value.proof.origins.iter_mut().enumerate(){
                let id=ValueIdentity::ElementComponent(value.regs[component]);
                if !origins.contains(&id){origins.push(id);}
            }
        }
        self.close_stored_resources(&mut model,&original,&bindings);Box::new(model)
    }
    pub(super) async fn materialize_element(&mut self,data:&DataReference,value:Option<Value>,place:&Place)->Result<returns::Returned>{
        let mut model=*unsafe { self.world.call(self.element_model(&data.ty,&data.elements,(&data.residency,&[]),self.scope.span())).await }?;let mut components=(0..data.ty.components.len()).collect::<Vec<_>>();
        for field in &data.projection{
            let returns::Returned::OwnedValue(owner)=model else{unreachable!()};
            let(offset,ty)=owner.value.ty.aggregate_part(*field);
            components=ty.components.iter().map(|(_,at)|components[owner.value.ty.components.iter().position(|(_,word)|*word==offset+at).unwrap()]).collect();
            model=self.unpack_record(&owner,*field);
        }
        let original=model.clone();let storage=self.returned_storage(&model);let mut bindings=self.element_owner_bindings(data);
        let mut selected=Some(place);let mut depth=0;let mut parent_indices=BTreeMap::new();
        while let Some(Place::ApplicationElement{index_value,index_range,parent,..})=selected.map(Place::storage){
            let proof=bounds::Proof::element_operand(*index_value,*index_range);
            if depth!=0{parent_indices.insert(ValueIdentity::ElementIndex(depth-1),proof.clone());}
            bindings.insert(ValueIdentity::ElementIndex(depth),proof);
            selected=parent.as_deref();depth+=1;
        }
        let registers = if let Some(value) = value {
            for(component,index)in components.into_iter().zip(0..value.regs.len()){
                let loaded=self.value_component(&value,index);
                let proof=self.path_proof(&loaded.ty,bounds::Proof::capture(&loaded));
                bindings.insert(ValueIdentity::ElementComponent(component),proof.clone());
                for source in storage.component_origins(index).into_iter().filter(|source|matches!(source,ValueIdentity::DatasetContent(..))){
                    bindings.insert(source,{let mut component_proof=proof.clone();component_proof.set_view_borrow(0,storage.proof.view_borrow(index));component_proof});
                }
            }
            value.regs
        } else {
            storage.regs
        };
        self.bind_element_resource_borrows(data,&self.data_value_borrow(data),&mut bindings);
        native_function::rebind_model(&mut model,registers);
        native_function::translate_returned(&mut model,&original,&bindings,true);
        let borrow=self.data_value_borrow(&data.data);
        owned_borrows::each_data(&mut model,&mut Vec::new(),&mut|child,path|{
            let mut fields=data.projection.clone();fields.extend_from_slice(path);
            let index=match place.storage(){Place::ApplicationElement{index_value,..}=>Some(*index_value),_=>None};
            if data.elements.known().is_none(){
                let path=std::iter::once(residency::Part::Selected(index)).chain(fields.iter().copied().map(residency::Part::Field)).collect::<Vec<_>>();
                // Data's stored parent indices exclude the element being loaded now.
                child.residency=data.residency.element_indices(&parent_indices).project(&path,&bindings);
            }
            child.root.proof.set_view_borrow(0,child.root.proof.view_borrow(0).joined(view::Borrow::element(borrow.clone(),fields,index)));
        });
        // Data.elements uses the same logical ordinal slots as its value model.
        // Bind those selections at the actual load before native cleanup consumes them.
        fn bind_presence(when:&owned_presence::Presence,indices:&[ownership::ApplicationIndex],depth:usize)->owned_presence::Presence{
            use owned_presence::Presence;
            let bind=|index:&ownership::ApplicationIndex|match index{
                ownership::ApplicationIndex::Value(ValueIdentity::ElementIndex(at),_)if *at>=depth=>indices[*at-depth],
                index=>*index,
            };
            match when{
                Presence::Selected(operator,a,b)=>Presence::indices(*operator,bind(a),bind(b)),
                Presence::Not(a)=>bind_presence(a,indices,depth).not(),
                Presence::Both(a,b)=>bind_presence(a,indices,depth).and(bind_presence(b,indices,depth)),
                Presence::Either(a,b)=>bind_presence(a,indices,depth).or(bind_presence(b,indices,depth)),
                when=>when.clone(),
            }
        }
        fn bind_model(model:&mut returns::Returned,indices:&[ownership::ApplicationIndex],depth:usize){
            let bind=|mode:&mut Ownership|if let Ownership::Conditional(when)=mode{*when=Box::new(bind_presence(when,indices,depth));};
            match model{
                returns::Returned::OwnedValue(value)=>{bind(&mut value.ownership);for child in value.payloads.iter_mut().flatten(){bind_model(child,indices,depth);}},
                returns::Returned::Dataset(data)=>{bind(&mut data.ownership);data.elements=std::mem::take(&mut data.elements).map(&mut|mut model|{bind_model(&mut model,indices,depth+1);model},&mut|when|when);},
                _=>{},
            }
        }
        let mut indices=Vec::new();let mut current=Some(place);
        while let Some(Place::ApplicationElement{index,index_value,parent,..})=current.map(Place::storage){
            indices.push(match index_value{bounds::Operand::Constant(_,at)=>ownership::ApplicationIndex::Constant(*at as u32),bounds::Operand::Value(id)=>ownership::ApplicationIndex::Value(*id,*index)});current=parent.as_deref();
        }
        bind_model(&mut model,&indices,0);self.resolve_owned_model(&mut model,None);
        Ok(model)
    }
    pub(super) async fn application_cursor_input(&mut self,data:&DataReference,index:&Value)->Result<bounds::Proof> {
        let model=unsafe { self.world.call(self.element_model(&data.data.ty,&data.data.elements,(&data.data.residency,&[]),self.scope.span())).await }?;
        let value=self.returned_storage(&model);let mut bindings=self.element_owner_bindings(&data.data);
        for(component,(scalar,_))in value.ty.components.iter().enumerate(){
            let input=bounds::Proof::default().with_identity(ValueIdentity::Register(self.reg(*scalar)));
            bindings.insert(ValueIdentity::ElementComponent(component),input.clone());
            for source in value.component_origins(component).into_iter().filter(|source|matches!(source,ValueIdentity::DatasetContent(..))){bindings.insert(source,input.clone());}
        }
        bindings.insert(ValueIdentity::ElementIndex(0),bounds::Proof::capture(index));
        let components=data.words().iter().map(|word|data.data.ty.components.iter().position(|(_,at)|at==word).unwrap()).collect::<Vec<_>>();
        let mut proof=value.proof.project_numeric_at_call(&components,value.regs.len(),&bindings);
        for(output,component)in components.iter().enumerate(){
            for source in value.component_origins(*component).into_iter().filter(|source|matches!(source,ValueIdentity::DatasetContent(..))){
                let scalar=value.ty.components[*component].0;proof.mark_domain(output,IndexDomain::CommonRange(source,scalar,scalar,scalar,scalar));
            }
        }
        Ok(proof)
    }
    pub(super) async fn application_element_proof(&mut self,place:&Place,regs:&[Reg],reads:&[(usize,usize)])->Result<Option<bounds::Proof>>{
        let Place::ApplicationElement{sources,resource:borrow,words,..}=place else{return Ok(None)};let mut result=None;
        let mut projections:Vec<Vec<view_contracts::ElementRead>>=Vec::new();let mut pending=false;
        let names=self.env.keys().cloned().collect::<Vec<_>>();
        for name in names{
            let mut binding=self.env[&name].clone();
            for resource in 0..binding_element_count(&binding){
                if let Some(data)=binding_element_data_mut(&mut binding,resource)
                    && self.resolve_ownership(&data.ownership,None).available(){
                    for(scope,_,_)in self.application_element_scopes(sources,borrow,data,(&self.path,&self.env)){
                        let mut data=&mut *data;let mut parent=self.data_value_borrow(data);
                        for fields in scope{
                            unsafe { self.world.call(self.expand_current_element_facts(data)).await }?;parent=view::Borrow::element(parent,fields.clone(),None);
                            let Some(child)=model_data_mut(data.elements.known_mut(),&fields)else{return Ok(None)};data=child;
                        }
                        unsafe { self.world.call(self.expand_current_element_facts(data)).await }?;
                        let Some(components)=words.iter().map(|word|data.ty.components.iter().position(|(_,at)|at==word)).collect::<Option<Vec<_>>>()else{return Ok(None)};
                        let Some(model)=data.elements.known()else{return Ok(None)};
                        let value=self.returned_storage(model);let mut bindings=self.element_owner_bindings(data);
                        for(component,reg)in components.iter().zip(regs){
                            let proof=bounds::Proof::default().with_identity(ValueIdentity::Register(*reg));
                            bindings.insert(ValueIdentity::ElementComponent(*component),proof.clone());
                            for source in value.component_origins(*component).into_iter().filter(|source|matches!(source,ValueIdentity::DatasetContent(..)|ValueIdentity::MemoryValue(..)|ValueIdentity::LoopValue(..))){
                                bindings.insert(source,{let mut component_proof=proof.clone();component_proof.set_view_borrow(0,value.proof.view_borrow(*component));component_proof});
                            }
                        }
                        let mut selected=Some(place);let mut depth=0;
                        while let Some(Place::ApplicationElement{index_value,index_range,parent,..})=selected.map(Place::storage){
                            bindings.insert(ValueIdentity::ElementIndex(depth),bounds::Proof::element_operand(*index_value,*index_range));
                            selected=parent.as_deref();depth+=1;
                        }
                        self.bind_element_resource_borrows(data,&parent,&mut bindings);
                        let mut proof=value.proof.project(&components,value.regs.len()).at_call(&bindings);
                        if !reads.is_empty() {
                            pending=pending||value.proof.any_scalar_identity(&|source|matches!(source,ValueIdentity::MemoryValue(..)|ValueIdentity::LoopValue(..)));
                            let bindings=Arc::new(bindings);
                            if projections.is_empty(){projections.resize_with(regs.len(),Vec::new);}
                            for(target,component)in components.iter().enumerate(){
                                projections[target].push(view_contracts::ElementRead{scalar:value.ty.components[*component].0,proof:value.proof.project(&[*component],value.regs.len()),bindings:bindings.clone()});
                            }
                        }
                        for(target,component)in components.iter().enumerate(){
                            for source in value.component_origins(*component).into_iter().filter(|source|matches!(source,ValueIdentity::DatasetContent(..))){
                                let scalar=value.ty.components[*component].0;proof.mark_domain(target,IndexDomain::CommonRange(source,scalar,scalar,scalar,scalar));
                            }
                        }
                        if let Some(previous)=&mut result{bounds::Proof::join(previous,&proof,regs.len(),None);}else{result=Some(proof);}
                    }
                }
            }
            self.env.insert(name,binding);
        }
        if pending && let Some(proof)=&mut result {
            let mut projections=projections.into_iter().map(|values|(!values.is_empty()).then(||Arc::<[view_contracts::ElementRead]>::from(values))).collect::<Vec<_>>();
            for &(component,usage) in reads {
                if let (Some(values),view::Requirement::Dataset{values:target,..})=(projections.get_mut(component).and_then(Option::take),&mut self.view_model.uses[usage].requirement) {
                    *target=Some(values);*proof=std::mem::take(proof).with_component_identity(component,ValueIdentity::MemoryValue(usage,0));
                }
            }
        }
        Ok(result)
    }
    pub(super) async fn update_application_elements(&mut self,sources:&[ValueIdentity],resource:&view::Borrow,words:&[u32],update:ElementUpdate<'_>)->Result<()> {
        let names=self.env.keys().cloned().collect::<Vec<_>>();
        let index=match &update{ElementUpdate::Element{index,..}|ElementUpdate::Bytes{index,..}=>*index,_=>None};
        let when=match &update{ElementUpdate::Element{when,..}=>*when,_=>None};
        let range=index.map(|(_,range)|range);
        let one=(range==Some((0,0))).then(||self.constant(Scalar::U32,1));
        for name in names{
            let mut binding=self.env[&name].clone();
            for resource_index in 0..binding_element_count(&binding){
                let Some(data)=binding_element_data_mut(&mut binding,resource_index)else{continue;};
                let scopes=self.application_element_scopes(sources,resource,data,(&self.path,&self.env));let unique=scopes.len()==1;
                for(scope,complete_scope,parents)in scopes {
                let selected=(unique&&complete_scope).then(||parents.scope_element_indices(1).logical(match index{
                    Some((index,_))=>bounds::Condition::element_index(0,self.element_index_operand(index,(&self.path,&self.env))),
                    None=>bounds::Condition::default(),
                },false));
                let selected=if let Some(when)=when{Some(selected.unwrap_or_default().logical(when.clone(),false))}else{selected};
                let mut complete_scope=bounds::Condition::constant(complete_scope);let mut selected_data=Some(&mut *data);
                for fields in scope{
                    selected_data=match selected_data{
                        Some(data)=>{unsafe { self.world.call(self.expand_current_element_facts(data)).await }?;if let Some(one)=&one{complete_scope=complete_scope.logical(bounds::Condition::comparison(Comparison::Equal,&data.length,one),false);}model_data_mut(data.elements.known_mut(),&fields)},
                        None=>None,
                    };
                }
                let Some(data)=selected_data else{continue;};unsafe { self.world.call(self.expand_current_element_facts(data)).await }?;
                let byte_writes=if let ElementUpdate::Bytes{effect,..}=&update {
                    let borrow=Arc::new(resource.clone());let layout=address::Layout::of(&data.ty);
                    data.ty.components.iter().enumerate().filter_map(|(component,(scalar,word))|{
                        let position=Position::Borrowed(borrow.clone(),layout.clone(),*word);let bytes=effect.written_bytes(&position,*scalar,None);
                        (bytes!=0).then_some((component,bytes))
                    }).collect::<Vec<_>>()
                }else{Vec::new()};
                let components=if matches!(update,ElementUpdate::Bytes{..}){byte_writes.iter().map(|(component,_)|*component).collect::<Vec<_>>()}else{words.iter().filter_map(|word|data.ty.components.iter().position(|(_,at)|at==word)).collect::<Vec<_>>()};
                if components.is_empty(){continue;}
                data.prefixes.retain(|word,_|!components.iter().any(|component|data.ty.components[*component].1==*word));
                let Some(elements)=data.elements.known()else{continue;};
                let old=self.returned_storage(elements);let mut updated=old.clone();updated.constant=None;
                let mut bindings=element_scope_bindings(&self.element_owner_bindings(data),old.regs.len(),false);
                for component in 0..old.regs.len(){if !components.contains(&component){for id in old.component_origins(component){
                    if !matches!(id,ValueIdentity::ElementComponent(at) if components.contains(&at)){
                        bindings.entry(id).or_insert_with(||bounds::Proof::default().with_identity(id));
                    }
                }}}
                let previous=bounds::Proof::capture(&old);
                for id in previous.scalar_identities().into_iter().filter(|id|matches!(id,ValueIdentity::ElementIndex(_))){
                    bindings.insert(id,bounds::Proof::default().with_identity(id));
                }
                updated.proof=previous.at_call(&bindings);
                let written=if let ElementUpdate::Element{proof:fresh,..}=&update{
                    let mut bindings=self.element_owner_bindings(data);
                    for(index,component)in components.iter().enumerate(){
                        let proof=fresh.project(&[index],components.len());
                        if proof.known_identity(0).is_some(){
                            let value=Value{ty:Ty::scalar(data.ty.components[*component].0),regs:vec![0],proof,uniform:false,literal:false,constant:None};
                            for id in value.component_origins(0){bindings.entry(id).or_insert_with(||bounds::Proof::default().with_identity(ValueIdentity::ElementComponent(*component)));}
                        }
                    }
                    fresh.at_call(&bindings)
                }else if let ElementUpdate::Bytes{effect,..}=&update{
                    let mut proof=bounds::Proof::default();
                    let borrow=Arc::new(resource.clone());let layout=address::Layout::of(&data.ty);
                    for(target,(component,_))in byte_writes.iter().enumerate(){
                        let(scalar,word)=data.ty.components[*component];let position=Position::Borrowed(borrow.clone(),layout.clone(),word);
                        proof.assign(&[target],components.len(),effect.written_value(&position,scalar,None,&previous.project(&[*component],old.regs.len())));
                    }
                    proof
                }else{bounds::Proof::default()};
                updated.proof.assign(&components,old.regs.len(),written);
                if let ElementUpdate::WholeFacts(proof)=&update {
                    updated.proof.assign_elements(&components,old.regs.len(),proof);
                }
                let complete=match &update {
                    ElementUpdate::WholeFacts(_)=>bounds::Condition::constant(unique).logical(complete_scope,false),
                    _=>match &one{Some(one)=>bounds::Condition::constant(unique).logical(complete_scope,false).logical(bounds::Condition::comparison(Comparison::Equal,&data.length,one),false),None=>bounds::Condition::constant(false)},
                };
                let complete=if let Some(when)=when{complete.logical(when.clone(),false)}else{complete};
                let truth=complete.truth_in(&self.path);
                if truth!=Some(true){
                    let mut proof=updated.proof.project(&components,old.regs.len());
                    proof.join_condition(&previous.project(&components,old.regs.len()),components.len(),selected.as_ref());
                    updated.proof.assign(&components,old.regs.len(),proof);
                }
                updated.proof.origins.resize_with(updated.regs.len(),Vec::new);
                for(component,origins)in updated.proof.origins.iter_mut().enumerate(){
                    let identity=ValueIdentity::ElementComponent(updated.regs[component]);
                    if !origins.contains(&identity){origins.push(identity);}
                }
                updated.proof.retain_view_borrows(&updated.ty);
                match data.elements.known_mut(){
                    Some(returns::Returned::Value(value))=>**value=updated.clone(),
                    Some(returns::Returned::OwnedValue(value))=>value.value=updated.clone(),
                    Some(returns::Returned::Dataset(_)|returns::Returned::Unit)|None=>{},
                }
                for path in view::resource_paths(&updated.ty){
                    if let Some(inner)=model_data_mut(data.elements.known_mut(),&path){
                        let handle=inner.root.regs[0];let length=inner.length.regs[0];
                        let borrow=inner.root.proof.view_borrow(0);
                        inner.root=self.value_component(&updated,handle);inner.length=self.value_component(&updated,length);
                        inner.root.proof={let mut component_proof=std::mem::take(&mut inner.root.proof).with_identity(ValueIdentity::ElementComponent(handle));component_proof.set_view_borrow(0,borrow);component_proof};
                        if components.contains(&handle)||components.contains(&length){
                            let original=unsafe { self.world.call(self.element_model(&inner.ty,&inner.elements,(&inner.residency,&[]),self.scope.span())).await }?;
                            let mut elements=unsafe { self.world.call(self.unknown_element_model(&inner.ty,(&inner.residency,&[]))).await }?;
                            residency::visit(&mut elements,&mut residency::Input::new(0),&mut|data,input|{
                                if let Some(source)=residency::model_location(&original,&input.fields){data.residency=source.residency.clone();}
                            });
                            inner.elements=ElementFacts::Known(elements);
                        }
                    }
                }
                }
            }
            self.env.insert(name,binding);
        }
        Ok(())
    }
    fn projected_element_model(&mut self, model: &returns::Returned, projection: &[usize]) -> returns::Returned {
        projection.iter().fold(model.clone(), |model, field| match model {
            returns::Returned::OwnedValue(parent) => self.unpack_record(&parent, *field),
            model => returns::Returned::value(self.record_component(&self.returned_storage(&model), *field)),
        })
    }
    fn replace_element_model(&self, stored: &mut returns::Returned, projection: &[usize], fresh: returns::Returned) {
        let Some((field, rest)) = projection.split_first() else { *stored = fresh; return; };
        let incoming = self.returned_storage(&fresh);
        let value = stored.view_value_mut().unwrap();
        let components = incoming.regs.iter().map(|reg| value.regs.iter().position(|parent| parent == reg).unwrap()).collect::<Vec<_>>();
        value.proof = bounds::Proof::capture(value);
        value.proof.assign(&components, value.regs.len(), bounds::Proof::capture(&incoming));
        value.constant = value.proof.known_value(&value.ty);
        value.uniform &= incoming.uniform;
        if let returns::Returned::OwnedValue(parent) = stored
            && parent.value.ty.aggregate_part(*field).1.owns_resources() {
            self.replace_element_model(parent.payloads[*field].as_deref_mut().unwrap(), rest, fresh);
        }
    }
    pub(super) async fn apply_application_prefixes<'a>(
        &mut self, reference: &DataReference, facts: impl Iterator<Item = (u32, &'a view::Borrow)> + Clone,
    ) -> Result<()> {
        let resource=view::resource_borrow(&reference.data);let names=self.env.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let mut binding=self.env[&name].clone();
            for index in 0..binding_element_count(&binding){
                let Some(data)=binding_element_data_mut(&mut binding,index)else{continue;};
                let scopes=self.application_element_scopes(&reference.sources,&resource,data,(&self.path,&self.env));let unique=scopes.len()==1;
                for(scope,mut complete,_)in scopes {
                    let mut data=&mut *data;
                    for fields in scope {
                        unsafe { self.world.call(self.expand_current_element_facts(data)).await }?;
                        complete&=data.length.minimum()==1&&data.length.maximum()==1;
                        data=model_data_mut(data.elements.known_mut(),&fields).unwrap();
                    }
                    let when=bounds::Condition::constant(unique&&complete).assume(&bounds::Clause::default(),true);
                    for (word, fact) in facts.clone() {
                        data.prefixes.entry(word).or_default().union_restricted(fact, &when);
                    }
                }
            }
            self.env.insert(name,binding);
        }
        Ok(())
    }
    pub(super) async fn apply_application_dataset_model(&mut self,reference:&DataReference,model:ElementFacts)->Result<()>{
        let resource=view::resource_borrow(&reference.data);let names=self.env.keys().cloned().collect::<Vec<_>>();
        for name in names{
            let mut binding=self.env[&name].clone();
            for index in 0..binding_element_count(&binding){
                let Some(data)=binding_element_data_mut(&mut binding,index)else{continue;};
                let scopes=self.application_element_scopes(&reference.sources,&resource,data,(&self.path,&self.env));let unique=scopes.len()==1;
                for(scope,mut complete,_)in scopes{
                    let count=scope.len()+1;
                    let qualifier=if unique&&complete{bounds::Condition::constant(true)}else{bounds::Condition::default()};
                    let selection=self.element_selection(reference.data.place.as_ref(),1,scope.len(),|_,_|{});
                    let selected=qualifier.logical(match selection{Some(selection)=>selection.condition(),None=>bounds::Condition::default()},false);
                    let mut selected_data=Some(&mut *data);
                    for fields in scope{selected_data=match selected_data{
                        Some(data)=>{unsafe { self.world.call(self.expand_current_element_facts(data)).await }?;complete&=data.length.minimum()==1&&data.length.maximum()==1;model_data_mut(data.elements.known_mut(),&fields)},
                        None=>None,
                    };}
                    let Some(data)=selected_data else{continue;};
                    if reference.projection.is_empty()&&unique&&complete{
                        data.elements=model.clone().map(&mut |model|self.capture_element_model(data,&reference.projection,*model,None),&mut |when|when);continue;
                    }
                    let incoming=unsafe { self.world.call(self.element_model(reference.element_layout().1,&model,(&reference.residency,&reference.projection),self.scope.span())).await }?;let model=incoming.as_ref();
                    let mut fresh=self.capture_element_model(data,&reference.projection,model.clone(),None);
                    if reference.projection.is_empty()&&unique&&complete{data.elements=application::ElementFacts::Known(fresh);continue;}
                    // A real partial write supplies new facts; only its enclosing model is materialized.
                    // Invalidating absent facts above does not manufacture an unknown element tree.
                    if data.elements.known().is_none(){data.elements=application::ElementFacts::Known(unsafe { self.world.call(self.element_model(&data.ty,&data.elements,(&data.residency,&[]),self.scope.span())).await }?);}
                    let stored=data.elements.known_mut().unwrap();
                    if !unique||!complete{
                        let incoming=fresh.clone();let previous=self.projected_element_model(stored,&reference.projection);
                        fresh.join(previous.clone(),true,None);
                        residency::select_elements(&mut fresh,&incoming,&previous,&selected,count);
                    }
                    self.replace_element_model(stored,&reference.projection,*fresh);
                }
            }
            self.env.insert(name,binding);
        }
        Ok(())
    }
}

// Each stored model is checked in its own existing element scope. Its local
// identities never become resources in an outer function's retained set.
impl Builder<'_> {
    pub(super) async fn require_element_return_borrows(&mut self,model:&returns::Returned,owned:&view::Borrow,span:Span)->Result<()> {
        async fn visit(builder:&mut Builder<'_>,model:&returns::Returned,owned:&view::Borrow,parents:&[usize],span:Span)->Result<()> {
            let value=builder.returned_storage(model);
            for path in view::resource_paths(&value.ty) {
                if let Some(data)=model_data(Some(model),&path){unsafe { builder.world.call(check(builder,data,owned,parents,span)).await }?;}
            }
            Ok(())
        }
        async fn check(builder:&mut Builder<'_>,data:&Data,owned:&view::Borrow,parents:&[usize],span:Span)->Result<()> {
            if !data.ownership.may_own()||data.length.maximum()==0{return Ok(());}
            let mut shifts=BTreeMap::new();
            for(depth,width)in parents.iter().enumerate(){for component in 0..*width {
                let id=if depth==0{ValueIdentity::ElementComponent(component)}else{ValueIdentity::ElementParent(depth-1,component)};
                shifts.insert(id,ValueIdentity::ElementParent(depth,component));
            }}
            let mut scoped=owned.output_bound(&shifts);
            let Some(elements)=data.elements.known()else{
                if unsafe { builder.world.call(builder.world.stored_borrows(&data.ty,builder.scope.source(),span,builder.scope.module(),builder.scope.imports(),false)).await }? && !matches!(data.elements,ElementFacts::Input(_)){
                    let proof={let mut component_proof=bounds::Proof::default();component_proof.set_view_borrow(0,view::Borrow::source(view::Source::Unresolved));component_proof};
                    builder.require_view_return_proof(&proof,scoped,span)?;
                }
                return Ok(());
            };
            for resource in view_contracts::resource_borrows(elements){scoped.union(&resource);}
            if let Some(value)=data.elements.known().and_then(returns::Returned::view_value) && value.ty.contains_borrows(){
                let mut proof=value.proof.borrow_state();proof.retain_view_borrows(&value.ty);
                builder.require_view_return_proof(&proof,scoped.clone(),span)?;
            }
            let mut nested=vec![elements.registers().len()];nested.extend_from_slice(parents);
            unsafe { builder.world.call(visit(builder,elements,&scoped,&nested,span)).await }
        }
        unsafe { self.world.call(visit(self,model,owned,&[],span)).await }
    }
}

impl Builder<'_> {
    // The stored root keeps the same resource's physical aliases in its Proof.
    // Element scope names describe that owner; they do not create another one.
    fn close_stored_resources(&self,model:&mut returns::Returned,original:&returns::Returned,bindings:&ElementBindings){
        let names=bindings.iter().filter_map(|(source,proof)|{
            matches!(source,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..))
                .then(||proof.known_identity(0).map(|target|(*source,target))).flatten()
        }).collect::<BTreeMap<_,_>>();
        let value=self.returned_storage(model);
        for path in view::resource_paths(&value.ty){if let(Some(data),Some(source))=(model_data_mut(Some(model),&path),model_data(Some(original),&path)){
            if data.ownership.may_own(){data.root.proof.set_view_borrow(0,view::resource_borrow(source).output_bound(&names));}
            let nested=element_scope_bindings(bindings,source.ty.components.len(),true);
            if let(Some(value),Some(original))=(data.elements.known_mut(),source.elements.known()){self.close_stored_resources(value,original,&nested);}
        }}
    }
}

// 已闭合源码包直接消费本目录的类型／program编号；实际C ABI数组只借用到本次同步调用结束。
pub(super) fn lower_sdk_resources(
    program: &mut Program,
    functions: &mut [FunctionBody],
    resources: &mut crate::application::Resources,
) {
    fn value(reg:Reg,scalar:Scalar)->CallArgument{
        CallArgument::Value{origin:None,dataset_borrow:None,ty:Ty::scalar(scalar),values:vec![reg]}
    }
    fn emit(code:&mut Vec<Inst>,original:&Inst,kind:InstKind){
        code.push(Inst{source_site:original.source_site.clone(),generated_origin:original.generated_origin.clone(),..Inst::new(kind)});
    }
    fn constant(registers:&mut Vec<Scalar>,code:&mut Vec<Inst>,original:&Inst,scalar:Scalar,bits:Bits)->Reg{
        let reg=registers.len();registers.push(scalar);emit(code,original,InstKind::Constant(reg,scalar,bits));reg
    }
    fn record(name:&str,size:u32,align:u32,fields:&[(&str,Scalar,u32)])->Ty{
        let mut ty=Ty::unit();ty.name=name.into();ty.size=size;ty.align=align;
        ty.components=fields.iter().map(|(_,scalar,offset)|(*scalar,*offset)).collect();
        ty.fields=fields.iter().map(|(name,scalar,offset)|(name.to_string(),*offset,Box::new(Ty::scalar(*scalar)))).collect();ty
    }
    fn packet(locals:&mut BTreeMap<usize,Ty>,code:&mut Vec<Inst>,original:&Inst,element:Ty,count:usize,fields:&[Reg])->CallArgument{
        let mut ty=Ty::unit();ty.name=format!("array<{},{}>",element.name,count);ty.size=element.size*count as u32;ty.align=element.align;
        ty.values=vec![count as u32];ty.arguments=vec![element.clone()];
        ty.components=(0..count).flat_map(|i|element.components.iter().map(move |(scalar,offset)|(*scalar,element.size*i as u32+offset))).collect();
        ty.array=Some(Box::new(element));
        let base=locals.last_key_value().map_or(0,|(end,_)|*end);
        let len=ty.components.len();
        // IR局部位置是分量槽；物理字节、填充与对齐仍只由原Ty交给后端。
        if len!=0{locals.insert(base+len,ty.clone());}
        for(slot,&reg)in fields.iter().enumerate(){emit(code,original,InstKind::StoreLocal(base+slot,reg));}
        CallArgument::Reference{origin:None,ty:ty.clone(),access:Access::Read,address:CallReference{
            storage:CallStorage::Local{base,len},offset:None,word:0,
        }}
    }
    fn unsigned_bits(registers:&mut Vec<Scalar>,code:&mut Vec<Inst>,original:&Inst,scalar:Scalar,reg:Reg)->Reg{
        let word=if matches!(scalar,Scalar::F32|Scalar::F64){
            let bits=if scalar==Scalar::F32{Scalar::U32}else{Scalar::U64};
            let result=registers.len();registers.push(bits);emit(code,original,InstKind::Unary{dst:result,op:UnaryOp::Bitcast,a:reg});result
        }else{reg};
        if registers[word]==Scalar::U64{word}else{
            let result=registers.len();registers.push(Scalar::U64);emit(code,original,InstKind::Cast{dst:result,src:word});result
        }
    }
    fn block(registers:&mut Vec<Scalar>,locals:&mut BTreeMap<usize,Ty>,code:&mut Vec<Inst>,resources:&crate::application::Resources){
        let mut out=Vec::new();
        for mut inst in std::mem::take(code){
            match &mut inst.kind{
                InstKind::Scope(body)=>block(registers,locals,body,resources),
                InstKind::If{yes,no,..}=>{block(registers,locals,yes,resources);block(registers,locals,no,resources);},
                InstKind::Loop{condition,body,..}=>{block(registers,locals,condition,resources);block(registers,locals,body,resources);},
                _=>{},
            }
            let symbol=match &inst.kind{
                InstKind::Call(Call{target:CallTarget::External(name),..})if name.library==crate::resource::RUNTIME_LIBRARY=>Some(name.symbol.clone()),
                _=>None,
            };
            if let Some(index)=symbol.as_ref().and_then(|name|name.strip_prefix("blink_allocate_")).and_then(|index|index.parse::<usize>().ok()){
                let ty=constant(registers,&mut out,&inst,Scalar::U32,index as u64);
                if let InstKind::Call(call)=&mut inst.kind{
                    call.arguments.insert(1,value(ty,Scalar::U32));
                    call.target=CallTarget::External(ExternalName{library:crate::resource::RUNTIME_LIBRARY.into(),symbol:"blink_allocate".into()});
                }
            }else if let Some(index)=symbol.as_ref().and_then(|name|name.strip_prefix("blink_run_")).and_then(|index|index.parse::<usize>().ok()){
                let mut inputs=Vec::new();
                if let InstKind::Call(call)=&inst.kind{call.all_operands(&mut |reg|{inputs.push(reg);true});}
                let program=&resources.programs[index].1;
                let fields:Vec<_>=(0..program.buffers.len()*2).map(|i|inputs[2+i]).collect();
                let binding=record("@blink.DatasetBinding",8,4,&[("handle",Scalar::U32,0),("offset",Scalar::U32,4)]);
                let data=packet(locals,&mut out,&inst,binding,program.buffers.len(),&fields);
                let mut fields=Vec::new();let mut at=2+program.buffers.len()*2;
                for scalar in program.uniforms.iter().flat_map(|uniform|uniform.ty.components.iter().map(|(scalar,_)|*scalar)){
                    let kind=constant(registers,&mut out,&inst,Scalar::U32,crate::resource::scalar_tag(scalar)as u64);
                    let bits=unsigned_bits(registers,&mut out,&inst,scalar,inputs[at]);at+=1;
                    fields.extend([kind,bits]);
                }
                let uniform=record("@blink.UniformValue",16,8,&[("kind",Scalar::U32,0),("bits",Scalar::U64,8)]);
                let words=packet(locals,&mut out,&inst,uniform,fields.len()/2,&fields);
                let textures=packet(locals,&mut out,&inst,Ty::scalar(Scalar::U64),program.textures.len(),&inputs[at..]);
                let program=constant(registers,&mut out,&inst,Scalar::U32,index as u64);
                if let InstKind::Call(call)=&mut inst.kind{
                    call.arguments=vec![call.arguments[0].clone(),value(program,Scalar::U32),data,words,textures];
                    call.target=CallTarget::External(ExternalName{library:crate::resource::RUNTIME_LIBRARY.into(),symbol:"blink_run".into()});
                }
            }else if let Some(index)=symbol.as_ref().and_then(|name|name.strip_prefix("blink_complete_")).and_then(|index|index.parse::<usize>().ok()){
                let mut inputs=Vec::new();
                if let InstKind::Call(call)=&inst.kind{call.all_operands(&mut |reg|{inputs.push(reg);true});}
                for field in &resources.completions[index]{
                    let mut values=Vec::new();
                    for(lower,upper)in &field.intervals{
                        let lower=constant(registers,&mut out,&inst,Scalar::U64,*lower);let upper=constant(registers,&mut out,&inst,Scalar::U64,*upper);
                        values.extend([lower,upper]);
                    }
                    let interval=record("@blink.ElementInterval",16,8,&[("lower",Scalar::U64,0),("upper",Scalar::U64,8)]);
                    let ranges=packet(locals,&mut out,&inst,interval,field.intervals.len(),&values);
                    let base=locals.last_key_value().map_or(0,|(end,_)|*end);
                    let bounds=packet(locals,&mut out,&inst,Ty::scalar(Scalar::U32),field.limits.len(),&[]);
                    let mut count=constant(registers,&mut out,&inst,Scalar::U32,0);
                    let one=constant(registers,&mut out,&inst,Scalar::U32,1);
                    let stride=constant(registers,&mut out,&inst,Scalar::U32,4);
                    for(parameter,distance)in &field.limits{
                        let source=inputs[4+parameter];
                        let(limit,increment)=match *distance{
                            0=>{
                                let maximum=constant(registers,&mut out,&inst,Scalar::U32,u32::MAX as u64);
                                let valid=registers.len();registers.push(Scalar::Bool);
                                emit(&mut out,&inst,InstKind::Binary{dst:valid,op:ir::BinaryOp::Compare(Comparison::NotEqual),a:source,b:maximum});
                                let increment=registers.len();registers.push(Scalar::U32);
                                emit(&mut out,&inst,InstKind::Cast{dst:increment,src:valid});
                                let limit=registers.len();registers.push(Scalar::U32);
                                emit(&mut out,&inst,InstKind::Binary{dst:limit,op:ir::BinaryOp::IAdd,a:source,b:one});
                                (limit,increment)
                            },
                            1=>(source,one),
                            distance=>{
                                let distance=constant(registers,&mut out,&inst,Scalar::U32,(distance-1)as u64);
                                let smaller=registers.len();registers.push(Scalar::Bool);
                                emit(&mut out,&inst,InstKind::Binary{dst:smaller,op:ir::BinaryOp::Compare(Comparison::Less),a:source,b:distance});
                                let removed=registers.len();registers.push(Scalar::U32);
                                emit(&mut out,&inst,InstKind::Select{dst:removed,condition:smaller,a:source,b:distance});
                                let limit=registers.len();registers.push(Scalar::U32);
                                emit(&mut out,&inst,InstKind::Binary{dst:limit,op:ir::BinaryOp::ISub,a:source,b:removed});
                                (limit,one)
                            },
                        };
                        let offset=registers.len();registers.push(Scalar::U32);
                        emit(&mut out,&inst,InstKind::Binary{dst:offset,op:ir::BinaryOp::IMul,a:count,b:stride});
                        emit(&mut out,&inst,InstKind::StoreLocalIndexed{base,len:field.limits.len(),index:offset,word:0,value:limit});
                        let next=registers.len();registers.push(Scalar::U32);
                        emit(&mut out,&inst,InstKind::Binary{dst:next,op:ir::BinaryOp::IAdd,a:count,b:increment});count=next;
                    }
                    let field_offset=constant(registers,&mut out,&inst,Scalar::U32,field.offset as u64);
                    let offset=registers.len();registers.push(Scalar::U32);
                    emit(&mut out,&inst,InstKind::Binary{dst:offset,op:ir::BinaryOp::IAdd,a:inputs[3],b:field_offset});
                    let mask=constant(registers,&mut out,&inst,Scalar::U64,field.known_bits.0);
                    let bits=constant(registers,&mut out,&inst,Scalar::U64,field.known_bits.1);
                    let range_count=constant(registers,&mut out,&inst,Scalar::U64,values.len()as u64/2);
                    let limit_count=registers.len();registers.push(Scalar::U64);
                    emit(&mut out,&inst,InstKind::Cast{dst:limit_count,src:count});
                    if let InstKind::Call(mut call)=inst.kind.clone(){
                        call.arguments=vec![call.arguments[0].clone(),call.arguments[1].clone(),value(offset,Scalar::U32),ranges,value(range_count,Scalar::U64),bounds,value(limit_count,Scalar::U64),value(mask,Scalar::U64),value(bits,Scalar::U64)];
                        call.target=CallTarget::External(ExternalName{library:crate::resource::RUNTIME_LIBRARY.into(),symbol:"blink_complete_field".into()});
                        emit(&mut out,&inst,InstKind::Call(call));
                    }
                }
                continue;
            }
            out.push(inst);
        }
        *code=out;
    }
    for function in functions {
        let body = &mut function.body;
        block(&mut body.registers, &mut body.locals, &mut body.code, resources);
    }
    if program.native_frame.is_some() {
        visit_stage_bodies(&mut program.stages, |registers, locals, code| {
            block(registers, locals, code, resources);
        });
    }
    for index in 0..resources.programs.len() {
        let nested = &mut resources.programs[index].1;
        if nested.native_frame.is_some() {
            // A nested dispatch reads the original program's immutable argument layout.
            let mut stages = std::mem::take(&mut nested.stages);
            visit_stage_bodies(&mut stages, |registers, locals, code| {
                block(registers, locals, code, resources);
            });
            resources.programs[index].1.stages = stages;
        }
    }
}

impl Builder<'_>{
    pub(super) async fn element_model(&mut self,ty:&Ty,facts:&ElementFacts,location:(&residency::Residency,&[usize]),span:Span)->Result<Box<returns::Returned>> {
        Ok(match facts{
            ElementFacts::Unknown=>unsafe { self.world.call(self.unknown_element_model(ty,location)).await }?,
            ElementFacts::Input(when)=>{
                let mut paths=BTreeSet::new();residency::inputs(&location.0.under(&self.path),&mut paths);
                let mut result:Option<Box<returns::Returned>>=None;
                for input in paths{
                    let mut input=input.family();input.fields.push(residency::Part::Elements);
                    let Some((resource,_))=self.view_model.element_inputs.iter()
                        .filter(|(parent,_)|parent.parameter==input.parameter&&input.fields.starts_with(&parent.fields))
                        .max_by_key(|(parent,_)|parent.fields.len()).map(|(_,id)|*id)else{continue;};
                    let parameter=input.parameter;
                    let mut next=self.view_model.parameters.keys().filter_map(|id|match id{ValueIdentity::DatasetContent(p,r,component)if (*p,*r)==(parameter,resource)=>Some(component+1),_=>None}).max().unwrap_or(0);
                    let mut model=unsafe { self.world.call(self.unknown_element_model(ty,location)).await }?;
                    bind_element_input(self,&mut model,resource,&mut next,&mut ElementBindings::new(),&input);
                    residency::visit(&mut model,&mut input,&mut|data,input|data.residency=residency::Residency::input(input.model()));
                    if let Some(previous)=&mut result{previous.join(*model,true,None);}else{result=Some(model);}
                }
                let mut model=match result{Some(model)=>model,None=>unsafe { self.world.call(self.unknown_element_model(ty,location)).await }?};
                unsafe { self.world.call(contracts::mark_cleanup_model(self,&mut model,when.scope_element_indices(1),span)).await }?;model
            },
            ElementFacts::Known(model)=>model.clone(),
            ElementFacts::Cleanup(when)=>{
                let mut model=unsafe { self.world.call(self.unknown_element_model(ty,location)).await }?;
                unsafe { self.world.call(contracts::mark_cleanup_model(self,&mut model,when.scope_element_indices(1),span)).await }?;model
            },
            ElementFacts::Common(a,b)=>{let mut model=unsafe { self.world.call(self.element_model(ty,a,location,span)).await }?;model.join(*unsafe { self.world.call(self.element_model(ty,b,location,span)).await }?,true,None);model},
            ElementFacts::Selected(when,a,b)=>{let mut model=unsafe { self.world.call(self.element_model(ty,a,location,span)).await }?;model.join(*unsafe { self.world.call(self.element_model(ty,b,location,span)).await }?,true,Some(when));model},
        })
    }
    pub(super) async fn normalize_element_facts(&mut self,value:&mut returns::Returned,span:Span)->Result<()> {
        match value{
            returns::Returned::Dataset(data)=>{
                if matches!(data.elements,ElementFacts::Common(..)|ElementFacts::Selected(..)){
                    data.elements=ElementFacts::Known(unsafe { self.world.call(self.element_model(&data.ty,&data.elements,(&data.residency,&[]),span)).await }?);
                }
                if let Some(model)=data.elements.known_mut(){unsafe { self.world.call(self.normalize_element_facts(model,span)).await }?;}
            },
            returns::Returned::OwnedValue(owner)=>for child in owner.payloads.iter_mut().flatten(){unsafe { self.world.call(self.normalize_element_facts(child,span)).await }?;},
            returns::Returned::Value(_)|returns::Returned::Unit=>{},
        }
        Ok(())
    }
}

impl Builder<'_>{
    pub(super) async fn expand_current_element_facts(&mut self,data:&mut Data)->Result<()> {
        match data.elements{
            ElementFacts::Input(_)|ElementFacts::Cleanup(_)|ElementFacts::Common(..)|ElementFacts::Selected(..)=>{
                data.elements=ElementFacts::Known(unsafe { self.world.call(self.element_model(&data.ty,&data.elements,(&data.residency,&[]),self.scope.span())).await }?);
            },
            ElementFacts::Unknown|ElementFacts::Known(_)=>{},
        }
        Ok(())
    }
}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(DataPayload);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        root: Slot,
        ty: Slot,
        length: Slot,
        ownership: Slot,
        access: Slot,
        count: Slot,
        residency: Slot,
        layout: Slot,
        elements: Slot,
        prefixes: Slot,
    }
    #[repr(C)]
    struct Writer {
        root: WriteCallback,
        ty: WriteCallback,
        length: WriteCallback,
        ownership: WriteCallback,
        access: WriteCallback,
        count: WriteCallback,
        residency: WriteCallback,
        layout: WriteCallback,
        elements: WriteCallback,
        prefixes: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        root: ReadCallback,
        ty: ReadCallback,
        length: ReadCallback,
        ownership: ReadCallback,
        access: ReadCallback,
        count: ReadCallback,
        residency: ReadCallback,
        layout: ReadCallback,
        elements: ReadCallback,
        prefixes: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_data_payload"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_data_payload"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for DataPayload {
        fn empty() -> Self {
            Self {
                root: <Value as Native>::empty(),
                ty: <Ty as Native>::empty(),
                length: <Value as Native>::empty(),
                ownership: <Ownership as Native>::empty(),
                access: <Access as Native>::empty(),
                count: <Option<u32> as Native>::empty(),
                residency: <residency::Residency as Native>::empty(),
                layout: <DataLayout as Native>::empty(),
                elements: <ElementFacts as Native>::empty(),
                prefixes: <prefix::Fields as Native>::empty(),
                place: None,
                initialization: None,
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        root: contracts::address(&self.root).cast_mut(),
                        ty: contracts::address(&self.ty).cast_mut(),
                        length: contracts::address(&self.length).cast_mut(),
                        ownership: contracts::address(&self.ownership).cast_mut(),
                        access: contracts::address(&self.access).cast_mut(),
                        count: contracts::address(&self.count).cast_mut(),
                        residency: contracts::address(&self.residency).cast_mut(),
                        layout: contracts::address(&self.layout).cast_mut(),
                        elements: contracts::address(&self.elements).cast_mut(),
                        prefixes: contracts::address(&self.prefixes).cast_mut(),
                    },
                    Writer {
                        root: contracts::write_value::<Value>,
                        ty: contracts::write_value::<Ty>,
                        length: contracts::write_value::<Value>,
                        ownership: contracts::write_value::<Ownership>,
                        access: contracts::write_value::<Access>,
                        count: contracts::write_value::<Option<u32>>,
                        residency: contracts::write_value::<residency::Residency>,
                        layout: contracts::write_value::<DataLayout>,
                        elements: contracts::write_value::<ElementFacts>,
                        prefixes: contracts::write_value::<prefix::Fields>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        root: contracts::slot(&mut output.root),
                        ty: contracts::slot(&mut output.ty),
                        length: contracts::slot(&mut output.length),
                        ownership: contracts::slot(&mut output.ownership),
                        access: contracts::slot(&mut output.access),
                        count: contracts::slot(&mut output.count),
                        residency: contracts::slot(&mut output.residency),
                        layout: contracts::slot(&mut output.layout),
                        elements: contracts::slot(&mut output.elements),
                        prefixes: contracts::slot(&mut output.prefixes),
                    },
                    Reader {
                        root: contracts::read_value::<Value>,
                        ty: contracts::read_value::<Ty>,
                        length: contracts::read_value::<Value>,
                        ownership: contracts::read_value::<Ownership>,
                        access: contracts::read_value::<Access>,
                        count: contracts::read_value::<Option<u32>>,
                        residency: contracts::read_value::<residency::Residency>,
                        layout: contracts::read_value::<DataLayout>,
                        elements: contracts::read_value::<ElementFacts>,
                        prefixes: contracts::read_value::<prefix::Fields>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(ElementFacts);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        unknown: ReadCallback,
        known: ReadCallback,
        cleanup: ReadCallback,
        common: ReadCallback,
        selected: ReadCallback,
        input: ReadCallback,
    }
    unsafe extern "C" fn build_unknown(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<ElementFacts>() = ElementFacts::Unknown;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_known(context: Slot, destination: Slot) -> ReadStatus {
        let mut value = <Box<returns::Returned> as Native>::empty();
        #[repr(C)]
        struct Fields {
            value: Slot,
        }
        #[repr(C)]
        struct Host {
            value: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_element_facts_known"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    value: contracts::slot(&mut value),
                },
                Host {
                    value: contracts::read_value::<Box<returns::Returned>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ElementFacts>() = ElementFacts::Known(value);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_cleanup(context: Slot, destination: Slot) -> ReadStatus {
        let mut when = <bounds::Condition as Native>::empty();
        #[repr(C)]
        struct Fields {
            when: Slot,
        }
        #[repr(C)]
        struct Host {
            when: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_element_facts_cleanup"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    when: contracts::slot(&mut when),
                },
                Host {
                    when: contracts::read_value::<bounds::Condition>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ElementFacts>() = ElementFacts::Cleanup(when);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_common(context: Slot, destination: Slot) -> ReadStatus {
        let mut a = <Box<ElementFacts> as Native>::empty();
        let mut b = <Box<ElementFacts> as Native>::empty();
        #[repr(C)]
        struct Fields {
            a: Slot,
            b: Slot,
        }
        #[repr(C)]
        struct Host {
            a: ReadCallback,
            b: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_element_facts_common"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    a: contracts::slot(&mut a),
                    b: contracts::slot(&mut b),
                },
                Host {
                    a: contracts::read_value::<Box<ElementFacts>>,
                    b: contracts::read_value::<Box<ElementFacts>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ElementFacts>() = ElementFacts::Common(a, b);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_selected(context: Slot, destination: Slot) -> ReadStatus {
        let mut when = <Box<bounds::Condition> as Native>::empty();
        let mut a = <Box<ElementFacts> as Native>::empty();
        let mut b = <Box<ElementFacts> as Native>::empty();
        #[repr(C)]
        struct Fields {
            when: Slot,
            a: Slot,
            b: Slot,
        }
        #[repr(C)]
        struct Host {
            when: ReadCallback,
            a: ReadCallback,
            b: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_element_facts_selected"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    when: contracts::slot(&mut when),
                    a: contracts::slot(&mut a),
                    b: contracts::slot(&mut b),
                },
                Host {
                    when: contracts::read_value::<Box<bounds::Condition>>,
                    a: contracts::read_value::<Box<ElementFacts>>,
                    b: contracts::read_value::<Box<ElementFacts>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ElementFacts>() = ElementFacts::Selected(when, a, b);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_input(context: Slot, destination: Slot) -> ReadStatus {
        let mut when = <bounds::Condition as Native>::empty();
        #[repr(C)]
        struct Fields {
            when: Slot,
        }
        #[repr(C)]
        struct Host {
            when: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_element_facts_input"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    when: contracts::slot(&mut when),
                },
                Host {
                    when: contracts::read_value::<bounds::Condition>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<ElementFacts>() = ElementFacts::Input(when);
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_element_facts"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for ElementFacts {
        fn empty() -> Self {
            Self::Unknown
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Unknown => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_element_facts_unknown"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Known(value) => {
                    #[repr(C)]
                    struct Fields {
                        value: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        value: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_element_facts_known"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                value: contracts::address(value).cast_mut(),
                            },
                            Host {
                                value: contracts::write_value::<Box<returns::Returned>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Cleanup(when) => {
                    #[repr(C)]
                    struct Fields {
                        when: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        when: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_element_facts_cleanup"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                when: contracts::address(when).cast_mut(),
                            },
                            Host {
                                when: contracts::write_value::<bounds::Condition>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Common(a, b) => {
                    #[repr(C)]
                    struct Fields {
                        a: Slot,
                        b: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        a: WriteCallback,
                        b: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_element_facts_common"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                a: contracts::address(a).cast_mut(),
                                b: contracts::address(b).cast_mut(),
                            },
                            Host {
                                a: contracts::write_value::<Box<ElementFacts>>,
                                b: contracts::write_value::<Box<ElementFacts>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Selected(when, a, b) => {
                    #[repr(C)]
                    struct Fields {
                        when: Slot,
                        a: Slot,
                        b: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        when: WriteCallback,
                        a: WriteCallback,
                        b: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_element_facts_selected"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                when: contracts::address(when).cast_mut(),
                                a: contracts::address(a).cast_mut(),
                                b: contracts::address(b).cast_mut(),
                            },
                            Host {
                                when: contracts::write_value::<Box<bounds::Condition>>,
                                a: contracts::write_value::<Box<ElementFacts>>,
                                b: contracts::write_value::<Box<ElementFacts>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Input(when) => {
                    #[repr(C)]
                    struct Fields {
                        when: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        when: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_element_facts_input"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                when: contracts::address(when).cast_mut(),
                            },
                            Host {
                                when: contracts::write_value::<bounds::Condition>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        unknown: build_unknown,
                        known: build_known,
                        cleanup: build_cleanup,
                        common: build_common,
                        selected: build_selected,
                        input: build_input,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(DataLayout);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        declared: ReadCallback,
        parameter: ReadCallback,
    }
    unsafe extern "C" fn build_declared(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<DataLayout>() = DataLayout::Declared;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_parameter(context: Slot, destination: Slot) -> ReadStatus {
        let mut parameter = <usize as Native>::empty();
        let mut stride = <Value as Native>::empty();
        let mut offset = <Value as Native>::empty();
        #[repr(C)]
        struct Fields {
            parameter: Slot,
            stride: Slot,
            offset: Slot,
        }
        #[repr(C)]
        struct Host {
            parameter: ReadCallback,
            stride: ReadCallback,
            offset: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_data_layout_parameter"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    parameter: contracts::slot(&mut parameter),
                    stride: contracts::slot(&mut stride),
                    offset: contracts::slot(&mut offset),
                },
                Host {
                    parameter: contracts::read_value::<usize>,
                    stride: contracts::read_value::<Value>,
                    offset: contracts::read_value::<Value>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<DataLayout>() = DataLayout::Parameter {
                parameter,
                stride,
                offset,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_data_layout"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for DataLayout {
        fn empty() -> Self {
            Self::Declared
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Declared => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_data_layout_declared"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Parameter {
                    parameter,
                    stride,
                    offset,
                } => {
                    #[repr(C)]
                    struct Fields {
                        parameter: Slot,
                        stride: Slot,
                        offset: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        parameter: WriteCallback,
                        stride: WriteCallback,
                        offset: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_data_layout_parameter"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                parameter: contracts::address(parameter).cast_mut(),
                                stride: contracts::address(stride).cast_mut(),
                                offset: contracts::address(offset).cast_mut(),
                            },
                            Host {
                                parameter: contracts::write_value::<usize>,
                                stride: contracts::write_value::<Value>,
                                offset: contracts::write_value::<Value>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        declared: build_declared,
                        parameter: build_parameter,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Ownership);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{self, Native, Read as ReadCallback, Slot};
    #[repr(C)]
    struct Reader {
        owned: ReadCallback,
        borrowed: ReadCallback,
        moved: ReadCallback,
    }
    unsafe extern "C" fn build_owned(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Ownership>() = Ownership::Owned;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_borrowed(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Ownership>() = Ownership::Borrowed;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_moved(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Ownership>() = Ownership::Moved;
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_ownership"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Ownership {
        fn empty() -> Self {
            Self::Owned
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Owned => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_ownership_owned"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Borrowed => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_ownership_borrowed"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Moved => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_ownership_moved"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Conditional(_) => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_ownership_owned"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        owned: build_owned,
                        borrowed: build_borrowed,
                        moved: build_moved,
                    },
                )
            }
        }
    }
};
