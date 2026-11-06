//! 应用入口是一次普通 CPU 值函数调用，不伪造 dataset 或 parallel 域。
use super::cooperative::Argument;
use super::*;
#[derive(Default)]
pub(super) struct Context {
    pub frame: Option<Vec<Reg>>,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Ownership {
    Owned,
    Borrowed,
    Moved,
    Conditional(Box<owned_presence::Presence>),
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
    pub elements: Box<returns::Returned>,
}
impl DataPayload {
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
    Element { proof: &'a bounds::Proof, range: Option<(u32, u32)> },
    WholeFacts(&'a bounds::Proof),
}
// 动态数组借用携带实际可能的资源叶；这些来源只用于编译期别名和存活期。
#[derive(Clone)]
pub(super) struct DataReference {
    pub data: Data,
    pub sources: Vec<ValueIdentity>,
    pub projection: Vec<usize>,
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
crate::module::codec::variants!(DataLayout{0=>Declared,1=>Parameter{parameter,stride,offset}});
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
        root:address::Root::Resource(view::resource_borrow(data)),base:address::Layout::of(&data.ty),layout:address::Layout::of(ty),
        words:ty.components.iter().map(|(_,word)|field+word).collect(),positions:Arc::from([]),
        writable:data.access.writes(),initialized:vec![initialized;ty.components.len()].into(),
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
    fn loop_writes(&self,a:usize,b:usize,read:&Inst)->bool {
        let mut boundary=0;
        self.0.loop_contexts.iter().any(|context|{
            boundary+=context.outer.len();
            a.min(b)<boundary && a.max(b)>=boundary && match &read.kind {
                InstKind::LoadLocal(_,slot)=>context.writes.external||context.writes.locals.contains(slot),
                InstKind::ViewLoad{dst,address,index,stride,word}=>context.writes.unknown_storage||context.writes.storage.iter().any(|(written,range)|
                    match (self.frame_root(*address),range,self.constant(*index),self.stride(*stride)) {
                        (Some(root),_,_,_)if self.disjoint_roots(root,*written)=>false,
                        (Some(root),Some(range),Some(index),Some(stride))if root==*written=>{
                            let start=u128::from(index)*u128::from(stride)+u128::from(*word);
                            Self::byte_overlap(&(start..start+u128::from(self.0.registers[*dst].size())),range)
                        },
                        _=>true,
                    }),
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
    fn constant(&self,reg:Reg)->Option<Bits> {
        let(at,inst)=self.definition(reg)?;
        match &inst.kind {
            InstKind::Constant(_,_,bits)=>Some(*bits),
            InstKind::Cast{src,..}=>constant::cast(self.0.registers[*src],self.0.registers[reg],self.constant(*src)?),
            InstKind::LoadLocal(_,slot)if self.code().nth(at)?.results().any(|result|result==reg)=>{
                let(distance,write)=self.code().rev().skip(self.len()-at).enumerate().find(|(_,inst)|self.writes_local(inst,*slot))?;
                if self.loop_writes(at-distance-1,at,inst){return None;}
                match &write.kind {
                    InstKind::StoreLocal(_,value)=>self.constant(*value),
                    _=>None,
                }
            },
            _=>None,
        }
    }
    fn same(&self,a:Reg,b:Reg)->bool {
        if a==b{return true;}
        if self.0.registers[a]!=self.0.registers[b]{return false;}
        let(Some((ai,a_inst)),Some((bi,b_inst)))=(self.definition(a),self.definition(b))else{return false;};
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
            (InstKind::ViewLoad{..},InstKind::ViewLoad{..})=>{
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
    fn byte_overlap(a:&std::ops::Range<u128>,b:&std::ops::Range<u128>)->bool {a.start<b.end&&b.start<a.end}
    fn store_overlaps(&self,read:&IndexedSource,read_word:u32,read_scalar:Scalar,source:&IndexedSource,offset:Option<Reg>,word:u32,scalar:Scalar)->bool {
        let IndexedSource::View{address,index,stride}=read else{return true;};
        let position = |parts:[Reg;2]| -> Option<&ir::CallReference> {
            let InstKind::Cast{src:low,..}=&self.definition(parts[0])?.1.kind else{return None;};
            let InstKind::Cast{src:high,..}=&self.definition(parts[1])?.1.kind else{return None;};
            let InstKind::Binary{op:ir::BinaryOp::ShiftRightLogical,a,b,..}=&self.definition(*high)?.1.kind else{return None;};
            if self.constant(*b)!=Some(32)||!self.same(*low,*a){return None;}
            let InstKind::PositionAddress{address,..}=&self.definition(*low)?.1.kind else{return None;};
            Some(address)
        };
        let mut selected=source;let mut written_word=u128::from(word);
        while let IndexedSource::Pointer{address,index,stride,..}=selected {
            let Some(reference)=position(*address)else{break;};
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
            if let(Some(a),Some(b))=(self.frame_root(*address),self.frame_root(*other))
                &&self.disjoint_roots(a,b){return false;}
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
        // Private completion updates Frame metadata; it does not write payload bytes.
        if Self::pointer_call(call).is_some()||Self::allocation(call)||matches!(&call.target,CallTarget::External(name)if name.library==crate::application::RUNTIME_LIBRARY&&name.symbol.starts_with("blink_complete_")){return false;}
        match &call.target {
            CallTarget::Defined(name)=>!self.0.functions.writes_only_references(name),
            _=>true,
        }
    }
    fn writes_local(&self,inst:&Inst,slot:usize)->bool {
        if inst.local_writes().contains(&slot){return true;}
        if let InstKind::Call(call)=&inst.kind {
            return self.call_memory_writes(call,|borrow|borrow.pending()||address::MemoryEffect::of(borrow,Access::InOut).writes_position(&Position::Local(slot)));
        }
        match &inst.kind {
            InstKind::If{yes,no,..}=>yes.iter().chain(no).any(|inst|self.writes_local(inst,slot)),
            InstKind::Loop{condition,body,..}=>condition.iter().chain(body).any(|inst|self.writes_local(inst,slot)),
            InstKind::Scope(body)=>body.iter().any(|inst|self.writes_local(inst,slot)),
            _=>false,
        }
    }
    fn writes(&self,inst:&Inst,load:&Inst)->bool {
        let InstKind::ViewLoad{dst,address,index,stride,word}=&load.kind else{return false;};
        self.writes_indexed(inst,&IndexedSource::View{address:*address,index:*index,stride:*stride},*word,self.0.registers[*dst])
    }
    fn writes_indexed(&self,inst:&Inst,read:&IndexedSource,word:u32,scalar:Scalar)->bool {
        match &inst.kind {
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
                    if borrow.pending()||address::external_memory(borrow){return true;}
                    if let CallTarget::Defined(name)=&call.target
                        && self.0.functions.writes_only_references(name) {
                        return call.memory.iter().any(|effect|effect.access.writes());
                    }
                    borrow.sources.entries().any(|(source,when)|when.possible()&&match source{
                        view::Source::Position(origin)if allocated&&matches!(origin.root,address::Root::Local{..})=>false,
                        _=>true,
                    })
                })
            },
            InstKind::If{yes,no,..}=>yes.iter().chain(no).any(|inst|self.writes_indexed(inst,read,word,scalar)),
            InstKind::Loop{condition,body,..}=>condition.iter().chain(body).any(|inst|self.writes_indexed(inst,read,word,scalar)),
            InstKind::Scope(body)=>body.iter().any(|inst|self.writes_indexed(inst,read,word,scalar)),
            InstKind::Store{..}|InstKind::Atomic(_)|InstKind::Append(_)=>true,
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
    fn stored_paths(&self, code: &[Inst], mut next: Option<bounds::Proof>, source: &IndexedSource, word: u32, scalar: Scalar, path: &bounds::Clause, state: &flow::LoopState, controls: &BTreeMap<Reg, Value>) -> (Option<bounds::Proof>, Option<bounds::Proof>, Option<bounds::Proof>) {
        fn join(a: Option<bounds::Proof>, b: Option<bounds::Proof>) -> Option<bounds::Proof> {
            match (a, b) { (Some(mut a), Some(b)) => { a.join_condition(&b, 1, None); Some(a) }, (a, None) | (None, a) => a }
        }
        let mut continues = None;
        let mut exits = None;
        for inst in code {
            let Some(current) = next.take() else { break; };
            match &inst.kind {
                InstKind::Continue => continues = join(continues, Some(current)),
                InstKind::Break => {},
                InstKind::Exit => exits = join(exits, Some(current)),
                InstKind::If { condition, yes, no } => {
                    let raw = Value { ty: Ty::scalar(Scalar::Bool), regs: vec![*condition], proof: bounds::Proof::default(), literal: false, uniform: false, constant: None };
                    let condition = bounds::Condition::of(controls.get(condition).unwrap_or(&raw));
                    let truth = condition.truth_in(path);
                    let a = if truth == Some(false) { (None, None, None) } else { self.stored_paths(yes, Some(current.clone()), source, word, scalar, &condition.assume(path, true), state, controls) };
                    let b = if truth == Some(true) { (None, None, None) } else { self.stored_paths(no, Some(current), source, word, scalar, &condition.assume(path, false), state, controls) };
                    next = join(a.0, b.0); continues = join(continues, join(a.1, b.1)); exits = join(exits, join(a.2, b.2));
                },
                InstKind::Scope(body) => {
                    let (normal, continued, exited) = self.stored_paths(body, Some(current), source, word, scalar, path, state, controls);
                    next = join(normal, exited); continues = join(continues, continued);
                },
                _ => next = Some(if let Some(value) = self.field_store(source, word, scalar, inst, true) {
                    bounds::scoped_proof(&Ty::scalar(scalar), bounds::Proof::default().with_identity(ValueIdentity::Register(value)), path, &state.env)
                } else if self.writes_indexed(inst, source, word, scalar) { bounds::Proof::default() } else { current }),
            }
        }
        (next, continues, exits)
    }

    fn prior_snapshot<'a>(&'a self,code:impl Iterator<Item=&'a Inst>,mut load:&'a Inst)->Option<Reg> {
        let mut result=None;
        for inst in code {
            let nested=match &inst.kind {
                InstKind::If{condition,yes,no}=>{
                    let mut source=*condition;
                    while let Some((at,Inst{kind:InstKind::LoadLocal(_,slot),..}))=self.definition(source) {
                        let Some(Inst{kind:InstKind::StoreLocal(_,value),..})=self.code().rev().skip(self.len()-at).find(|inst|self.writes_local(inst,*slot))else{break;};
                        source=*value;
                    }
                    let value=Value{ty:Ty::scalar(Scalar::Bool),regs:vec![source],proof:bounds::Proof::default(),literal:false,uniform:false,constant:None};
                    self.0.condition_truth(&bounds::Condition::of(&value),None).map(|truth|if truth{yes.as_slice()}else{no.as_slice()})
                },
                InstKind::Scope(body)=>Some(body.as_slice()),
                _=>None,
            };
            if let Some(reg)=nested.and_then(|code|self.prior_snapshot(code.iter().rev(),load)) {
                result=Some(reg);load=self.definition(reg).unwrap().1;
            }
            if self.writes(inst,load){break;}
            if self.same_load(inst,load) {
                if let InstKind::ViewLoad{dst,..}=inst.kind{result=Some(dst);load=inst;}
            }
        }
        result
    }
    fn snapshot(&self,reg:Reg)->Reg {
        let Some((at,load))=self.definition(reg)else{return reg;};
        if !matches!(load.kind,InstKind::ViewLoad{..}){return reg;}
        self.prior_snapshot(self.code().rev().skip(self.len()-at),load).unwrap_or(reg)
    }
}
impl Builder<'_> {
    pub(super) fn application_condition_writes(&self,code:&[Inst],place:&Place,component:usize)->bool {
        let mut positions=place.data_positions();if positions.is_empty(){positions=self.positions(place);}
        let position=positions.get(component);
        fn writes(loads:&ApplicationLoads<'_, '_>,code:&[Inst],position:Option<&Position>)->bool {
            code.iter().any(|inst|match &inst.kind {
                InstKind::If{yes,no,..}=>writes(loads,yes,position)||writes(loads,no,position),
                InstKind::Loop{condition,body,..}=>writes(loads,condition,position)||writes(loads,body,position),
                InstKind::Scope(body)|InstKind::Cleanup{body,..}=>writes(loads,body,position),
                InstKind::Call(call)=>{
                    let affected=|borrow:&view::Borrow|borrow.pending()||position.is_none_or(|position|address::MemoryEffect::of(borrow,Access::InOut).writes_position(position));
                    position.is_some_and(|p|matches!(p,Position::Local(slot)if inst.local_writes().contains(slot)))
                        ||loads.call_memory_writes(call,affected)
                        ||call.memory_use.is_some_and(|first|loads.0.view_model.uses.iter().skip(first+1)
                            .take_while(|usage|matches!(&usage.requirement,view::Requirement::Memory{target:address::MemoryTarget::Call(at),..}if *at==first))
                            .any(|usage|matches!(&usage.requirement,view::Requirement::Memory{borrow,access:Some(access),..}if access.writes()&&affected(&loads.0.current_address_borrow(borrow)))))
                },
                InstKind::Store{..}|InstKind::StoreIndexed{..}|InstKind::Atomic(_)|InstKind::Append(_)=>true,
                _=>position.is_some_and(|p|matches!(p,Position::Local(slot)if inst.local_writes().contains(slot))),
            })
        }
        writes(&ApplicationLoads(self),code,position)
    }
    pub(super) fn application_iteration_interference(&self, data: &DataReference, index: &Value, proof: &mut bounds::Proof, states: &[flow::LoopState], controls: &BTreeMap<Reg, Value>, writes: &effects::LoopWrites) {
        let loads = ApplicationLoads(self);
        let (offset, ty) = data.element_layout();
        for (component, &(scalar, word)) in ty.components.iter().enumerate() {
            let mut current = proof.project(&[component], ty.components.len());
            for state in states {
                if let Some(source) = loads.stored_position(&self.code, &data.data, index, offset + word, scalar, state, writes) {
                    if let Some(other) = loads.iteration_interference(&self.code, source, offset + word, scalar, &state.path, state, controls) { current.join_condition(&other, 1, None); }
                } else { current = bounds::Proof::default(); }
            }
            proof.assign(&[component], ty.components.len(), current);
        }
    }
    pub(super) fn application_indexed_proof(&self, data: &Data, index: &Value, states: &[flow::LoopState], controls: &BTreeMap<Reg, Value>, writes: &effects::LoopWrites) -> bounds::Proof {
        let loads = ApplicationLoads(self);
        let mut output = bounds::Proof::default();
        for (component, &(scalar, word)) in data.ty.components.iter().enumerate() {
            let mut common: Option<bounds::Proof> = None;
            for state in states {
                let mut proof = bounds::Proof::default();
                if let Some(source) = loads.stored_position(&self.code, data, index, word, scalar, state, writes) {
                    let (normal, continued, _) = loads.stored_paths(&self.code, Some(bounds::Proof::default()), source, word, scalar, &state.path, state, controls);
                    proof = match (normal, continued) { (Some(mut a), Some(b)) => { a.join_condition(&b, 1, None); a }, (Some(a), None) | (None, Some(a)) => a, _ => bounds::Proof::default() };
                    if let Some(other) = loads.iteration_interference(&self.code, source, word, scalar, &state.path, state, controls) { proof.join_condition(&other, 1, None); }
                }
                if let Some(value) = &mut common { value.join_condition(&proof, 1, None); } else { common = Some(proof); }
            }
            if let Some(proof) = common { output.assign(&[component], data.ty.components.len(), proof); }
        }
        output
    }
    pub(super) fn application_read_snapshot(&self,reg:Reg)->Reg {ApplicationLoads(self).snapshot(reg)}
}

pub(super) fn pointer_type() -> Ty {
    let mut ty = Ty::vector(Scalar::U32, 2);
    ty.name = "__blink_frame".into();
    ty
}
fn argument(value: Value) -> CallArgument {
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
pub(crate) fn compile_application(
    world: &World,
    instance: &crate::generics::Instance,
) -> Result<crate::application::Application> {
    let d = &instance.definition;
    validate_signature(d)?;
    let result = if d.function.result.name == "void" || d.function.result.is_never() {
        None
    } else {
        Some(world.ty(
            &d.function.result,
            &d.source,
            d.function.span,
            &d.module,
            &d.imports,
        )?)
    };
    if d.function.kind != FunctionKind::Value
        || d.function.external.is_some()
        || d.function.target != Target::Cpu
        || !d.function.params.is_empty()
        || !result.is_some_and(|result| {
            [Ty::scalar(Scalar::U32), Ty::scalar(Scalar::I32)].contains(&result)
        })
    {
        return d.source.fail(
            d.function.span,
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
    let mut builder = Builder::new(world, d, &context, 0, Some(&instance.name))?;
    let entry_anchor = GenerationAnchor::Definition {
        origin: d.source_origin().map(Arc::new), instance: Arc::from(instance.name.as_str()),
    };
    builder.application = Some(Context::default());
    builder.in_function = true;
    let result = builder.native_function(instance.clone(), Vec::new(), d.function.span, Some(Arc::new(GeneratedOrigin {
        reason: GenerationReason::ApplicationEntryCall, trigger: Some(entry_anchor.clone()),
    })), None)?;
    let result = builder.ordinary_return(result, d.function.span)?.unwrap();
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
    let finalized = native_function::finish(world, builder.functions, &mut body, &builder.view_model)?;
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
            InstKind::Scope(body) => runtime_calls(body, visitor),
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
fn declarations(
    codes: &mut [&mut Vec<Inst>],
    resources: crate::application::Resources,
) -> crate::application::Resources {
    let mut names = BTreeSet::new();
    for code in codes.iter_mut() {
        runtime_calls(code, &mut |call| {
            names.insert(call.symbol.clone());
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
                renames.insert(name, format!("blink_{prefix}_{}", live.len()));
                live.push(item);
            }
        }
        live
    }
    let mut renames = BTreeMap::new();
    let types = retain(resources.types, "allocate", &names, &mut renames);
    let programs = retain(resources.programs, "run", &names, &mut renames);
    let completions = retain(resources.completions, "complete", &names, &mut renames);
    for code in codes.iter_mut() {
        runtime_calls(code, &mut |call| {
            if let Some(name) = renames.get(&call.symbol) {
                call.symbol = name.clone();
            }
        });
    }
    crate::application::Resources { types, programs, completions }
}
pub(super) fn module_resources(
    functions: &mut [FunctionBody],
    resources: crate::application::Resources,
) -> Option<crate::application::Resources> {
    let mut used = false;
    for function in &mut *functions {
        runtime_calls(&mut function.body.code, &mut |_| used = true);
    }
    if !used {
        return None;
    }
    let mut codes = functions
        .iter_mut()
        .map(|function| &mut function.body.code)
        .collect::<Vec<_>>();
    Some(declarations(&mut codes, resources))
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
    pub(super) fn application_parameter(
        &mut self,
        binding: usize,
        parameter: usize,
        access: Access,
    ) -> Result<Data> {
        if access == Access::Value {
            self.application_frame(self.definition.function.span)?;
        }
        let scalar = if access == Access::Value { Scalar::U32 }
            else { Self::borrowed_dataset_parameter_type(access, self.definition.execution).components[0].0 };
        let reg = self.reg(scalar);
        let handle = self.application_root(reg);
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
        let elements=self.unknown_element_model(&element_ty);
        Ok(Box::new(DataPayload {
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
                            flow::initialized_application_word(
                                &self.initialized,
                                data.initialization.as_ref().unwrap(),
                                index,
                                *word,
                            )
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
            if scope.application_word(None,word).initialized_by(&facts)
                ||length.is_some_and(|length|scope.application_prefix_initialized(length,word,&facts)){return true;}
            match scope {
                Position::Conditional(when,a,b)=>complete(builder,&a,word,length,initialized,assuming.clone().and(when.as_ref().clone()))
                    &&complete(builder,&b,word,length,initialized,assuming.and(when.as_ref().clone().not())),
                Position::Both(a,b)=>complete(builder,&a,word,length,initialized,assuming.clone())
                    &&complete(builder,&b,word,length,initialized,assuming),
                Position::Indexed(choices,index)=>choices.iter().enumerate().all(|(element,choice)|complete(builder,choice,word,length,initialized,assuming.clone().and(owned_presence::Presence::Selected(index,element)))),
                _=>false,
            }
        }
        let Some(scope)=&data.initialization else{return true;};
        let length=(data.length.minimum()==data.length.maximum()).then(||data.length.minimum());
        complete(self,scope,word,length,initialized,owned_presence::Presence::All)
    }
    pub(super) fn require_application_initialized(&mut self, data: &Data, span: Span) -> Result<()> {
        if !self.application_initialized(data)
            && !self.defer_position_initialization(data.ty.components.iter().map(|(_,word)|data.initialization.as_ref().unwrap().application_word(None,*word)).collect(),"B142",span)? {
            return self.fail(
                span,
                "B142",
                "读取或 inout 借用了尚未完整初始化的 dataset；write 输出须先完整写入所需元素与字段",
            );
        }
        Ok(())
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

    pub(super) fn application_drop(&mut self,data:&Data,retained:&view::Borrow,span:Span)->Result<()> {
        self.application_movable(data,retained,span)?;
        let handle=if let Some(place)=&data.place{self.read_place(&place.clone().project(Ty::scalar(Scalar::U32),&[0]),span, None)?.regs[0]}else{data.root.regs[0]};
        if data.ty.owns_resources(){self.drop_application_elements(data,retained,span)?;}
        let origin=Arc::new(GeneratedOrigin{reason:GenerationReason::DatasetRelease,trigger:self.source_site.clone().map(GenerationAnchor::Syntax)});
        self.code.push(Inst::generated(self.application_release(handle).kind,origin));
        Ok(())
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
    pub(super) fn application_borrowed(&self,identity:ValueIdentity)->bool{self.application_view_borrowed(identity)||self.application_position_borrowed(identity)}
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
        self.check_view_requirement(view::Requirement::Unborrowed{resource:self.owned_resource_borrow(data),borrow:retained.clone(),ownership:true,span})?;
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
    pub(super) fn application_value(&mut self, name: &str, span: Span) -> Result<Option<Data>> {
        let Some(Binding::ApplicationData(mut data)) = self.env.get(name).cloned() else {
            return Ok(None);
        };
        data.ownership=self.resolve_ownership(&data.ownership,None);
        data.residency=self.resolve_owned_residency(&data.residency,None);
        self.application_available(&data, span)?;
        self.prepare_owned_read(&returns::Returned::Dataset(data.clone()));
        data = self.refresh_application_data(data,span)?;
        if let Some(Binding::ApplicationData(source)) = self.env.get_mut(name) {
            source.root.proof = data.root.proof.clone();
            source.length.proof = data.length.proof.clone();
        }
        Ok(Some(data))
    }
    pub(super) fn refresh_application_data(&mut self,mut data:Data,span:Span)->Result<Data> {
        if let Some(place)=&data.place {
            let value=self.read_storage_value(place,span, None)?;data.root.regs=vec![value.regs[0]];data.length.regs=vec![value.regs[1]];
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
            let ty=Ty::owned_dataset(data.ty.clone(),data.count);let(slots,storage)=self.local_slots(&ty);
            let origin=Arc::new(GeneratedOrigin{reason:GenerationReason::DatasetBindSlots,trigger:self.source_site.clone().map(GenerationAnchor::Syntax)});
            self.code.push(Inst::generated(InstKind::StoreLocal(slots[0],data.root.regs[0]),origin.clone()));
            self.code.push(Inst::generated(InstKind::StoreLocal(slots[1],data.length.regs[0]),origin));
            let place=Place::Local(ty,slots,storage);self.initialize(&place);data.place=Some(place);
        }
        data
    }
    pub(super) fn release_application_data(&mut self,keep:Option<&BTreeSet<String>>,retained:view::Borrow)->Result<()> {
        self.cleanup_group(Vec::new(),|builder|{
        let mut names=builder.env.iter()
            .filter(|(name,_)|!keep.is_some_and(|names|names.contains(*name)))
            .filter_map(|(name,binding)|match binding {
                Binding::ApplicationData(data) if data.ownership.may_own()=>Some(name.clone()),
                Binding::OwnedValue(value) if value.ownership.may_own()&&(matches!(builder.pending_error,Some(PendingError::Exit(_)))||!matches!(value.place(),Some(Place::Reference{..})))=>Some(name.clone()),
                _=>None,
            }).collect::<Vec<_>>();
        names.sort_by_key(|name|{
            let temporary=name.strip_prefix("\0argument:").map(|index|index.parse::<usize>().unwrap());
            let declaration=match &builder.env[name]{Binding::OwnedValue(value)=>value.declaration,_=>0};
            (temporary.is_some(),temporary.unwrap_or(declaration))
        });
        let mut bindings=names.into_iter().map(|name|{
            let binding=builder.env[&name].clone();
            (name,binding)
        }).collect::<Vec<_>>();
        while let Some((name,binding))=bindings.pop(){
            let mut remaining=retained.clone();
            for(future,value)in &bindings{
                if future.starts_with('\0'){remaining.union(&user_owned::cleanup_borrow(value,&BTreeSet::new()));}
            }
            builder.drop_binding(binding,&remaining,builder.definition.function.span)?;
            if name.starts_with('\0'){builder.env.remove(&name);}else{
                match builder.env.get_mut(&name){
                    Some(Binding::OwnedValue(value))=>value.mark_moved(),
                    Some(Binding::ApplicationData(data))=>data.ownership=Ownership::Moved,
                    _=>{},
                }
            }
        }
        Ok(())
            })
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
    pub(super) fn application_declare(
        &mut self,
        annotation: &TypeName,
        name: &str,
        span: Span,
    ) -> Result<()> {
        self.application_frame(span)?;
        let definition = self.definition.clone();
        let ty = self.world.ty(
            annotation,
            &definition.source,
            span,
            &definition.module,
            &definition.imports,
        )?;
        let count = self.world.shape(
            &annotation.count,
            &definition.source,
            span,
            &definition.module,
            &definition.imports,
        )?;
        let slots = self.local_slots(&Ty::owned_dataset(ty.clone(), count)).0;
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
        let elements=self.unknown_element_model(&ty);
        self.env.insert(
            name.into(),
            Binding::ApplicationData(Box::new(DataPayload {
                elements,
                root: value(handle),
                length: value(length),
                place: Some(Place::Local(Ty::owned_dataset(ty.clone(),count),slots.to_vec(),slots[0]..slots[1]+1)),
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
    pub(super) fn application_assign(
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
        let Some(mut value) = self.application_resource(rhs, span)? else {
            return Ok(());
        };
        self.application_data_type(&mut value, &target.ty, target.count, span)?;
        self.view_uses.after(span);
        if matches!(&self.env[name], Binding::ApplicationData(data) if data.ownership == Ownership::Owned)
        {
            let old = self.application_value(name, span)?.unwrap();
            self.drop_returned(returns::Returned::Dataset(old),&view::Borrow::default(),span)?;
        }
        let place=target.place.unwrap();let packed=self.returned_storage(&returns::Returned::Dataset(value.clone()));
        let origin=Arc::new(GeneratedOrigin{reason:GenerationReason::DatasetAssignSlots,trigger:self.source_site.clone().map(GenerationAnchor::Syntax)});
        self.write_storage_value(&place,&packed,span,Some(origin))?;value.place=Some(place);
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
    pub(super) fn application_resource(&mut self, expr: &Expr, span: Span) -> Result<Option<Data>> {
        match self.evaluate_returned(expr, None)? {
            Some(returns::Returned::Dataset(data)) => Ok(Some(data)),
            None => Ok(None),
            _ => self.fail(span, "B224", "dataset 资源需要同类型拥有值，不能由普通值或聚合整体替代"),
        }
    }
    #[inline(never)]
    pub(super) fn move_application_dataset(&mut self, name: &str, span: Span) -> Result<Option<Data>> {
        self.application_frame(span)?;
        let Some(mut data) = self.application_value(name, span)? else {
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
    pub(super) fn resize_application_dataset(&mut self, args: &[Expr], span: Span) -> Result<Option<Data>> {
        if args.len() != 3 {
            return self.fail(span, "B266", "resize(data,length,fill) 消费拥有的 CPU dataset，以同型值初始化新增位置");
        }
        let Some(returned) = self.evaluate_returned(&args[0], None)? else { return Ok(None); };
        let returns::Returned::Dataset(data) = returned else {
            return self.fail(args[0].span, "B224", "resize 需要一份拥有的 dataset");
        };
        let data = self.refresh_application_data(data, span)?;
        if data.ownership != Ownership::Owned {
            return self.fail(args[0].span, "B227", "dataset 借用不拥有调整存储长度的权利");
        }
        self.require_application_initialized(&data, span)?;
        self.application_cpu(&data, span)?;
        let input=self.hold_binding(Binding::ApplicationData(data.clone()));
        let Some(length) = self.expr(&args[1])? else { return Ok(None); };
        self.require_same(&Ty::scalar(Scalar::U32), &length.ty, args[1].span)?;
        let Some(mut returned) = self.evaluate_returned(&args[2], Some(&data.ty))? else { return Ok(None); };
        self.check_returned_type(&mut returned, &data.ty, args[2].span)?;
        self.require_returned_initialized(&mut returned, args[2].span)?;
        let fill = self.returned_storage(&returned);
        if fill.ty.contains_views() {
            return self.fail(args[2].span, "B224", "resize 填充值不能复制 view 借用图");
        }
        let mut retained = fill.proof.typed_view_borrow(&fill.ty);
        for borrow in view_contracts::resource_borrows(&returned) { retained.union(&borrow); }
        self.view_uses.invocation(span);
        retained.union(&self.view_uses.all(self,&[view_liveness::Transfer::OwnedData(&data)]));
        self.application_movable(&data, &retained, span)?;
        if fill.ty.owns_resources() {
            let exact = data.length.minimum() == data.length.maximum()
                && length.minimum() == length.maximum()
                && data.length.maximum().checked_add(1) == Some(length.minimum());
            let sources = data.length.component_origins(0);
            let offset = length.proof.domains(0).into_iter().any(|domain|
                matches!(domain, IndexDomain::Offset(base, 1) if sources.contains(&base)));
            if !exact && !offset {
                return self.fail(args[1].span, "B224", "拥有填充值只能移动到已证明不回绕且恰好新增的一项，不能广播复制或截短拥有元素");
            }
        }
        let pending=match &returned {
            returns::Returned::OwnedValue(value)=>Some(self.hold_binding(Binding::OwnedValue(value.clone()))),
            returns::Returned::Dataset(data)=>Some(self.hold_binding(Binding::ApplicationData(data.clone()))),
            _=>None,
        };
        let incoming = self.capture_element_model(&data, &[], returned);
        let growth = bounds::Condition::comparison(Comparison::Greater, &length, &data.length);
        let grows = growth.truth_in(&self.path);
        let elements = if grows == Some(false) { data.elements } else if data.length.maximum() == 0 { incoming } else {
            let mut previous = data.elements;
            if let Some(value)=previous.view_value_mut(){
                let present=bounds::Condition::selected(ownership::ApplicationIndex::of(&data.length),0).negate();
                value.proof.under_path(&value.ty,&present.assume(&bounds::Clause::default(),true));
            }
            if grows == Some(true) {
                previous.join(*incoming,true);
            } else {
                let mut combined = previous.clone();
                combined.join(*incoming,true);
                previous.join_locations(*combined, &growth.negate(), true);
            }
            previous
        };
        let (slots, storage) = self.local_slots(&fill.ty);
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetResizeFill,
            trigger: self.source_at(args[2].span, SourceSyntax::Expression).map(GenerationAnchor::Syntax),
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
    fn resize_application_storage(&mut self,root:Value,ty:Ty,mut length:Value,elements:Box<returns::Returned>,fill:CallArgument,snapshots:&[&Value],span:Span)->Result<Data> {
        let frame = self.application_frame(span)?;
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
        // Frame 槽位不变；新 SSA 身份使旧长度／下标证明停留在旧存储版本。
        let handle = self.application_root(reg);
        length.literal = false;
        length.proof = bounds::Proof::capture(&length).with_domains(Domains::from([
            IndexDomain::ApplicationLength(handle.identity()),
        ]));
        let data = Box::new(DataPayload {
            elements,
            root: handle, place: None, initialization: None, ty, length,
            ownership: Ownership::Owned, access: Access::InOut, count: None, residency: residency::Residency::cpu(), layout:DataLayout::Declared,
        });
        self.complete_application_elements(&DataReference::direct(data.clone()), &data.elements, snapshots, span, Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetResizeComplete,
            trigger: self.source_at(span, SourceSyntax::Expression).map(GenerationAnchor::Syntax),
        }))?;
        Ok(data)
    }
    pub(super) fn take_last_application_dataset(&mut self,args:&[Expr],span:Span)->Result<Option<Data>> {
        let [source,output]=args else{return self.fail(span,"B266","take_last(data,output)消费非空CPU dataset并交出唯一尾值");};
        let Some(returns::Returned::Dataset(data))=self.evaluate_returned(source,None)? else {
            if !self.flow.continues(){return Ok(None);}
            return self.fail(source.span,"B224","take_last需要一份拥有的dataset");
        };
        let data=self.refresh_application_data(data,span)?;
        if data.ownership!=Ownership::Owned{return self.fail(source.span,"B227","dataset借用不能交出尾元素或缩短存储");}
        self.require_application_initialized(&data,span)?;self.application_cpu(&data,span)?;
        let one=self.constant(Scalar::U32,1);let last=self.binary("-",data.length.clone(),one,span,None)?;
        if !self.index_in(&last,IndexDomain::ApplicationData(data.root.identity()),data.length.minimum(),None) {
            return self.fail(source.span,"B137","take_last需要由原长度条件证明输入非空");
        }
        let held=self.hold_binding(Binding::ApplicationData(data.clone()));
        let destination=self.mutable_place(output,Access::Write)?;
        if !self.flow.continues(){return Ok(None);}
        let Some(destination)=destination else{return self.fail(output.span,"B227","take_last输出需要同元素类型的可写位置");};
        self.require_same(&data.ty,destination.ty(),output.span)?;
        let place=Self::application_element(&data,&last,(last.minimum(),last.maximum()));
        let packed=self.read_storage_value(&place,span,None)?;
        let mut value=if data.ty.owns_resources(){self.materialize_element(&DataReference::direct(data.clone()),packed,&place)}else{returns::Returned::value(packed)};
        self.require_returned_initialized(&mut value,span)?;
        self.env.remove(&held);
        self.view_uses.invocation(span);let retained=self.view_uses.all(self,&[]);
        self.application_movable(&data,&retained,span)?;
        if data.ty.owns_resources(){
            let old=if matches!(destination.storage(),Place::ApplicationElement{..}){self.application_owned_argument(&destination,span)?}else{self.owned_place_model(&destination)};
            self.view_uses.after(span);
            self.store_owned_returned(destination,old,value,span)?;
        }
        else {
            let packed=self.returned_storage(&value);self.write_storage_value(&destination,&packed,span,None)?;
            self.assign_owned_proof(&destination,bounds::Proof::capture(&packed),packed.uniform);self.initialize(&destination);
        }
        // A shrinking Frame never reads fill; the moved tail has no second owner there.
        let fill=argument(self.constant(Scalar::U64,0));
        self.resize_application_storage(data.root,data.ty,last,data.elements,fill,&[],span).map(Some)
    }
    pub(super) fn dataset_constructor_type(world:&World,definition:&Definition,explicit:&[GenericArgument],arguments:usize,span:Span)->Result<(Ty,Option<u32>)> {
        if !(1..=2).contains(&explicit.len())||!(1..=2).contains(&arguments) {
            return definition.source.fail(span,"B266","dataset<T>(length) 需要元素类型和一个 u32 长度");
        }
        let GenericArgument::Type(element) = &explicit[0] else {
            return definition.source.fail(span, "B266", "dataset<T> 需要元素类型");
        };
        if element.dataset {
            return definition.source.fail(span,"B232","资源包装不能直接嵌套；子资源通过记录字段表达");
        }
        if element.tile {
            return definition.source.fail(
                span,
                "B266",
                "dataset 的元素是普通值，不是另一份 dataset 或 tile 所有权",
            );
        }
        let ty = world.ty(element,&definition.source,span,&definition.module,&definition.imports)?;
        let count = if let Some(argument) = explicit.get(1) {
            let Some(shape) = argument.shape() else {
                return definition.source.fail(span, "B266", "dataset<T,N> 的 N 需要静态 u32 形状");
            };
            world.shape(
                &Some(shape),
                &definition.source,
                span,
                &definition.module,
                &definition.imports,
            )?
        } else {
            None
        };
        Ok((ty,count))
    }
    #[inline(never)]
    pub(super) fn create_application_dataset(&mut self, expr: &Expr, span: Span) -> Result<Option<Data>> {
        self.application_frame(span)?;
        let ExprKind::Specialize(constructor, explicit, args) = &expr.kind else {
            return self.fail(
                span,
                "B266",
                "动态 dataset 使用 dataset<T>(length) 创建独占存储",
            );
        };
        if constructor != "dataset" {
            return self.fail(
                span,
                "B266",
                "dataset<T>(length) 需要元素类型和一个 u32 长度",
            );
        }
        let definition=self.definition.clone();
        let(ty,count)=Self::dataset_constructor_type(self.world,&definition,explicit,args.len(),span)?;
        let Some(mut length) = self.expr(&args[0])? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &length.ty, args[0].span)?;
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
                    trigger: self.source_at(expr.span, SourceSyntax::Expression).map(GenerationAnchor::Syntax),
                })),
            )?
            .unwrap();
        let handle = self.application_root(handle.regs[0]);
        length.proof = bounds::Proof::capture(&length).with_domains(Domains::from([
            IndexDomain::ApplicationLength(handle.identity()),
        ]));
        let elements=self.unknown_element_model(&ty);
        let mut data=Box::new(DataPayload {
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
            data.elements=self.initialize_application_elements(&data,factory,span)?;
            data.initialization=None;
        }
        Ok(Some(data))
    }
    // Generate actual element storage with the original resource owner.
    fn initialize_application_elements(&mut self,data:&Data,factory:&Expr,span:Span)->Result<Box<returns::Returned>> {
        let (name,explicit)=match &factory.kind {
            ExprKind::Name(name)=>(name.as_str(),None),
            ExprKind::FunctionRef(name,args)=>(name.as_str(),Some(args.as_slice())),
            _=>return self.fail(factory.span,"B224","dataset生成函数需要明确静态函数引用"),
        };
        let key=self.world.resolve(name,&self.definition.module,&self.definition.imports,true)
            .ok_or_else(||Diagnostic{source:self.definition.source.clone(),span:factory.span,code:"B158",message:format!("未知生成函数 {name}")})?;
        let trigger=self.source_at(factory.span,SourceSyntax::Expression);
        let origin=|reason|Arc::new(GeneratedOrigin{reason,trigger:trigger.clone().map(GenerationAnchor::Syntax)});
        let cursor=origin(GenerationReason::DatasetInitializeCursor);
        let scalar=Ty::scalar(Scalar::U32);let zero=self.constant_with_origin(Scalar::U32,0,Some(cursor.clone()));let index_reg=self.reg(Scalar::U32);
        let mut proof=bounds::for_cursor(&zero,&data.length,&[]);
        for identity in data.length.component_origins(0) { proof.mark_domain(0,IndexDomain::Upper(identity,1)); }
        let index=Value{ty:scalar.clone(),regs:vec![index_reg],proof,uniform:false,literal:false,constant:None};
        let arguments=vec![Argument::Value(index.clone())];
        let instance=self.specialized_arguments(key,&arguments,explicit,factory.span)?;
        let function=&instance.definition.function;
        if function.kind!=FunctionKind::Value || function.target!=Target::Cpu || function.params.len()!=1 || function.params[0].access!=Access::Value {
            return self.fail(factory.span,"B224","dataset生成函数接收一个按值u32下标并返回一个元素，使用普通on cpu函数");
        }
        if data.length.maximum()==0 {
            let definition=&instance.definition;
            let input=cooperative::parameter_type(self.world,&function.params[0],definition)?;
            self.require_same(&scalar,&input,factory.span)?;
            let output=self.world.ty(&function.result,&definition.source,function.span,&definition.module,&definition.imports)?;
            self.require_same(&data.ty,&output,factory.span)?;
            return Ok(self.unknown_element_model(&data.ty));
        }
        let slot=self.local_slots(&scalar).0[0];self.code.push(Inst::generated(InstKind::StoreLocal(slot,zero.regs[0]),cursor));
        let condition_origin=origin(GenerationReason::DatasetInitializeCondition);
        let outer=std::mem::take(&mut self.code);self.code.push(Inst::generated(InstKind::LoadLocal(index_reg,slot),condition_origin.clone()));
        let test=self.reg(Scalar::Bool);self.code.push(Inst::generated(InstKind::Binary{dst:test,op:ir::BinaryOp::Compare(ir::Comparison::Less),a:index_reg,b:data.length.regs[0]},condition_origin));
        let condition=std::mem::take(&mut self.code);
        // 构造拥有者的可结束范围就是原游标之前的初始化前缀；Frame仍持有完整分配。
        let mut prefix=data.clone();prefix.length=index.clone();prefix.count=None;prefix.initialization=None;
        let owner=self.hold_binding(Binding::ApplicationData(prefix));
        let Some(mut returned)=self.invoke_returned(instance,arguments,span,Some(origin(GenerationReason::DatasetInitializeCall)),None)? else{return self.fail(span,"B224","dataset生成函数必须交付一个实际元素");};
        self.check_returned_type(&mut returned,&data.ty,span)?;self.require_returned_initialized(&mut returned,span)?;
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
                returns::Returned::Dataset(data)=>bind_indices(&mut data.elements,ordinal,depth+1),
                _=>{},
            }
        }
        bind_indices(&mut model,ordinal,0);
        let elements=self.capture_element_model(data,&[],model);let value=self.returned_storage(&returned);
        if let Some(Binding::ApplicationData(prefix))=self.env.get_mut(&owner){prefix.elements=elements.clone();}
        // 原factory合同形成的模型交给此前捕获的同一前缀事实，不重建代码或调用图。
        native_function::calls(&mut self.code,&mut |call|{
            if let Some(FailureContinuation::Scope(scope))=&mut call.failure {
                if let Some(Binding::ApplicationData(prefix))=scope.env.get_mut(&owner){prefix.elements=elements.clone();}
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
        self.complete_application_elements(&DataReference::direct(data.clone()), &elements, &[], span, origin(GenerationReason::DatasetInitializeComplete))?;
        self.env.remove(&owner);
        Ok(elements)
    }
    pub(super) fn complete_application_loop(&mut self, writes: &effects::LoopWrites, span: Span) -> Result<()> {
        let data = self.env.values().flat_map(Binding::application_resources).filter(|data| {
            data.root.ty == Ty::scalar(Scalar::U32)
                && self.resolve_ownership(&data.ownership, None).available()
                && self.resolve_owned_residency(&data.residency, None) == residency::Residency::cpu()
                && (writes.storage.iter().any(|(root, _)| *root == data.root.regs[0])
                    || data.place.as_ref().is_some_and(|place| matches!(place.storage(),
                        Place::Local(_, slots, _) if slots.iter().any(|slot| writes.locals.contains(slot)))))
        }).cloned().collect::<Vec<_>>();
        let origin = Arc::new(GeneratedOrigin {
            reason: GenerationReason::DatasetLoopComplete,
            trigger: self.source_site.clone().map(GenerationAnchor::Syntax),
        });
        let mut completed = BTreeSet::new();
        for data in data {
            if completed.insert(data.root.identity()) {
                let data = self.refresh_application_data(data, span)?;
                self.complete_application_elements(&DataReference::direct(data.clone()), &data.elements, &[], span, origin.clone())?;
            }
        }
        Ok(())
    }
    pub(super) fn complete_application_elements(
        &mut self, data: &DataReference, elements: &returns::Returned, snapshots: &[&Value], span: Span,
        origin: Arc<GeneratedOrigin>,
    ) -> Result<()> {
        if data.root.ty.components[0].0 != Scalar::U32 { return Ok(()); }
        let ty = data.element_layout().1;
        let model = self.returned_storage(elements);
        let mut ranges = model.proof.element_ranges(0, ty).into_iter()
            .map(|range| (range.offset, range.intervals)).collect::<BTreeMap<_, _>>();
        let mut candidates = vec![data.length.clone()];
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
                let proof = model.proof.project(&[component], model.regs.len());
                let domains = proof.domains(0);
                let origins = model.component_origins(component);
                for length in &candidates {
                    let sources = length.component_origins(0).into_iter().filter(|id| !matches!(id,
                        ValueIdentity::ElementComponent(_) | ValueIdentity::ElementParent(..)
                            | ValueIdentity::ElementIndex(_) | ValueIdentity::DatasetContent(..))).collect::<Vec<_>>();
                    let strict = proof.below_length(&origins, &length.proof, 0, true)
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
            let intervals = ranges.remove(&offset);
            if intervals.is_some() || !limits.is_empty() {
                fields.push(crate::application::ElementCompletion {
                    offset, intervals: intervals.unwrap_or_else(|| vec![(0, scalar.bit_mask())]),
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
    fn drop_application_elements(&mut self,data:&Data,retained:&view::Borrow,span:Span)->Result<()> {
        if data.length.maximum()==0 {return Ok(());}
        let data=self.refresh_application_data(data.clone(),span)?;
        self.application_cpu(&data,span)?;
        let trigger=self.source_site.clone().map(GenerationAnchor::Syntax);
        let origin=|reason|Arc::new(GeneratedOrigin{reason,trigger:trigger.clone()});
        let scalar=Ty::scalar(Scalar::U32);let slot=self.local_slots(&scalar).0[0];
        self.code.push(Inst::generated(InstKind::StoreLocal(slot,data.length.regs[0]),origin(GenerationReason::DatasetDropCursor)));
        let condition_origin=origin(GenerationReason::DatasetDropCondition);
        let outer=std::mem::take(&mut self.code);
        let cursor=self.reg(Scalar::U32);self.code.push(Inst::generated(InstKind::LoadLocal(cursor,slot),condition_origin.clone()));
        let zero=self.constant_with_origin(Scalar::U32,0,Some(condition_origin.clone())).regs[0];let test=self.reg(Scalar::Bool);
        self.code.push(Inst::generated(InstKind::Binary{dst:test,op:ir::BinaryOp::Compare(ir::Comparison::Greater),a:cursor,b:zero},condition_origin));
        let condition=std::mem::take(&mut self.code);
        let index_origin=origin(GenerationReason::DatasetDropIndex);
        let one=self.constant_with_origin(Scalar::U32,1,Some(index_origin.clone())).regs[0];let index=self.reg(Scalar::U32);
        self.code.push(Inst::generated(InstKind::Binary{dst:index,op:ir::BinaryOp::ISub,a:cursor,b:one},index_origin));
        let value_index=Value{ty:scalar,regs:vec![index],proof:bounds::Proof::unsigned(0,data.length.maximum().saturating_sub(1)),uniform:false,literal:false,constant:None};
        let place=Self::application_element(&data,&value_index,(value_index.minimum(),value_index.maximum()));
        let value=self.read_storage_value(&place,span,Some(origin(GenerationReason::DatasetDropElementRead)))?;
        let element=self.materialize_element(&DataReference::direct(data.clone()),value,&place);self.drop_owned_tree(element,retained,span)?;
        self.code.push(Inst::generated(InstKind::StoreLocal(slot,index),origin(GenerationReason::DatasetDropAdvance)));
        let body=std::mem::replace(&mut self.code,outer);self.code.push(Inst::generated(InstKind::Loop{condition,test,body},origin(GenerationReason::DatasetDropLoop)));Ok(())
    }
    pub(super) fn application_element(
        data: &Data,
        index: &Value,
        index_range: (u32, u32),
    ) -> Place {
        Place::ApplicationElement {
            ty: data.ty.clone(),
            handle: data.root.regs[0],
            identity: data.root.identity(),
            sources: vec![data.root.identity()],
            resource: view::resource_borrow(data),
            initialization: data
                .initialization.clone()
                .map(|scope| (scope, ownership::ApplicationIndex::of(index))),
            index: index.regs[0],
            index_value: match ownership::ApplicationIndex::of(index) {
                ownership::ApplicationIndex::Constant(at)=>bounds::Operand::Constant(Scalar::U32,Bits::from(at)),
                ownership::ApplicationIndex::Value(id,_)=>bounds::Operand::Value(id),
            },
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
    pub(super) fn application_index(
        &mut self,
        data: DataReference,
        index: &Expr,
    ) -> Result<Option<Place>> {
        self.application_cpu(&data, index.span)?;
        let held = self.hold_binding(Binding::DataReference(data.clone()));
        let value = self.expr(index)?;
        self.env.remove(&held);
        let Some(value) = value else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, index.span)?;
        if !self.index_in(
            &value,
            IndexDomain::ApplicationData(data.root.identity()),
            data.length.minimum(),
            None,
        ) {
            return self.fail(
                index.span,
                "B137",
                "dataset 索引需要由本资源的长度条件或有界游标证明",
            );
        }
        Ok(Some(
            data.element(&value, (value.minimum(), value.maximum())),
        ))
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
        self.application_handle_available(identity, span, &[],&self.path)?;
        if bytes != 0 { self.require_view_unborrowed(&self.data_borrow(&[identity]),span)?; }
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
                value.proof=value.proof.with_view_borrow(0,self.data_value_borrow(&data.data));
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
        root.proof=root.proof.with_view_borrow(0,view::Borrow::projection(self.data_value_borrow(&data.data),data.projection.clone()));
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
    pub(super) fn target_call(
        &mut self,
        target: Target,
        call: &Expr,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        if self.application.is_none() {
            return self.fail(
                span,
                "B266",
                "target 调用由 CPU 主流程发起，元素实例与协作成员不能选择发起位置",
            );
        }
        let (name, explicit, args) = match &call.kind {
            ExprKind::Call(name, args) => (name, None, args),
            ExprKind::Specialize(name, explicit, args) => (name, Some(explicit.as_slice()), args),
            _ => {
                return self.fail(
                    span,
                    "B266",
                    "target 包围一次实际函数调用，例如 target(gpu, step(data))",
                );
            }
        };
        if target == Target::Cpu {
            return self.evaluate_returned(call, None);
        }
        let key = self
            .world
            .resolve(
                name,
                &self.definition.module,
                &self.definition.imports,
                true,
            )
            .ok_or_else(|| Diagnostic {
                source: self.definition.source.clone(),
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
        let Some(arguments) = self.function_arguments(&key, args, span)? else {
            return Ok(None);
        };
        let instance = self.specialized_arguments(key, &arguments, explicit, span)?;
        self.application_execute(instance, arguments, target, span)
    }
    pub(super) fn application_execute(
        &mut self,
        instance: crate::generics::Instance,
        prepared: Vec<Argument>,
        target: Target,
        span: Span,
    ) -> Result<Option<returns::Returned>> {
        self.application_frame(span)?;
        let callee = &instance.definition;
        if callee.function.target != Target::Compute && callee.function.target != target {
            return self.fail(span, "B266", "函数声明的目标能力与本次调用位置不一致");
        }
        let mut data = Vec::new();
        let mut uniforms = Vec::new();
        let mut held = Vec::new();
        for (parameter, value) in callee.function.params.iter().zip(prepared) {
            if parameter.ty.dataset {
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
                    .require(&value.residency, target, &self.definition.source, span,&path)
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
                data.push(value);
            } else {
                let Argument::Value(value) = value else {
                    return self.fail(span, "B266", "当前整域统一参数需要普通值");
                };
                uniforms.push(value);
            }
        }
        for value in &data {
            self.application_handle_available(value.root.identity(), span, &[],&self.path)?;
        }
        let interface = self.prepare_domain(&instance, target)?;
        for &(left, right) in &interface.length_requirements {
            if !self.lengths_equal(&data[left].length, &data[right].length) {
                return self.fail(span, "B226", "整域调用前未证明same_length数据集长度关系");
            }
        }
        for (value, buffer) in data.iter().zip(&interface.buffers) {
            self.require_same(&buffer.ty, value.element_layout().1, span)?;
            if buffer.count.is_some() && buffer.count != value.count {
                return self.fail(span, "B167", "调用方没有交付函数需要的数据形状");
            }
        }
        for (value, uniform) in uniforms.iter().zip(&interface.uniforms) {
            self.require_same(&uniform.ty, &value.ty, span)?;
        }
        let roots = interface.buffers.iter().enumerate().map(|(binding, buffer)|
            (buffer.name.clone(), Root::Data(binding, buffer.access))).collect();
        for requirement in prefix::element_requirements(&interface.buffers, &roots, callee)? {
            let ElementFact::Index { binding, offset, target } = requirement else { continue; };
            let source = &data[binding];
            if source.length.maximum() == 0 { continue; }
            let model = self.returned_storage(&source.elements);
            let word = source.element_layout().0 + offset;
            let component = source.data.ty.components.iter().position(|(_, at)| *at == word).unwrap();
            let value = self.value_component(&model, component);
            let target = &data[target];
            if !self.index_in(&value, IndexDomain::ApplicationData(target.root.identity()), target.length.minimum(), None) {
                return self.fail(span, "B226", "整域调用前尚未证明共同字段的目标索引前提");
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
            self.update_application_contents(reference, &words, update);
        }
        self.initialize_application_domain(&interface, &data);
        let frame = self.application_frame(span)?;
        let mut physical = vec![argument(frame)];
        let mut data = data.into_iter();
        let mut uniforms = uniforms.into_iter();
        for parameter in &callee.function.params {
            if parameter.ty.dataset {
                let value = data.next().unwrap();
                physical.push(self.application_domain_argument(&value));
            } else {
                physical.push(argument(uniforms.next().unwrap()));
            }
        }
        self.call_domain(instance, target, physical);
        for name in held {
            self.finish_scoped_argument(&name, span)?;
        }
        Ok(Some(returns::Returned::Unit))
    }
    #[inline(never)]
    pub(super) fn application_parallel(&mut self, statement: &Stmt) -> Result<()> {
        if self.application.is_none() {
            return self.fail(
                statement.span,
                "B131",
                "并行域由主流程或整域函数发起；元素实例中的嵌套域尚未实现",
            );
        }
        let mut names = BTreeSet::new();
        captured_names(std::slice::from_ref(statement), &mut names);
        let mut params = Vec::new();
        let mut args = Vec::new();
        for name in names {
            let Some(binding) = self.env.get(&name).cloned() else {
                continue;
            };
            let expr = Expr {
                kind: ExprKind::Name(name.clone()),
                span: statement.span,
            };
            let (ty, access, arg) = if matches!(binding, Binding::ApplicationData(_) | Binding::DataReference(_)) {
                let data = self.application_reference(&expr)?.unwrap();
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
                let value = self.name_expr(&name, &expr)?;
                (value.ty.syntax(), Access::Value, Argument::Value(value))
            };
            params.push(Param {
                name,
                ty,
                access,
                span: statement.span,
            });
            args.push(arg);
        }
        self.check_argument_aliases(&params, &args, |_| statement.span)?;
        let mut contracts = Vec::new();
        let mut groups: Vec<usize> = Vec::new();
        for (index, argument) in args.iter().enumerate() {
            let Argument::BorrowedData(data) = argument else { continue; };
            let representative = groups.iter().copied().find(|&previous| {
                let Argument::BorrowedData(other) = &args[previous] else { return false; };
                self.lengths_equal(&data.length, &other.length)
            });
            if let Some(previous) = representative {
                contracts.push(FunctionContract {
                    kind: ContractKind::Requires,
                    predicate: ContractPredicate::SameLength {
                        left: params[previous].name.clone(),
                        right: params[index].name.clone(),
                    },
                    span: statement.span,
                });
            } else {
                groups.push(index);
            }
        }
        let name = format!(
            "{}@parallel{}:{:?}",
            self.calls.last().unwrap(),
            statement.span.start,
            params.iter().map(|p| (&p.ty, p.access)).collect::<Vec<_>>()
        );
        let function = Function {
            name: name.clone(),
            types: Vec::new(),
            values: Vec::new(),
            params,
            result: TypeName::named("void"),
            kind: FunctionKind::Value,
            target: Target::Cpu,
            contracts,
            body: Some(vec![statement.clone()]),
            c_abi: false,
            external: None,
            span: statement.span,
        };
        let definition = Arc::new(Definition {
            function,
            syntax_origin: crate::frontend::DefinitionSyntax::Generated(self.source_at(statement.span, SourceSyntax::Statement)),
            execution: crate::execution::Requirement::Domain,
            source: self.definition.source.clone(),
            module: self.definition.module.clone(),
            imports: self.definition.imports.clone(),
            public: false,
            compiled: None,
        });
        self.application_execute(
            crate::generics::Instance { name, definition },
            args,
            Target::Cpu,
            statement.span,
        )?;
        Ok(())
    }
    pub(super) fn application_cpu(&mut self, data: &Data, span: Span) -> Result<()> {
        let path=self.residency_path();
        if !self
            .residency
            .require(&data.residency, Target::Cpu, &self.definition.source, span,&path)
        {
            return self.fail(
                span,
                "B266",
                "CPU 访问需要 CPU 存储；先显式 download，条件分支也须统一存储位置",
            );
        }
        Ok(())
    }
    pub(super) fn application_transfer(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        self.application_frame(span)?;
        if args.len() != 1 {
            return self.fail(span, "B266", "upload／download 需要一个拥有的 dataset 名称");
        }
        let Some(data) = self.application_reference(&args[0])? else {
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
        let borrow=self.view_uses.all(self,&[]);self.application_movable(&data,&borrow,span)?;
        let (source, target) = if name == "upload" {
            (Target::Cpu, residency::Residency::gpu())
        } else {
            (Target::Gpu, residency::Residency::cpu())
        };
        let path=self.residency_path();
        if !self
            .residency
            .require(&data.residency, source, &self.definition.source, span,&path)
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
    pub(super) fn println(&mut self, args: &[Expr], span: Span) -> Result<Option<Value>> {
        if self.program.target != Target::Cpu {
            return self.fail(span, "B260", "println 是 CPU 标准输出操作，要求显式 on cpu");
        }
        if args.len() != 1 {
            return self.fail(span, "B261", "println 需要一个整数、f32、f64 或 bool 标量");
        }
        let Some(value) = self.expr(&args[0])? else {
            return Ok(None);
        };
        if !value.ty.components.first().is_some_and(|(scalar, _)| value.ty == Ty::scalar(*scalar))
        {
            return self.fail(
                args[0].span,
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
pub(super) fn rebound(body: &[Stmt], name: &str) -> bool {
    fn root(expr: &Expr) -> Option<&str> {
        match &expr.kind {
            ExprKind::Name(name) => Some(name),
            ExprKind::Member(base, _) => root(base),
            _ => None,
        }
    }
    body.iter().any(|s| match &s.kind {
        StmtKind::Assign(lhs, _, _) => root(lhs) == Some(name),
        StmtKind::If(_, yes, no) => {
            rebound(yes, name) || rebound(no, name)
        }
        StmtKind::Match(_, arms) => arms.iter().any(|arm| rebound(&arm.body,name)),
        StmtKind::For(_, _, _, body) | StmtKind::While(_, body) => rebound(body, name),
        _ => false,
    })
}

fn captured_names(body: &[Stmt], names: &mut BTreeSet<String>) {
    fn name(name: &str, bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        if !bound.contains(name) {
            names.insert(name.to_owned());
        }
    }
    fn expr(e: &Expr, bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        match &e.kind {
            ExprKind::Name(n) => name(n, bound, names),
            ExprKind::Invoke(callee, args) => {
                expr(callee, bound, names);
                for arg in args { expr(arg, bound, names); }
            }
            ExprKind::Call(callee, args) => {
                name(callee, bound, names);
                for arg in args { expr(arg, bound, names); }
            }
            ExprKind::Specialize(_, _, args)
            | ExprKind::Construct(_, args)
            | ExprKind::Cooperate(_, _, args) => {
                for a in args {
                    expr(a, bound, names);
                }
            }
            ExprKind::Target(_, e) | ExprKind::Unary(_, e) | ExprKind::Member(e, _) => {
                expr(e, bound, names)
            }
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
                expr(a, bound, names);
                expr(b, bound, names);
            }
            ExprKind::Select(a, b, c) => {
                expr(a, bound, names);
                expr(b, bound, names);
                expr(c, bound, names);
            }
            _ => {}
        }
    }
    fn block(body: &[Stmt], bound: &BTreeSet<String>, names: &mut BTreeSet<String>) {
        let mut bound = bound.clone();
        for s in body {
            match &s.kind {
                StmtKind::Parallel(iterator, domain, body) => {
                    name(&domain.dataset, &bound, names);
                    if let Some(e) = &domain.count {
                        expr(e, &bound, names);
                    }
                    if let Some(e) = &domain.selection {
                        expr(e, &bound, names);
                    }
                    let mut nested = bound.clone();
                    nested.insert(iterator.clone());
                    block(body, &nested, names);
                }
                StmtKind::Let(_, local, e) => {
                    expr(e, &bound, names);
                    bound.insert(local.clone());
                }
                StmtKind::Declare(_, local) => {
                    bound.insert(local.clone());
                }
                StmtKind::Expr(e) | StmtKind::Return(Some(e)) | StmtKind::Seek(_, e) => {
                    expr(e, &bound, names)
                }
                StmtKind::Assign(a, _, b) => {
                    expr(a, &bound, names);
                    expr(b, &bound, names);
                }
                StmtKind::If(e, a, b) => {
                    expr(e, &bound, names);
                    block(a, &bound, names);
                    block(b, &bound, names);
                }
                StmtKind::Match(e, arms) => {
                    expr(e,&bound,names);
                    for arm in arms {let mut nested=bound.clone();nested.extend(arm.binding.iter().cloned());block(&arm.body,&nested,names);}
                }
                StmtKind::While(e, body) => {
                    expr(e, &bound, names);
                    block(body, &bound, names);
                }
                StmtKind::For(iterator, a, b, body) => {
                    expr(a, &bound, names);
                    if let Some(b) = b {
                        expr(b, &bound, names);
                    }
                    let mut nested = bound.clone();
                    nested.insert(iterator.clone());
                    block(body, &nested, names);
                }
                _ => {}
            }
        }
    }
    block(body, &BTreeSet::new(), names);
}

// 函数出口已要求完整拥有值；局部条件与Place一样不进入模块接口。
impl crate::module::codec::Encode for Ownership{fn encode(&self,w:&mut crate::module::codec::Writer){let tag:u32=match self{Self::Borrowed=>1,Self::Moved=>2,Self::Owned|Self::Conditional(_)=>0};crate::module::codec::Encode::encode(&tag,w);}}
impl crate::module::codec::Decode for Ownership{fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{match <u32 as crate::module::codec::Decode>::decode(r)?{0=>Ok(Self::Owned),1=>Ok(Self::Borrowed),2=>Ok(Self::Moved),_=>Err("invalid ownership tag".into())}}}
// slots与初始化位置属于当前编译调用；实际原生接口只交接已初始化资源。
impl crate::module::codec::Encode for DataPayload {
    fn encode(&self,w:&mut crate::module::codec::Writer) {
        self.root.encode(w);self.ty.encode(w);self.length.encode(w);self.ownership.encode(w);self.access.encode(w);self.count.encode(w);self.residency.encode(w);self.layout.encode(w);self.elements.encode(w);
    }
}
impl crate::module::codec::Decode for DataPayload {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self> {
        Ok(Self{root:Value::decode(r)?,ty:Ty::decode(r)?,length:Value::decode(r)?,ownership:Ownership::decode(r)?,access:Access::decode(r)?,count:Option::decode(r)?,residency:residency::Residency::decode(r)?,layout:DataLayout::decode(r)?,elements:Box::<returns::Returned>::decode(r)?,place:None,initialization:None})
    }
}

// Common element metadata belongs to the original logical Data.
type ElementBindings = BTreeMap<ValueIdentity,bounds::Proof>;
pub(super) fn element_types(ty:&Ty)->Vec<&Ty> {
    fn visit<'a>(ty:&'a Ty, out:&mut Vec<&'a Ty>) {
        if let Some(element)=ty.dataset.as_deref(){out.push(element);return;}
        if ty.has_variants(){for variant in &ty.variants{visit(&variant.ty,out);}}
        else{for index in 0..ty.aggregate_len(){visit(ty.aggregate_part(index).1,out);}}
    }
    let mut out=Vec::new();visit(ty,&mut out);out
}
fn element_width(ty:&Ty)->usize {
    ty.components.len()+element_types(ty).into_iter().map(element_width).sum::<usize>()
}
pub(super) fn model_data<'a>(model:&'a returns::Returned,path:&[usize])->Option<&'a Data> {
    match model {
        returns::Returned::Dataset(data) if path.is_empty()=>Some(data),
        returns::Returned::OwnedValue(value)=>view::owned_data_at(value,path),
        _=>None,
    }
}
pub(super) fn model_data_mut<'a>(model:&'a mut returns::Returned,path:&[usize])->Option<&'a mut Data> {
    match model {
        returns::Returned::Dataset(data) if path.is_empty()=>Some(data),
        returns::Returned::OwnedValue(value)=>view::owned_data_at_mut(value,path),
        _=>None,
    }
}
fn binding_element_types(binding:&Binding)->Vec<&Ty> {
    match binding {
        Binding::ApplicationData(data)=>vec![&data.ty],
        Binding::DataReference(data)=>vec![&data.ty],
        Binding::OwnedValue(value)=>element_types(&value.value.ty),
        _=>Vec::new(),
    }
}
fn binding_element_data<'a>(binding:&'a Binding,resource:usize)->Option<&'a Data> {
    match binding {
        Binding::ApplicationData(data) if resource==0=>Some(data),
        Binding::DataReference(data) if resource==0=>Some(&data.data),
        Binding::OwnedValue(value)=>view::resource_paths(&value.value.ty).get(resource)
            .and_then(|path|view::owned_data_at(value,path)),
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
pub(super) fn canonical_element_inputs(builder:&Builder<'_>,bindings:&mut ElementBindings) {
    for(parameter,p)in builder.definition.function.params.iter().enumerate(){
        if let Some(binding)=builder.env.get(&p.name){
            for(resource,ty)in binding_element_types(binding).into_iter().enumerate(){
                for component in 0..element_width(ty){
                    let id=ValueIdentity::DatasetContent(parameter,resource,component);
                    bindings.insert(id,bounds::Proof::default().with_identity(id).with_view_sources(0,BTreeSet::from([view::Source::Parameter(id,Vec::new())])));
                }
            }
        }
    }
}
pub(super) fn element_input_bindings(builder:&mut Builder<'_>,definition:&Definition,env:&BTreeMap<String,Binding>)->(ElementBindings,view::Borrow) {
    fn collect(builder:&Builder<'_>,data:&Data,parent:view::Borrow,mut bindings:ElementBindings,parameter:usize,resource:usize,next:&mut usize,out:&mut ElementBindings,memory:&mut view::Borrow,reads:bool){
        let model=&data.elements;let value=builder.returned_storage(model);let width=value.regs.len();
        for component in 0..width{
            let id=ValueIdentity::ElementComponent(component);
            bindings.insert(id,bounds::Proof::default().with_identity(id));
        }
        builder.bind_element_resource_borrows(data,&parent,&mut bindings);
        let mut proof=bounds::Proof::capture(&value);
        let model_values=proof.scalar_identities().into_iter().filter(|id|matches!(id,ValueIdentity::ElementComponent(_)))
            .map(|id|(id,bounds::Proof::default())).collect();
        let mut references=view_contracts::components(&value.ty);
        for fields in view::resource_paths(&value.ty){if let Some(data)=model_data(model,&fields){
            references.insert(value.regs.iter().position(|reg|*reg==data.root.regs[0]).unwrap());
        }}
        for component in 0..width{
            let borrow=proof.view_borrow(component).at_call(&bindings);
            proof=proof.with_view_borrow(component,borrow.clone());
            let mut input=proof.project(&[component],width);
            if !references.contains(&component){input=input.substitute_values(&model_values);}
            let input=input.with_view_borrow(0,borrow);
            out.insert(ValueIdentity::DatasetContent(parameter,resource,*next+component),input);
        }
        if reads{memory.union(&builder.address_contents(&value.ty,&proof));}
        *next+=width;
        for(path,ty)in view::resource_paths(&value.ty).into_iter().zip(element_types(&value.ty)){
            if let Some(child)=model_data(model,&path){
                let nested=element_scope_bindings(&bindings,child.elements.registers().len(),true);
                collect(builder,child,view::Borrow::element(parent.clone(),path,Some(bounds::Operand::Value(ValueIdentity::ElementIndex(0)))),nested,parameter,resource,next,out,memory,reads&&child.length.maximum()!=0);
            }else{*next+=element_width(ty);}
        }
    }
    let mut out=ElementBindings::new();let mut memory=view::Borrow::default();
    for(parameter,p)in definition.function.params.iter().enumerate(){
        if let Some(binding)=env.get(&p.name){
            for resource in 0..binding_element_types(binding).len(){
                if let Some(data)=binding_element_data(binding,resource){
                    let mut projected;
                    let data=if let Binding::DataReference(reference)=binding {
                        projected=data.clone();
                        projected.root.proof=projected.root.proof.clone().with_view_borrow(0,view::Borrow::projection(builder.data_value_borrow(data),reference.projection.clone()));
                        projected.ty=reference.element_layout().1.clone();
                        let mut model=*projected.elements;
                        for field in &reference.projection{model=match model{returns::Returned::OwnedValue(parent)=>builder.unpack_record(&parent,*field),returns::Returned::Value(value)=>returns::Returned::value(builder.record_component(&value,*field)),_=>unreachable!()};}
                        projected.elements=Box::new(model);
                        &projected
                    }else{data};
                    collect(builder,data,builder.data_value_borrow(data),builder.element_owner_bindings(data),parameter,resource,&mut 0,&mut out,&mut memory,p.access!=Access::Write&&data.length.maximum()!=0);
                }
            }
        }
    }
    (out,memory)
}
pub(super) fn bind_parameter_elements(builder:&mut Builder<'_>,definition:&Definition){
    fn bind(builder:&Builder<'_>,model:&mut returns::Returned,parameter:usize,resource:usize,next:&mut usize,inputs:&ElementBindings,depth:usize){
        let value=builder.returned_storage(model);let width=value.regs.len();let mut bindings=inputs.clone();
        for component in 0..width{
            bindings.insert(ValueIdentity::ElementComponent(component),bounds::Proof::default()
                .with_identity(ValueIdentity::ElementComponent(component))
                .with_identity(ValueIdentity::DatasetContent(parameter,resource,*next+component)));
        }
        let original=model.clone();native_function::translate_returned(model,&original,&bindings,true);
        let start=*next;
        if let Some(value)=model.view_value_mut(){
            let indices=(0..depth).rev().map(|depth|Some(bounds::Operand::Value(ValueIdentity::ElementIndex(depth)))).collect::<Vec<_>>();
            value.proof=value.proof.clone().view_parameters(&value.ty,&indices,|component|ValueIdentity::DatasetContent(parameter,resource,start+component));
        }
        *next+=width;
        let children=view::resource_paths(&value.ty).into_iter().zip(element_types(&value.ty).into_iter().cloned()).collect::<Vec<_>>();
        for(path,ty)in children{
            if let Some(data)=model_data_mut(model,&path){bind(builder,&mut data.elements,parameter,resource,next,inputs,depth+1);}
            else{*next+=element_width(&ty);}
        }
    }
    for(parameter,p)in definition.function.params.iter().enumerate(){
        if let Some(mut binding)=builder.env.remove(&p.name){
            each_binding_data(&mut binding,&mut |resource,path,data|{
                let inputs=(0..element_width(&data.ty)).map(|component|{
                    let id=ValueIdentity::DatasetContent(parameter,resource,component);(id,bounds::Proof::default().with_identity(id))
                }).collect();
                bind(builder,&mut data.elements,parameter,resource,&mut 0,&inputs,1);
                if p.access!=Access::Write && data.length.maximum()!=0 {
                    let mut input=residency::Input{parameter,fields:path.iter().copied().map(residency::Part::Field).chain([residency::Part::Elements]).collect()};
                    residency::visit(&mut data.elements,&mut input,&mut |data,input|data.residency=residency::Residency::input(input.model()));
                }
            });
            builder.env.insert(p.name.clone(),binding);
        }
    }
}
impl Builder<'_> {
    pub(super) fn unknown_element_model(&self,ty:&Ty)->Box<returns::Returned>{
        let mut proof=bounds::Proof::default();
        for(component,(scalar,_))in ty.components.iter().enumerate(){
            proof.append(&Value{ty:Ty::scalar(*scalar),regs:vec![component],proof:bounds::Proof::default().with_identity(ValueIdentity::ElementComponent(component)),uniform:false,literal:false,constant:None});
        }
        Box::new(self.template_returned(Value{ty:ty.clone(),regs:(0..ty.components.len()).collect(),proof,uniform:false,literal:false,constant:None},true))
    }
    pub(super) fn owned_resource_borrow(&self,data:&Data)->view::Borrow {
        fn collect(builder:&Builder<'_>,data:&Data,parent:view::Borrow,mut bindings:ElementBindings,out:&mut view::Borrow){
            out.union(&parent);if data.length.maximum()==0{return;}
            builder.bind_element_resource_borrows(data,&parent,&mut bindings);
            for fields in view::resource_paths(&data.ty){if let Some(child)=model_data(&data.elements,&fields){
                let borrow=view::resource_borrow(child).at_call(&bindings);
                let nested=element_scope_bindings(&bindings,child.elements.registers().len(),true);
                collect(builder,child,borrow,nested,out);
            }}
        }
        let mut out=view::Borrow::default();collect(self,data,view::resource_borrow(data),self.element_owner_bindings(data),&mut out);out
    }
    pub(super) fn element_owner_bindings(&self,data:&Data)->ElementBindings {
        fn aliases(borrow:&view::Borrow,out:&mut ElementBindings){
            for source in &borrow.sources{match source{
                view::Source::Data(id@(ValueIdentity::Register(_)|ValueIdentity::ResourceParameter(..)))=>{out.entry(*id).or_insert_with(||bounds::Proof::default().with_identity(*id));},
                view::Source::Parameter(id@ValueIdentity::DatasetContent(..),_)=>{out.entry(*id).or_insert_with(||bounds::Proof::default().with_identity(*id).with_view_borrow(0,view::Borrow::source(view::Source::Parameter(*id,Vec::new()))));},
                view::Source::Element{parent,..}|view::Source::Projection{parent,..}=>aliases(parent,out),
                view::Source::Position(origin)=>aliases(&origin.resources(),out),_=>{},
            }}
        }
        fn owned(data:&Data,out:&mut ElementBindings){
            aliases(&data.root.proof.view_borrow(0),out);
            if let Some(value)=data.elements.view_value(){aliases(&value.proof.typed_view_borrow(&value.ty),out);for id in value.proof.scalar_identities(){if matches!(id,ValueIdentity::MemoryValue(..)){out.entry(id).or_insert_with(||bounds::Proof::default().with_identity(id));}}}
            for fields in view::resource_paths(&data.ty){if let Some(child)=model_data(&data.elements,&fields){owned(child,out);}}
        }
        let mut out=ElementBindings::new();owned(data,&mut out);
        let ordinal=ValueIdentity::ElementIndex(0);out.insert(ordinal,bounds::Proof::default().with_identity(ordinal));
        for value in [&data.root,&data.length]{
            let proof=bounds::Proof::capture(value);
            for id in value.component_origins(0){out.insert(id,proof.clone());}
        }
        for (name,binding) in &self.env{
            let parameter=self.definition.function.params.iter().any(|p|p.name==*name);
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
            let borrowed = match binding { Binding::DataReference(reference) => Some(&reference.data), _ => None };
            for source in view::data_paths(binding).into_iter().map(|(_,source,_)|source).chain(borrowed) {
                for value in [&source.root,&source.length]{
                    let proof=bounds::Proof::capture(value);
                    for id in value.component_origins(0){out.entry(id).or_insert_with(||proof.clone());}
                }
            }
        }
        canonical_element_inputs(self,&mut out);
        out
    }
    pub(super) fn bind_element_resource_borrows(&self,data:&Data,parent:&view::Borrow,bindings:&mut ElementBindings) {
        let shifts=bindings.keys().filter_map(|id|match id{ValueIdentity::ElementIndex(depth)=>Some((*id,ValueIdentity::ElementIndex(depth+1))),_=>None}).collect();
        let parent=parent.output_bound(&shifts);
        let index=bounds::Operand::Value(ValueIdentity::ElementIndex(0)).at_call(bindings);
        for fields in view::resource_paths(&data.ty) {
            if let Some(resource)=model_data(&data.elements,&fields) {
                let id=ValueIdentity::ElementComponent(resource.root.regs[0]);
                let proof=bindings.entry(id).or_default();
                let mut borrow=proof.view_borrow(0);
                borrow.union(&view::Borrow::element(parent.clone(),fields,index));
                borrow.union(&view::resource_aliases(resource));
                *proof=std::mem::take(proof).with_view_borrow(0,borrow);
            }
        }
    }
    pub(super) fn application_element_scopes(&self,sources:&[ValueIdentity],resource:&view::Borrow,data:&Data)->Vec<(Vec<Vec<usize>>,bool)> {
        fn collect(borrow:&view::Borrow,owner:&view::Borrow,path:&bounds::Clause)->Vec<(Vec<Vec<usize>>,bool)> {
            let mut paths=Vec::new();let selected=borrow.restricted(path);let root=owner.restricted(path);
            let shared=selected.sources.entries().any(|(source,when)|matches!(source,view::Source::Data(_)|view::Source::Parameter(..)|view::Source::Element{..})&&root.overlaps_source(source,&when))
                ||selected.calls.entries().any(|(call,when)|when.overlaps(&root.calls.when(call)))
                ||selected.inputs.entries().any(|(input,when)|when.overlaps(&root.inputs.when(input)));
            if shared{paths.push((Vec::new(),selected==root));}
            for(source,when)in borrow.sources.entries(){
                let truth=bounds::Condition::guard(when).truth_in(path);
                if truth==Some(false){continue;}
                match source {
                    view::Source::Element{parent,fields,..}=>for(mut nested,complete)in collect(parent,owner,path){nested.push(fields.clone());paths.push((nested,complete&&truth==Some(true)));},
                    view::Source::Projection{parent,..}=>for(nested,complete)in collect(parent,owner,path){paths.push((nested,complete&&truth==Some(true)));},
                    _=>{},
                }
            }
            paths
        }
        let mut paths=BTreeMap::new();
        if sources.contains(&data.root.identity()){paths.insert(Vec::new(),true);}
        let owner=view::resource_borrow(data);
        let owners=std::iter::once(&owner).chain(self.loop_contexts.iter().filter_map(|context|context.view_resources.get(&data.root.identity())));
        for owner in owners{for(path,complete)in collect(resource,owner,&self.path){paths.entry(path).and_modify(|old|*old|=complete).or_insert(complete);}}
        paths.into_iter().collect()
    }
    pub(super) fn assign_application_element(&mut self,data:DataReference,index:&Expr,projection:&[&Expr],op:&str,rhs:&Expr,span:Span)->Result<()> {
        let Some(mut place)=self.application_index(data.clone(),index)?else{return Ok(())};
        let mut paths=vec![data.projection.clone()];
        for part in projection {match &part.kind {
            ExprKind::Member(_,field)=>{
                let component=place.ty().fields.iter().position(|(name,..)|name==field).unwrap();
                let(ty,components)=member_type(place.ty(),field,&self.definition.source,part.span)?;
                place=place.project(ty,&components);for path in &mut paths{path.push(component);}
            },
            ExprKind::Index(_,index)=>{
                let held=self.hold_binding(Binding::DataReference(data.clone()));let value=self.expr(index)?;self.env.remove(&held);
                let Some(value)=value else{return Ok(())};
                let(minimum,maximum,count)=(value.minimum(),value.maximum(),place.ty().values[0]);
                place=self.array_index(place,value,index.span)?;
                paths=paths.into_iter().flat_map(|path|(0..count).filter(move|i|*i>=minimum&&*i<=maximum).map(move|i|{let mut path=path.clone();path.push(i as usize);path})).collect();
            },
            _=>unreachable!(),
        }}
        self.require_owned_writable(&place,span)?;self.writable_storage(&place,span,true)?;
        if op!="="{return self.fail(span,"B224","拥有内容只支持整份移动赋值");}
        let loaded=self.read_storage_value(&place,span, None)?;
        let mut selected=data.clone();selected.projection=paths[0].clone();let mut old=self.materialize_element(&selected,loaded.clone(),&place);
        for path in &paths[1..]{selected.projection=path.clone();old.join(self.materialize_element(&selected,loaded.clone(),&place),false);}
        let held=self.hold_binding(Binding::DataReference(data.clone()));
        let result=self.evaluate_returned(rhs,Some(place.ty()))?;self.env.remove(&held);
        if !self.flow.continues(){return Ok(());}
        let value=result.ok_or_else(||Diagnostic{source:self.definition.source.clone(),span,code:"B228",message:"拥有赋值需要实际值或资源".into()})?;
        // RHS结束后的原消费者仍指向旧内容；被覆盖位置的后续读取属于新内容。
        self.view_uses.after(rhs.span);
        self.store_owned_returned(place,old,value,span)
    }
    // These are the original already-evaluated metadata positions, not a second element table.
    fn element_selection(place:Option<&Place>,first:usize,count:usize)->bounds::Condition {
        let mut current=place;let mut selected=bounds::Condition::constant(true);
        for depth in first..first+count{
            let Some(Place::ApplicationElement{index_value,parent,..})=current.map(Place::storage)else{return bounds::Condition::default()};
            selected=selected.logical(bounds::Condition::element_index(depth,*index_value),false);current=parent.as_deref();
        }
        selected
    }
    fn assign_application_element_model(&mut self,data:&DataReference,place:&Place,paths:&[Vec<usize>],value:&returns::Returned){
        let resource=view::resource_borrow(&data.data);let one=self.constant(Scalar::U32,1);
        let names=self.env.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let mut binding=self.env.remove(&name).unwrap();
            each_binding_data(&mut binding,&mut|_,_,target|{
                let scopes=self.application_element_scopes(&data.sources,&resource,target);let unique=scopes.len()==1;
                for(scope,complete_scope)in scopes {
                let count=scope.len()+1;
                let qualifier=if unique&&complete_scope&&paths.len()==1{bounds::Condition::constant(true)}else{bounds::Condition::default()};
                let selected=qualifier.logical(Self::element_selection(Some(place),0,count),false);
                let mut complete_scope=bounds::Condition::constant(complete_scope);
                let mut target=&mut *target;
                for fields in scope{complete_scope=complete_scope.logical(bounds::Condition::comparison(Comparison::Equal,&target.length,&one),false);target=model_data_mut(&mut target.elements,&fields).unwrap();}
                for projection in paths {
                    let mut fresh=self.capture_element_model(target,projection,value.clone());
                    let mut previous=(*target.elements).clone();
                    for field in projection{let returns::Returned::OwnedValue(parent)=previous else{unreachable!()};previous=self.unpack_record(&parent,*field);}
                    let complete=bounds::Condition::constant(unique&&paths.len()==1&&matches!(place.storage(),Place::ApplicationElement{index_range:(0,0),..})).logical(complete_scope.clone(),false).logical(bounds::Condition::comparison(Comparison::Equal,&target.length,&one),false);
                    let truth=complete.truth_in(&self.path);
                    let locations=(truth!=Some(true)).then(||(fresh.clone(),previous.clone()));
                    match truth{
                        Some(true)=>{},Some(false)=>fresh.join(previous,true),
                        None=>{let mut common=fresh.clone();common.join(previous,true);fresh.join_locations(*common,&complete,true);},
                    }
                    if let Some((incoming,old))=locations{residency::select_elements(&mut fresh,&incoming,&old,&selected,count);}
                    let mut stored=target.elements.as_mut();
                    for field in projection{let returns::Returned::OwnedValue(parent)=stored else{unreachable!()};stored=parent.payloads[*field].as_deref_mut().unwrap();}
                    *stored=*fresh;
                }
                }
            });
            self.env.insert(name,binding);
        }
    }
    fn application_owned_references(&self,place:&Place)->Vec<(DataReference,Vec<Vec<usize>>)>{
        fn projections(ty:&Ty,wanted:&Ty,words:&[u32],indexed:bool,offset:u32,path:&mut Vec<usize>,into:&mut Vec<Vec<usize>>){
            let actual=ty.components.iter().map(|(_,word)|offset+word).collect::<Vec<_>>();
            if ty==wanted && (if indexed{actual.iter().all(|word|words.contains(word))}else{actual==words}){into.push(path.clone());return;}
            if !ty.owns_resources() || ty.has_variants(){return;}
            for field in 0..ty.aggregate_len(){let(at,child)=ty.aggregate_part(field);if child.owns_resources(){path.push(field);projections(child,wanted,words,indexed,offset+at,path,into);path.pop();}}
        }
        let Place::ApplicationElement{sources,resource,words,..}=place.storage()else{unreachable!()};
        let mut references=Vec::new();
        for binding in self.env.values(){for index in 0..binding_element_types(binding).len(){if let Some(data)=binding_element_data(binding,index){
            for(scope,_)in self.application_element_scopes(sources,resource,data){
                let mut parent=self.data_value_borrow(data);let mut data=data;
                for fields in scope{parent=view::Borrow::element(parent,fields.clone(),None);data=model_data(&data.elements,&fields).unwrap();}
                let mut paths=Vec::new();projections(&data.ty,place.ty(),words,matches!(place,Place::Indexed{..}),0,&mut Vec::new(),&mut paths);
                if !paths.is_empty(){let mut data=data.clone();data.root.proof=std::mem::take(&mut data.root.proof).with_view_borrow(0,parent);references.push((DataReference::direct(data),paths));}
            }
        }}}
        references
    }
    pub(super) fn application_owned_argument(&mut self,place:&Place,span:Span)->Result<returns::Returned>{
        let loaded=self.read_storage_value(place,span,None)?;
        let references=self.application_owned_references(place);let mut values=Vec::new();
        for(mut data,paths)in references{for path in paths{data.projection=path;values.push(self.materialize_element(&data,loaded.clone(),place));}}
        let mut models=values.into_iter();
        let mut model=models.next().unwrap();for other in models{model.join(other,false);}
        if let returns::Returned::OwnedValue(value)=&mut model{value.place=Some(place.clone());}
        Ok(model)
    }
    pub(super) fn assign_application_owned_argument(&mut self,place:&Place,value:&returns::Returned){
        for(data,paths)in self.application_owned_references(place){self.assign_application_element_model(&data,place,&paths,value);}
    }
    fn capture_element_model(&self,data:&Data,projection:&[usize],mut model:returns::Returned)->Box<returns::Returned>{
        model.bind_outputs(None,&BTreeMap::new(),&bounds::Clause::default());
        let value=self.returned_storage(&model);let mut bindings=self.element_owner_bindings(data);
        let mut components=(0..data.ty.components.len()).collect::<Vec<_>>();let mut ty=&data.ty;
        for field in projection{let(offset,next)=ty.aggregate_part(*field);components=next.components.iter().map(|(_,at)|components[ty.components.iter().position(|(_,word)|*word==offset+at).unwrap()]).collect();ty=next;}
        for (component,stored) in components.iter().enumerate(){
            let id=ValueIdentity::ElementComponent(*stored);let proof=bounds::Proof::default().with_identity(id);
            bindings.insert(id,proof.clone());
            for source in value.component_origins(component){bindings.entry(source).or_insert_with(||proof.clone());}
        }
        for fields in view::resource_paths(&value.ty){if let Some(resource)=model_data(&model,&fields){
            let(mut offset,mut ty)=(0,&value.ty);
            for field in fields{let(at,next)=if ty.has_variants(){(ir::align_up(4,ty.align),ty.variants[field].ty.as_ref())}else{ty.aggregate_part(field)};offset+=at;ty=next;}
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
    pub(super) fn materialize_element(&mut self,data:&DataReference,value:Value,place:&Place)->returns::Returned{
        fn loaded_value(builder:&Builder<'_>,target:&mut Value,loaded:&Value){
            let mut proof=bounds::Proof::default();
            for(component,reg)in target.regs.iter().enumerate(){
                let index=loaded.regs.iter().position(|source|source==reg).unwrap();
                let mut part=builder.value_component(target,component);
                part.proof=builder.path_proof(&part.ty,part.proof.with_identity(loaded.component_origins(index)[0]));
                proof.append(&part);
            }
            target.proof=proof;
        }
        fn loaded_model(builder:&Builder<'_>,model:&mut returns::Returned,loaded:&Value){
            match model{
                returns::Returned::Value(target)=>loaded_value(builder,target,loaded),
                returns::Returned::OwnedValue(target)=>{
                    loaded_value(builder,&mut target.value,loaded);
                    for payload in target.payloads.iter_mut().flatten(){loaded_model(builder,payload,loaded);}
                },
                returns::Returned::Dataset(target)=>{
                    loaded_value(builder,&mut target.root,loaded);
                    loaded_value(builder,&mut target.length,loaded);
                    target.length.proof=std::mem::take(&mut target.length.proof).with_domains(Domains::from([IndexDomain::ApplicationLength(target.root.identity())]));
                },
                returns::Returned::Unit=>{},
            }
        }
        let mut model=(*data.elements).clone();let mut components=(0..data.ty.components.len()).collect::<Vec<_>>();
        for field in &data.projection{
            let returns::Returned::OwnedValue(owner)=model else{unreachable!()};
            let(offset,ty)=owner.value.ty.aggregate_part(*field);
            components=ty.components.iter().map(|(_,at)|components[owner.value.ty.components.iter().position(|(_,word)|*word==offset+at).unwrap()]).collect();
            model=self.unpack_record(&owner,*field);
        }
        let original=model.clone();let storage=self.returned_storage(&model);let mut bindings=self.element_owner_bindings(data);
        let mut selected=Some(place);let mut depth=0;
        while let Some(Place::ApplicationElement{index_value,index_range,parent,..})=selected.map(Place::storage){
            let proof=bounds::Proof::element_operand(*index_value,*index_range);
            bindings.insert(ValueIdentity::ElementIndex(depth),proof);
            selected=parent.as_deref();depth+=1;
        }
        for(component,index)in components.into_iter().zip(0..value.regs.len()){
            let mut proof=bounds::Proof::default();
            proof.origins=vec![storage.proof.origins.get(index).cloned().unwrap_or_default()];
            let proof=self.path_proof(&Ty::scalar(value.ty.components[index].0),proof);
            let loaded=bounds::Proof::default().with_identity(value.component_origins(index)[0]);
            let renamed=proof.origins[0].iter().map(|source|(*source,loaded.clone())).collect();
            let proof=proof.substitute_values(&renamed);
            bindings.insert(ValueIdentity::ElementComponent(component),proof.clone());
            for source in storage.component_origins(index).into_iter().filter(|source|matches!(source,ValueIdentity::DatasetContent(..))){
                bindings.insert(source,proof.clone().with_view_borrow(0,storage.proof.view_borrow(index)));
            }
        }
        self.bind_element_resource_borrows(data,&self.data_value_borrow(data),&mut bindings);
        native_function::rebind_model(&mut model,value.regs.clone());
        native_function::translate_returned(&mut model,&original,&bindings,true);
        loaded_model(self,&mut model,&value);
        let borrow=self.data_value_borrow(&data.data);
        owned_borrows::each_data(&mut model,&mut Vec::new(),&mut|child,path|{
            let mut fields=data.projection.clone();fields.extend_from_slice(path);
            let index=match place.storage(){Place::ApplicationElement{index_value,..}=>Some(*index_value),_=>None};
            child.root.proof=child.root.proof.clone().with_view_borrow(0,child.root.proof.view_borrow(0).joined(view::Borrow::element(borrow.clone(),fields,index)));
        });
        model
    }
    pub(super) fn application_element_proof(&self,place:&Place,regs:&[Reg])->Option<bounds::Proof>{
        let Place::ApplicationElement{sources,resource:borrow,words,index_value,index_range,..}=place else{return None};let mut result=None;
        for binding in self.env.values(){
            for resource in 0..binding_element_types(binding).len(){
                if let Some(data)=binding_element_data(binding,resource)
                    && self.resolve_ownership(&data.ownership,None).available(){
                    for(scope,_)in self.application_element_scopes(sources,borrow,data){
                    let mut data=data;let mut parent=self.data_value_borrow(data);for fields in scope{parent=view::Borrow::element(parent,fields.clone(),None);data=model_data(&data.elements,&fields)?;}
                    let components=words.iter().map(|word|data.ty.components.iter().position(|(_,at)|at==word)).collect::<Option<Vec<_>>>()?;
                    let value=self.returned_storage(&data.elements);let mut bindings=self.element_owner_bindings(data);
                    for(component,reg)in components.iter().zip(regs){
                        let proof=bounds::Proof::default().with_identity(ValueIdentity::Register(*reg));
                        bindings.insert(ValueIdentity::ElementComponent(*component),proof.clone());
                        for source in value.component_origins(*component).into_iter().filter(|source|matches!(source,ValueIdentity::DatasetContent(..)|ValueIdentity::MemoryValue(..))){
                            bindings.insert(source,proof.clone().with_view_borrow(0,value.proof.view_borrow(*component)));
                        }
                    }
                    bindings.insert(ValueIdentity::ElementIndex(0),bounds::Proof::element_operand(*index_value,*index_range));
                    self.bind_element_resource_borrows(data,&parent,&mut bindings);
                    // Stored model slots are not registers in the caller function.
                    let mut proof=value.proof.project(&components,value.regs.len()).at_call(&bindings);
                    for (target,component) in components.iter().enumerate() {
                        for source in value.component_origins(*component).into_iter().filter(|source|matches!(source,ValueIdentity::DatasetContent(..)|ValueIdentity::MemoryValue(..))) {
                            let scalar=value.ty.components[*component].0;proof.mark_domain(target,IndexDomain::CommonRange(source,scalar,scalar));
                        }
                    }
                    if let Some(previous)=&mut result{bounds::Proof::join(previous,&proof,regs.len(),None);}else{result=Some(proof);}
                    }
                }
            }
        }
        result
    }
    pub(super) fn update_application_elements(&mut self,sources:&[ValueIdentity],resource:&view::Borrow,words:&[u32],update:ElementUpdate<'_>){
        let names=self.env.keys().cloned().collect::<Vec<_>>();
        let range=match &update{ElementUpdate::Element{range,..}=>*range,_=>None};
        let one=(range==Some((0,0))).then(||self.constant(Scalar::U32,1));
        let selected=range.map(|(a,b)|bounds::Condition::paths(bounds::Clause::values([(Scalar::U32,ValueIdentity::ElementIndex(0),&bounds::Proof::unsigned(a,b))]),bounds::Clause::default()));
        for name in names{
            let mut binding=self.env[&name].clone();
            each_binding_data(&mut binding,&mut |_,_,data|{
                let scopes=self.application_element_scopes(sources,resource,data);let unique=scopes.len()==1;
                for(scope,complete_scope)in scopes {
                let mut complete_scope=bounds::Condition::constant(complete_scope);let mut data=&mut *data;
                for fields in scope{if let Some(one)=&one{complete_scope=complete_scope.logical(bounds::Condition::comparison(Comparison::Equal,&data.length,one),false);}data=model_data_mut(&mut data.elements,&fields).unwrap();}
                let components=words.iter().filter_map(|word|data.ty.components.iter().position(|(_,at)|at==word)).collect::<Vec<_>>();
                if components.is_empty(){continue;}
                let old=self.returned_storage(&data.elements);let mut updated=old.clone();updated.constant=None;
                let mut bindings=element_scope_bindings(&self.element_owner_bindings(data),old.regs.len(),false);
                for component in 0..old.regs.len(){if !components.contains(&component){for id in old.component_origins(component){
                    if !matches!(id,ValueIdentity::ElementComponent(at) if components.contains(&at)){
                        bindings.entry(id).or_insert_with(||bounds::Proof::default().with_identity(id));
                    }
                }}}
                updated.proof=bounds::Proof::capture(&old).at_call(&bindings);
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
                }else{bounds::Proof::default()};
                updated.proof.assign(&components,old.regs.len(),written);
                if let ElementUpdate::WholeFacts(proof)=&update {
                    updated.proof.assign_elements(&components,old.regs.len(),proof);
                }
                let complete=match &update {
                    ElementUpdate::WholeFacts(_)=>bounds::Condition::constant(unique).logical(complete_scope,false),
                    _=>match &one{Some(one)=>bounds::Condition::constant(unique).logical(complete_scope,false).logical(bounds::Condition::comparison(Comparison::Equal,&data.length,one),false),None=>bounds::Condition::constant(false)},
                };
                let truth=complete.truth_in(&self.path);
                if truth!=Some(true){
                    let mut common=updated.proof.clone();
                    let mut proof=updated.proof.project(&components,old.regs.len());
                    proof.join_condition(&old.proof.project(&components,old.regs.len()),components.len(),selected.as_ref());
                    common.assign(&components,old.regs.len(),proof);
                    if truth==Some(false){updated.proof=common;}else{updated.proof.join_condition(&common,old.regs.len(),Some(&complete));}
                }
                updated.proof.origins.resize_with(updated.regs.len(),Vec::new);
                for(component,origins)in updated.proof.origins.iter_mut().enumerate(){
                    let identity=ValueIdentity::ElementComponent(updated.regs[component]);
                    if !origins.contains(&identity){origins.push(identity);}
                }
                updated.proof.retain_view_borrows(&updated.ty);
                match data.elements.as_mut(){
                    returns::Returned::Value(value)=>**value=updated.clone(),
                    returns::Returned::OwnedValue(value)=>value.value=updated.clone(),
                    returns::Returned::Dataset(_)=>{},
                    returns::Returned::Unit=>{},
                }
                for path in view::resource_paths(&updated.ty){
                    if let Some(inner)=model_data_mut(&mut data.elements,&path){
                        let handle=inner.root.regs[0];let length=inner.length.regs[0];
                        let borrow=inner.root.proof.view_borrow(0);
                        inner.root=self.value_component(&updated,handle);inner.length=self.value_component(&updated,length);
                        inner.root.proof=std::mem::take(&mut inner.root.proof).with_identity(ValueIdentity::ElementComponent(handle)).with_view_borrow(0,borrow);
                        if components.contains(&handle)||components.contains(&length){inner.elements=self.unknown_element_model(&inner.ty);}
                    }
                }
                }
            });
            self.env.insert(name,binding);
        }
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
    pub(super) fn apply_application_dataset_model(&mut self,reference:&DataReference,model:returns::Returned){
        let resource=view::resource_borrow(&reference.data);let names=self.env.keys().cloned().collect::<Vec<_>>();
        for name in names{
            let mut binding=self.env[&name].clone();
            each_binding_data(&mut binding,&mut |_,_,data|{
                let scopes=self.application_element_scopes(&reference.sources,&resource,data);let unique=scopes.len()==1;
                for(scope,mut complete)in scopes{
                    let count=scope.len()+1;
                    let qualifier=if unique&&complete{bounds::Condition::constant(true)}else{bounds::Condition::default()};
                    let selected=qualifier.logical(Self::element_selection(reference.data.place.as_ref(),1,scope.len()),false);
                    let mut data=&mut *data;
                    for fields in scope{complete&=data.length.minimum()==1&&data.length.maximum()==1;data=model_data_mut(&mut data.elements,&fields).unwrap();}
                    let mut fresh=self.capture_element_model(data,&reference.projection,model.clone());
                    if !unique||!complete{
                        let incoming=fresh.clone();let previous=self.projected_element_model(&data.elements,&reference.projection);
                        fresh.join(previous.clone(),true);
                        residency::select_elements(&mut fresh,&incoming,&previous,&selected,count);
                    }
                    self.replace_element_model(&mut data.elements,&reference.projection,*fresh);
                }
            });
            self.env.insert(name,binding);
        }
    }
}

// Each stored model is checked in its own existing element scope. Its local
// identities never become resources in an outer function's retained set.
impl Builder<'_> {
    pub(super) fn require_element_return_borrows(&mut self,model:&returns::Returned,owned:&view::Borrow,span:Span)->Result<()> {
        fn visit(builder:&mut Builder<'_>,model:&returns::Returned,owned:&view::Borrow,parents:&[usize],span:Span)->Result<()> {
            let value=builder.returned_storage(model);
            for path in view::resource_paths(&value.ty) {
                if let Some(data)=model_data(model,&path){check(builder,data,owned,parents,span)?;}
            }
            Ok(())
        }
        fn check(builder:&mut Builder<'_>,data:&Data,owned:&view::Borrow,parents:&[usize],span:Span)->Result<()> {
            if !data.ownership.may_own()||data.length.maximum()==0{return Ok(());}
            let mut scoped=owned.clone();scoped.union(&view::resource_borrow(data));
            let mut shifts=BTreeMap::new();
            for(depth,width)in parents.iter().enumerate(){for component in 0..*width {
                let id=if depth==0{ValueIdentity::ElementComponent(component)}else{ValueIdentity::ElementParent(depth-1,component)};
                shifts.insert(id,ValueIdentity::ElementParent(depth,component));
            }}
            scoped=scoped.output_bound(&shifts);
            for resource in view_contracts::resource_borrows(&data.elements){scoped.union(&resource);}
            if let Some(value)=data.elements.view_value() && value.ty.contains_borrows(){
                let mut proof=value.proof.borrow_state();proof.retain_view_borrows(&value.ty);
                builder.require_view_return_proof(&proof,scoped.clone(),span)?;
            }
            let mut nested=vec![data.elements.registers().len()];nested.extend_from_slice(parents);
            visit(builder,&data.elements,&scoped,&nested,span)
        }
        visit(self,model,owned,&[],span)
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
        for path in view::resource_paths(&value.ty){if let(Some(data),Some(source))=(model_data_mut(model,&path),model_data(original,&path)){
            if data.ownership.may_own(){data.root.proof=data.root.proof.clone().with_view_borrow(0,view::resource_borrow(source).output_bound(&names));}
            let nested=element_scope_bindings(bindings,source.elements.registers().len(),true);
            self.close_stored_resources(&mut data.elements,&source.elements,&nested);
        }}
    }
}
pub(super) fn element_parameter_sources(model:&returns::Returned)->BTreeSet<ValueIdentity>{
    fn visit(model:&returns::Returned,out:&mut BTreeSet<ValueIdentity>){
        if let Some(value)=model.view_value(){for source in value.proof.typed_view_borrow(&value.ty).owners().sources{
            if let view::Source::Parameter(id,_)=source{out.insert(id);}
        }}
        match model{
            returns::Returned::Dataset(data)=>visit(&data.elements,out),
            returns::Returned::OwnedValue(value)=>for payload in value.payloads.iter().flatten(){visit(payload,out);},
            returns::Returned::Value(_)|returns::Returned::Unit=>{},
        }
    }
    let mut out=BTreeSet::new();visit(model,&mut out);out
}


// 已闭合源码包直接消费本目录的类型／program编号；实际C ABI数组只借用到本次同步调用结束。
pub(super) fn lower_sdk_resources(body:&mut ValueCode,resources:&crate::application::Resources){
    fn value(reg:Reg,scalar:Scalar)->CallArgument{
        CallArgument::Value{origin:None,dataset_borrow:None,ty:Ty::scalar(scalar),values:vec![reg]}
    }
    fn emit(code:&mut Vec<Inst>,original:&Inst,kind:InstKind){
        code.push(Inst{kind,source_site:original.source_site.clone(),generated_origin:original.generated_origin.clone()});
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
                let program=constant(registers,&mut out,&inst,Scalar::U32,index as u64);
                if let InstKind::Call(call)=&mut inst.kind{
                    call.arguments=vec![call.arguments[0].clone(),value(program,Scalar::U32),data,words];
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
                    let field=constant(registers,&mut out,&inst,Scalar::U32,field.offset as u64);
                    let offset=registers.len();registers.push(Scalar::U32);
                    emit(&mut out,&inst,InstKind::Binary{dst:offset,op:ir::BinaryOp::IAdd,a:inputs[3],b:field});
                    let range_count=constant(registers,&mut out,&inst,Scalar::U64,values.len()as u64/2);
                    let limit_count=registers.len();registers.push(Scalar::U64);
                    emit(&mut out,&inst,InstKind::Cast{dst:limit_count,src:count});
                    if let InstKind::Call(mut call)=inst.kind.clone(){
                        call.arguments=vec![call.arguments[0].clone(),call.arguments[1].clone(),value(offset,Scalar::U32),ranges,value(range_count,Scalar::U64),bounds,value(limit_count,Scalar::U64)];
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
    block(&mut body.registers,&mut body.locals,&mut body.code,resources);
}
