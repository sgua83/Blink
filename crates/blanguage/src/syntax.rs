//! Read-only syntax views borrow the one B-owned store; builders publish no views.
#[path="syntax/wire.rs"] pub(crate) mod wire;
use crate::ast::Source;
use std::{ffi::c_void,fmt,marker::PhantomData,ptr::NonNull,sync::Arc};

#[derive(Debug,Clone,Copy)]
pub(crate) struct StorageFailure {pub kind:u32,pub native_code:u32}
impl fmt::Display for StorageFailure {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {
        write!(f,"B syntax storage error {}, native code {}",self.kind,self.native_code)
    }
}
impl std::error::Error for StorageFailure {}
type Result<T> = std::result::Result<T,StorageFailure>;
fn checked(error:wire::StorageError)->Result<()> {
    if error.kind==0 {Ok(())} else {Err(StorageFailure{kind:error.kind,native_code:error.native_code})}
}
struct Owner {raw:NonNull<c_void>,source:Option<Arc<str>>}
// An Owner enters Arc only when its unique builder is consumed. All later
// access is read-only; its source snapshot and B allocations outlive every view.
unsafe impl Send for Owner {}
unsafe impl Sync for Owner {}
impl Owner {
    fn view(&self)->wire::StorageView {unsafe{wire::view(self.raw.as_ptr())}}
    fn row<T:Copy>(&self,offset:u64)->T {unsafe {
        self.view().nodes.cast::<u8>().add(offset as usize).cast::<T>().read()
    }}
    fn array<T>(&self,range:wire::Slice)->&[T] {
        if range.count==0 {return &[];}
        unsafe {std::slice::from_raw_parts(self.view().nodes.cast::<u8>().add(range.offset as usize).cast::<T>(),range.count as usize)}
    }
    fn text(&self,value:wire::Text)->&str {
        if value.range.start==value.range.end {return "";}
        let view=self.view();
        let base=if value.kind==0 {view.source} else {view.text};
        // Source spans and generated names are UTF-8 supplied by their actual
        // producers; the private store retains bytes, not a second string tree.
        unsafe {std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            base.cast::<u8>().add(value.range.start as usize),(value.range.end-value.range.start)as usize))}
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        let result=unsafe{wire::destroy(self.raw.as_ptr())};
        if let Err(error)=checked(result) {panic!("{error}");}
    }
}
pub(crate) struct Builder {owner:Owner}
impl Builder {
    pub fn new(source:Option<Source>)->Result<Self> {
        let (data,length)=source.as_ref().map_or((std::ptr::null_mut(),0),|s|(s.text.as_ptr().cast_mut().cast(),s.text.len()as u64));
        let created=unsafe{wire::create(data,length)};checked(created.error)?;
        Ok(Self{owner:Owner{raw:unsafe{NonNull::new_unchecked(created.storage)},source:source.map(|source|source.text)}})
    }
    pub fn read_text(&self,text:wire::Text)->&str {self.owner.text(text)}
    pub fn finish<T>(self,offset:u64)->Node<T> {Node{owner:Arc::new(self.owner),offset,marker:PhantomData}}
    pub fn finish_list<T>(self,value:wire::Slice)->List<T> {List{owner:Arc::new(self.owner),value,marker:PhantomData}}
}

pub(crate) struct Node<T> {owner:Arc<Owner>,offset:u64,marker:PhantomData<T>}
impl<T> Node<T> {pub fn same_node(&self,other:&Self)->bool{Arc::ptr_eq(&self.owner,&other.owner)&&self.offset==other.offset}}
impl<T> Clone for Node<T> {fn clone(&self)->Self {Self{owner:self.owner.clone(),offset:self.offset,marker:PhantomData}}}
impl<T> fmt::Debug for Node<T> {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.debug_struct(std::any::type_name::<T>()).field("offset",&self.offset).finish()}
}
impl<T:Copy> Node<T> {
    fn row(&self)->T {self.owner.row(self.offset)}
    fn use_context(&self, bindings: &[wire::BoundName]) -> wire::UseContext {
        wire::UseContext { storage: self.owner.view(), bindings: bindings.as_ptr(), count: bindings.len() as u64 }
    }
    fn signature_node(&self) -> wire::SignatureNode {
        wire::SignatureNode { storage: self.owner.view(), offset: self.offset }
    }
    fn child<U>(&self,offset:u64)->Node<U> {Node{owner:self.owner.clone(),offset,marker:PhantomData}}
    fn list<U>(&self,value:wire::Slice)->List<U> {List{owner:self.owner.clone(),value,marker:PhantomData}}
}
pub(crate) struct List<T> {owner:Arc<Owner>,value:wire::Slice,marker:PhantomData<T>}
impl<T> Clone for List<T> {fn clone(&self)->Self {Self{owner:self.owner.clone(),value:self.value,marker:PhantomData}}}
impl<T> fmt::Debug for List<T> {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.debug_list().entries(self.iter()).finish()}}
impl<T> List<T> {
    pub fn len(&self)->usize {self.value.count as usize}
    pub fn is_empty(&self)->bool {self.value.count==0}
    pub fn get(&self,index:usize)->Option<Node<T>> {
        self.owner.array::<u64>(self.value).get(index).map(|&offset|Node{owner:self.owner.clone(),offset,marker:PhantomData})
    }
    pub fn at(&self,index:usize)->Node<T> {
        let offset=self.owner.array::<u64>(self.value)[index];
        Node{owner:self.owner.clone(),offset,marker:PhantomData}
    }
    pub fn slice<I:std::slice::SliceIndex<[u64],Output=[u64]>>(&self,index:I)->Self {
        let whole=self.owner.array::<u64>(self.value);
        let values=&whole[index];
        let offset=self.value.offset+(values.as_ptr()as usize-whole.as_ptr()as usize)as u64;
        Self{owner:self.owner.clone(),value:wire::Slice{offset,count:values.len()as u64},marker:PhantomData}
    }
    pub fn first(&self)->Option<Node<T>> {self.get(0)}
    pub fn last(&self)->Option<Node<T>> {self.iter().next_back()}
    pub fn iter(&self)->ListIter<T> {self.clone().into_iter()}
}
// An iterator owns the same immutable store, so a composed query can outlive
// the temporary list view without retaining an interior address.
pub(crate) struct ListIter<T> {list:List<T>,remaining:std::ops::Range<usize>}
impl<T> Clone for ListIter<T>{fn clone(&self)->Self{Self{list:self.list.clone(),remaining:self.remaining.clone()}}}
impl<T> Iterator for ListIter<T> {
    type Item=Node<T>;
    fn next(&mut self)->Option<Self::Item> {self.remaining.next().map(|index|self.list.at(index))}
    fn size_hint(&self)->(usize,Option<usize>){self.remaining.size_hint()}
}
impl<T> DoubleEndedIterator for ListIter<T> {
    fn next_back(&mut self)->Option<Self::Item> {self.remaining.next_back().map(|index|self.list.at(index))}
}
impl<T> ExactSizeIterator for ListIter<T> {}
impl<T> IntoIterator for List<T> {
    type Item=Node<T>;type IntoIter=ListIter<T>;
    fn into_iter(self)->Self::IntoIter {let end=self.len();ListIter{list:self,remaining:0..end}}
}
impl<T> IntoIterator for &List<T> {
    type Item=Node<T>;type IntoIter=ListIter<T>;
    fn into_iter(self)->Self::IntoIter {self.iter()}
}
macro_rules! text_field {($row:ident,$field:ident)=>{impl Node<wire::$row>{pub fn $field(&self)->&str{self.owner.text(self.row().$field)}}};}
macro_rules! child_field {($row:ident,$field:ident,$child:ident)=>{impl Node<wire::$row>{pub fn $field(&self)->Node<wire::$child>{self.child(self.row().$field.offset)}}};}
macro_rules! list_field {($row:ident,$field:ident,$child:ident)=>{impl Node<wire::$row>{pub fn $field(&self)->List<wire::$child>{self.list(self.row().$field)}}};}
macro_rules! optional_child {($row:ident,$field:ident,$child:ident)=>{impl Node<wire::$row>{pub fn $field(&self)->Option<Node<wire::$child>>{let v=self.row().$field;(v.present!=0).then(||self.child(v.value.offset))}}};}
macro_rules! optional_list {($row:ident,$field:ident,$child:ident)=>{impl Node<wire::$row>{pub fn $field(&self)->Option<List<wire::$child>>{let v=self.row().$field;(v.present!=0).then(||self.list(v.value))}}};}
macro_rules! span_field {($row:ident)=>{impl Node<wire::$row>{pub fn span(&self)->crate::ast::Span{let s=self.row().span;crate::ast::Span{start:s.start as usize,end:s.end as usize}}}};}
pub(crate) type Block=List<wire::StmtNode>;
pub(crate) type Expressions=List<wire::ExprNode>;
pub(crate) type MatchArms=List<wire::ArmNode>;
pub(crate) type Type=Node<wire::TypeNode>;
pub(crate) type Shape=Node<wire::ShapeNode>;
pub(crate) type Argument=Node<wire::ArgumentNode>;
pub(crate) type Condition=Node<wire::ConditionNode>;
pub(crate) type Predicate=Node<wire::PredicateNode>;
pub(crate) type Interval=Node<wire::IntervalNode>;
pub(crate) type Contract=Node<wire::ContractNode>;
pub(crate) type TypeFamily=Node<wire::TypeFamilyNode>;
pub(crate) type TypeParameter=Node<wire::TypeParameterNode>;
pub(crate) type ValueParameter=Node<wire::ValueParameterNode>;
pub(crate) type Field=Node<wire::FieldNode>;
pub(crate) type Variant=Node<wire::VariantNode>;
pub(crate) type Record=Node<wire::RecordNode>;
pub(crate) type Param=Node<wire::ParamNode>;
pub(crate) type External=Node<wire::ExternalNode>;
pub(crate) type Function=Node<wire::FunctionNode>;
pub(crate) type TypeSet=Node<wire::TypeSetNode>;
#[derive(Clone,Debug)]
pub(crate) struct Module {pub source:Source,pub root:Node<wire::ModuleNode>}
pub(crate) type Expr=Node<wire::ExprNode>;
pub(crate) type Stmt=Node<wire::StmtNode>;
pub(crate) type ParallelDomain=Node<wire::ParallelNode>;
pub(crate) type MatchArm=Node<wire::ArmNode>;
text_field!(TypeNode,name);
text_field!(TypeParameterNode,name);
text_field!(ValueParameterNode,name);
text_field!(FieldNode,name);
text_field!(VariantNode,name);
text_field!(RecordNode,name);
text_field!(ParamNode,name);
text_field!(ExternalNode,library);
text_field!(ExternalNode,symbol);
text_field!(FunctionNode,name);
text_field!(TypeSetNode,name);
text_field!(ParallelNode,dataset);
text_field!(ArmNode,variant);
child_field!(ContractNode,predicate,PredicateNode);
child_field!(IntervalNode,lower,ConditionNode);
child_field!(IntervalNode,upper,ConditionNode);
child_field!(FieldNode,ty,TypeNode);
child_field!(VariantNode,payload,TypeNode);
child_field!(VariantNode,discriminant,ExprNode);
child_field!(ParamNode,ty,TypeNode);
child_field!(FunctionNode,result,TypeNode);
list_field!(TypeNode,arguments,ArgumentNode);
list_field!(RecordNode,types,TypeParameterNode);
list_field!(RecordNode,values,ValueParameterNode);
list_field!(RecordNode,variants,VariantNode);
list_field!(RecordNode,fields,FieldNode);
list_field!(FunctionNode,types,TypeParameterNode);
list_field!(FunctionNode,values,ValueParameterNode);
list_field!(FunctionNode,params,ParamNode);
list_field!(FunctionNode,contracts,ContractNode);
list_field!(TypeSetNode,members,TypeNode);
list_field!(ArmNode,body,StmtNode);
optional_child!(TypeNode,count,ShapeNode);
optional_child!(RecordNode,stride,ShapeNode);
optional_child!(RecordNode,enumeration,TypeNode);

pub(crate) enum ContractMember {
    Missing,
    Variant,
    Field(Type),
}

impl Record {
    pub(crate) fn contract_member(&self, name: &str) -> ContractMember {
        // The declaration owner keeps both node and source bytes alive during the query.
        match unsafe { wire::record_member(self.signature_node(), name.as_ptr(), name.len() as u64) } {
            wire::RecordMember::Missing => ContractMember::Missing,
            wire::RecordMember::Variant => ContractMember::Variant,
            wire::RecordMember::Field(ty) => ContractMember::Field(self.child(ty.offset)),
        }
    }
}
optional_child!(FunctionNode,external,ExternalNode);
optional_child!(TypeParameterNode,family,TypeFamilyNode);
optional_child!(ParallelNode,count,ExprNode);
optional_child!(ParallelNode,selection,ExprNode);
span_field!(FieldNode);
span_field!(VariantNode);
span_field!(RecordNode);
span_field!(ParamNode);
span_field!(FunctionNode);
span_field!(TypeSetNode);
span_field!(ExprNode);
span_field!(StmtNode);
span_field!(ArmNode);
span_field!(ContractNode);

optional_list!(TypeNode,contracts,PredicateNode);
optional_list!(FunctionNode,body,StmtNode);
impl Type {pub fn canonical(&self)->bool{self.row().canonical!=0} pub fn dataset(&self)->bool{self.row().dataset!=0} pub fn view(&self)->bool{self.row().view!=0} pub fn tile(&self)->bool{self.row().tile!=0}}

impl Function {
    pub(crate) fn bound_uses(&self, bindings: &[wire::BoundName]) -> wire::UseError {
        unsafe { wire::check_function(self.use_context(bindings), wire::FunctionRef { offset: self.offset }) }
    }
}
impl Type {
    pub(crate) fn bound_uses(&self, bindings: &[wire::BoundName], span: wire::Span) -> wire::UseError {
        unsafe { wire::check_type(self.use_context(bindings), wire::TypeRef { offset: self.offset }, span) }
    }
}
impl Expr {
    pub(crate) fn bound_uses(&self, bindings: &[wire::BoundName]) -> wire::UseError {
        unsafe { wire::check_expression(self.use_context(bindings), wire::ExprRef { offset: self.offset }) }
    }
}
impl Type {
    pub(crate) fn mentions(&self, name: &str) -> bool {
        unsafe { wire::type_mentions(self.signature_node(), name.as_ptr(), name.len() as u64) }
    }
    pub(crate) fn names_shape(&self, name: &str) -> bool {
        unsafe { wire::resource_names_shape(self.signature_node(), name.as_ptr(), name.len() as u64) }
    }
}
impl Argument {
    pub(crate) fn names_value(&self, name: &str) -> bool {
        unsafe { wire::argument_names_const(self.signature_node(), name.as_ptr(), name.len() as u64) }
    }
}
impl Type {
    pub(crate) fn cfn_parameter(&self) -> (crate::ast::Access, Option<Type>) {
        // The sealed owner stays live for the query and the returned child.
        let parameter = unsafe { wire::cfn_parameter(self.signature_node()) };
        (parameter.access, (parameter.ty.present != 0).then(|| self.child(parameter.ty.value.offset)))
    }
}
impl ValueParameter {
    pub fn choices(&self) -> Option<&[u32]> {
        let choices = self.row().choices;
        (choices.present != 0).then(|| self.owner.array(choices.value))
    }

    pub(crate) fn accepts_const(&self, value: u32) -> bool {
        // B reads only the canonical choices in this live sealed store.
        unsafe { wire::accepts_const(self.owner.view().nodes, self.row().choices, value) }
    }
}
pub(crate) struct InstanceCombinations<'a> {
    parameters: &'a List<wire::ValueParameterNode>,
    type_sizes: &'a [u64],
    indices: Vec<u64>,
    values: Vec<u32>,
}

impl List<wire::ValueParameterNode> {
    pub fn instance_combinations<'a>(&'a self, type_sizes: &'a [u64]) -> InstanceCombinations<'a> {
        InstanceCombinations {
            parameters: self,
            type_sizes,
            indices: vec![0; type_sizes.len() + self.len()],
            values: vec![0; self.len()],
        }
    }
}

impl InstanceCombinations<'_> {
    pub fn try_for_each(&mut self, mut consume: impl FnMut(&[u64], &[u32]) -> crate::Result<()>) -> crate::Result<()> {
        let domains = wire::InstanceDomains {
            nodes: self.parameters.owner.view().nodes,
            parameters: self.parameters.value,
            type_sizes: self.type_sizes.as_ptr(),
            type_count: self.type_sizes.len() as u64,
        };
        let row = wire::InstanceRow { indices: self.indices.as_mut_ptr(), values: self.values.as_mut_ptr() };
        let mut first = 1;
        // The sealed syntax store and the domain lengths stay live. B
        // writes one index per axis and one value per const parameter.
        while unsafe { wire::advance_instance_domain(domains, row, first) } != 0 {
            consume(&self.indices[..self.type_sizes.len()], &self.values)?;
            first = 0;
        }
        Ok(())
    }
}

impl MatchArm {pub fn binding(&self)->Option<&str>{let value=self.row().binding;(value.present!=0).then(||self.owner.text(value.value))}}
impl Param {pub fn access(&self)->crate::ast::Access{self.row().access}}
impl Contract {pub fn kind(&self)->crate::ast::ContractKind{self.row().kind}}
impl Function {pub fn signature_end(&self)->usize{self.row().signature_end as usize} pub fn kind(&self)->crate::ast::FunctionKind{self.row().kind} pub fn target(&self)->crate::ast::Target{self.row().target} pub fn c_abi(&self)->bool{self.row().c_abi!=0}}
impl Function {
    pub(crate) fn same_signature(&self, other: &Self) -> bool {
        let mut left = Vec::<u64>::with_capacity(self.row().contracts.count as usize);
        let mut right = Vec::<u64>::with_capacity(other.row().contracts.count as usize);
        // B initializes and sorts these scratch offsets while both source owners stay live.
        unsafe {
            wire::same_signature(self.signature_node(), other.signature_node(), left.as_mut_ptr(), right.as_mut_ptr())
        }
    }
}
impl Module {
    pub fn name(&self)->&str {self.root.owner.text(self.root.row().name)}
    pub fn imports(&self)->impl ExactSizeIterator<Item=&str>{self.root.owner.array::<wire::Text>(self.root.row().imports).iter().map(|&text|self.root.owner.text(text))}
    pub fn records(&self)->List<wire::RecordNode>{self.root.list(self.root.row().records)}
    pub fn type_sets(&self)->List<wire::TypeSetNode>{self.root.list(self.root.row().type_sets)}
    pub fn functions(&self)->List<wire::FunctionNode>{self.root.list(self.root.row().functions)}
}
#[derive(PartialEq,Eq,PartialOrd,Ord)]
pub(crate) enum ArgumentKind {Type(Type),Value(Shape)}
impl Argument {pub fn kind(&self)->ArgumentKind{match self.row(){wire::ArgumentNode::Type(v)=>ArgumentKind::Type(self.child(v.offset)),wire::ArgumentNode::Value(v)=>ArgumentKind::Value(self.child(v.offset))}}}
#[derive(PartialEq,Eq,PartialOrd,Ord)]
pub(crate) enum ShapeKind<'a>{Literal(u32),Constant(&'a str),Expression(Expr)}
impl Shape {pub fn kind(&self)->ShapeKind<'_>{match self.row(){wire::ShapeNode::Literal(v)=>ShapeKind::Literal(v),wire::ShapeNode::Constant(v)=>ShapeKind::Constant(self.owner.text(v)),wire::ShapeNode::Expression(v)=>ShapeKind::Expression(self.child(v.offset))}}}
#[derive(PartialEq,Eq,PartialOrd,Ord)]
pub(crate) enum FamilyKind<'a>{Numeric,Floating,Integer,ScalarNumeric,ScalarInteger,Named(&'a str)}
impl TypeFamily {pub fn kind(&self)->FamilyKind<'_>{match self.row(){wire::TypeFamilyNode::Numeric=>FamilyKind::Numeric,wire::TypeFamilyNode::Floating=>FamilyKind::Floating,wire::TypeFamilyNode::Integer=>FamilyKind::Integer,wire::TypeFamilyNode::ScalarNumeric=>FamilyKind::ScalarNumeric,wire::TypeFamilyNode::ScalarInteger=>FamilyKind::ScalarInteger,wire::TypeFamilyNode::Named(v)=>FamilyKind::Named(self.owner.text(v))}}}

#[repr(C)]
#[derive(Clone, Copy)]
struct BuiltinFamilyName {
    bytes: [u8; 6],
    length: u32,
}

pub(crate) struct BuiltinFamilyNames {
    names: [std::mem::MaybeUninit<BuiltinFamilyName>; 19],
    len: usize,
}

unsafe extern "C" {
    #[link_name = "bootstrap.type_families::builtin_members"]
    fn builtin_family_members(family: *const wire::TypeFamilyNode, output: *mut BuiltinFamilyName) -> u32;
}

impl TypeFamily {
    pub(crate) fn builtin_names(&self) -> BuiltinFamilyNames {
        let mut names = [std::mem::MaybeUninit::uninit(); 19];
        // B writes one row per selected name, with at most 19 rows in total.
        let len = unsafe { builtin_family_members(&self.row(), names.as_mut_ptr().cast()) } as usize;
        BuiltinFamilyNames { names, len }
    }
}

impl BuiltinFamilyNames {
    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> {
        self.names[..self.len].iter().map(|name| {
            // The B table initializes the selected rows with ASCII spellings.
            let name = unsafe { name.assume_init_ref() };
            unsafe { std::str::from_utf8_unchecked(&name.bytes[..name.length as usize]) }
        })
    }
}
#[derive(PartialEq,Eq,PartialOrd,Ord)]
pub(crate) enum ExprKind<'a>{
    Number(&'a str),StaticU32(u32),Bool(bool),Name(&'a str),
    Member(Expr,&'a str),Index(Expr,Expr),Call(&'a str,List<wire::ExprNode>),
    Invoke(Expr,List<wire::ExprNode>),Construct(Type,List<wire::ExprNode>),
    Specialize(&'a str,List<wire::ArgumentNode>,List<wire::ExprNode>),
    FunctionRef(&'a str,List<wire::ArgumentNode>),
    Cooperate(&'a str,Option<List<wire::ArgumentNode>>,List<wire::ExprNode>),
    Target(crate::ast::Target,Expr),Unary(&'a str,Expr),Binary(&'a str,Expr,Expr),Select(Expr,Expr,Expr)
}
impl Expr {pub fn kind(&self)->ExprKind<'_>{
    use wire::ExprKind as W;
    match self.row().kind {
        W::Number(v)=>ExprKind::Number(self.owner.text(v)),W::StaticU32(v)=>ExprKind::StaticU32(v),
        W::Bool(v)=>ExprKind::Bool(v!=0),W::Name(v)=>ExprKind::Name(self.owner.text(v)),
        W::Member(v)=>ExprKind::Member(self.child(v.base.offset),self.owner.text(v.field)),
        W::Index(v)=>ExprKind::Index(self.child(v.base.offset),self.child(v.index.offset)),
        W::Call(v)=>ExprKind::Call(self.owner.text(v.name),self.list(v.arguments)),
        W::Invoke(v)=>ExprKind::Invoke(self.child(v.callee.offset),self.list(v.arguments)),
        W::Construct(v)=>ExprKind::Construct(self.child(v.ty.offset),self.list(v.arguments)),
        W::Specialize(v)=>ExprKind::Specialize(self.owner.text(v.name),self.list(v.types),self.list(v.arguments)),
        W::FunctionRef(v)=>ExprKind::FunctionRef(self.owner.text(v.name),self.list(v.types)),
        W::Cooperate(v)=>ExprKind::Cooperate(self.owner.text(v.name),(v.types.present!=0).then(||self.list(v.types.value)),self.list(v.arguments)),
        W::Target(v)=>ExprKind::Target(v.target,self.child(v.value.offset)),
        W::Unary(v)=>ExprKind::Unary(self.owner.text(v.operation),self.child(v.value.offset)),
        W::Binary(v)=>ExprKind::Binary(self.owner.text(v.operation),self.child(v.left.offset),self.child(v.right.offset)),
        W::Select(v)=>ExprKind::Select(self.child(v.condition.offset),self.child(v.yes.offset),self.child(v.no.offset))
    }
}}
pub(crate) enum ConditionKind<'a>{Name(&'a str),Number(&'a str),Bool(bool),Member(Condition,&'a str),Unary(&'a str,Condition),Binary(&'a str,Condition,Condition),StaticU32(u32)}
impl Condition {pub fn kind(&self)->ConditionKind<'_>{match self.row(){
    wire::ConditionNode::Name(v)=>ConditionKind::Name(self.owner.text(v)),wire::ConditionNode::Number(v)=>ConditionKind::Number(self.owner.text(v)),
    wire::ConditionNode::Bool(v)=>ConditionKind::Bool(v!=0),wire::ConditionNode::StaticU32(v)=>ConditionKind::StaticU32(v),
    wire::ConditionNode::Member(v)=>ConditionKind::Member(self.child(v.base.offset),self.owner.text(v.field)),
    wire::ConditionNode::Unary(v)=>ConditionKind::Unary(self.owner.text(v.operation),self.child(v.value.offset)),
    wire::ConditionNode::Binary(v)=>ConditionKind::Binary(self.owner.text(v.operation),self.child(v.left.offset),self.child(v.right.offset))
}}}
pub(crate) enum PredicateKind<'a>{
    Prefix{data:&'a str,count:&'a str,field:&'a str},Nonzero(&'a str),Nonempty(&'a str),Contiguous(&'a str),
    Memory(Option<crate::ast::Access>),Index{value:&'a str,resource:&'a str},Range{value:&'a str,intervals:List<wire::IntervalNode>},
    SameLength{left:&'a str,right:&'a str},Initialized{value:&'a str,condition:Condition},Cleanup{value:&'a str,condition:Condition}
}
impl Predicate {pub fn kind(&self)->PredicateKind<'_>{
    use wire::PredicateNode as W;
    match self.row(){
        W::Prefix(v)=>PredicateKind::Prefix{data:self.owner.text(v.data),count:self.owner.text(v.count),field:self.owner.text(v.field)},
        W::Nonzero(v)=>PredicateKind::Nonzero(self.owner.text(v)),W::Nonempty(v)=>PredicateKind::Nonempty(self.owner.text(v)),
        W::Contiguous(v)=>PredicateKind::Contiguous(self.owner.text(v)),W::Memory(v)=>PredicateKind::Memory((v.present!=0).then_some(v.value)),
        W::Index(v)=>PredicateKind::Index{value:self.owner.text(v.value),resource:self.owner.text(v.resource)},
        W::Range(v)=>PredicateKind::Range{value:self.owner.text(v.value),intervals:self.list(v.intervals)},
        W::SameLength(v)=>PredicateKind::SameLength{left:self.owner.text(v.left),right:self.owner.text(v.right)},
        W::Initialized(v)=>PredicateKind::Initialized{value:self.owner.text(v.value),condition:self.child(v.condition.offset)},
        W::Cleanup(v)=>PredicateKind::Cleanup{value:self.owner.text(v.value),condition:self.child(v.condition.offset)}
    }
}}
pub(crate) enum StmtKind<'a>{
    Declare(Type,&'a str),Let(Option<Type>,&'a str,Expr),Assign(Expr,&'a str,Expr),Expr(Expr),
    If(Expr,List<wire::StmtNode>,List<wire::StmtNode>),Match(Expr,List<wire::ArmNode>),
    While(Expr,List<wire::StmtNode>),For(&'a str,Expr,Option<Expr>,List<wire::StmtNode>),
    Parallel(&'a str,ParallelDomain,List<wire::StmtNode>),Return(Option<Expr>),
    Break,Continue,Phase,Seek(&'a str,Expr)
}
impl Stmt {pub fn kind(&self)->StmtKind<'_>{
    use wire::StmtKind as W;
    match self.row().kind {
        W::Declare(v)=>StmtKind::Declare(self.child(v.ty.offset),self.owner.text(v.name)),
        W::Let(v)=>StmtKind::Let((v.annotation.present!=0).then(||self.child(v.annotation.value.offset)),self.owner.text(v.name),self.child(v.value.offset)),
        W::Assign(v)=>StmtKind::Assign(self.child(v.target.offset),self.owner.text(v.operation),self.child(v.value.offset)),
        W::Expr(v)=>StmtKind::Expr(self.child(v.offset)),
        W::If(v)=>StmtKind::If(self.child(v.condition.offset),self.list(v.yes),self.list(v.no)),
        W::Match(v)=>StmtKind::Match(self.child(v.value.offset),self.list(v.arms)),
        W::While(v)=>StmtKind::While(self.child(v.condition.offset),self.list(v.body)),
        W::For(v)=>StmtKind::For(self.owner.text(v.name),self.child(v.start.offset),(v.end.present!=0).then(||self.child(v.end.value.offset)),self.list(v.body)),
        W::Parallel(v)=>StmtKind::Parallel(self.owner.text(v.name),self.child(v.domain.offset),self.list(v.body)),
        W::Return(v)=>StmtKind::Return((v.present!=0).then(||self.child(v.value.offset))),
        W::Break=>StmtKind::Break,W::Continue=>StmtKind::Continue,W::Phase=>StmtKind::Phase,
        W::Seek(v)=>StmtKind::Seek(self.owner.text(v.name),self.child(v.value.offset))
    }
}}

// Structural identity follows the language value, never the storage offset.
// Expr span is provenance and deliberately does not participate in comparison.
trait Structural:Copy {
    fn compare(left:&Node<Self>,right:&Node<Self>)->std::cmp::Ordering;
}
impl<T:Structural> PartialEq for Node<T> {fn eq(&self,other:&Self)->bool{self.cmp(other).is_eq()}}
impl<T:Structural> Eq for Node<T> {}
impl<T:Structural> PartialOrd for Node<T> {fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl<T:Structural> Ord for Node<T> {fn cmp(&self,other:&Self)->std::cmp::Ordering{T::compare(self,other)}}
impl<T:Structural> PartialEq for List<T> {fn eq(&self,other:&Self)->bool{self.iter().eq(other.iter())}}
impl<T:Structural> Eq for List<T> {}
impl<T:Structural> PartialOrd for List<T> {fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl<T:Structural> Ord for List<T> {fn cmp(&self,other:&Self)->std::cmp::Ordering{self.iter().cmp(other.iter())}}
impl Structural for wire::TypeNode {
    fn compare(a:&Type,b:&Type)->std::cmp::Ordering {
        a.name().cmp(b.name()).then_with(||a.canonical().cmp(&b.canonical()))
            .then_with(||a.arguments().cmp(&b.arguments()))
            .then_with(||a.dataset().cmp(&b.dataset())).then_with(||a.view().cmp(&b.view())).then_with(||a.tile().cmp(&b.tile()))
            .then_with(||a.count().cmp(&b.count())).then_with(||a.contracts().cmp(&b.contracts()))
    }
}
impl Structural for wire::ShapeNode {fn compare(a:&Shape,b:&Shape)->std::cmp::Ordering{a.kind().cmp(&b.kind())}}
impl Structural for wire::ArgumentNode {fn compare(a:&Argument,b:&Argument)->std::cmp::Ordering{a.kind().cmp(&b.kind())}}
impl Structural for wire::ExprNode {fn compare(a:&Expr,b:&Expr)->std::cmp::Ordering{a.kind().cmp(&b.kind())}}
impl Structural for wire::ConditionNode {
    fn compare(a: &Condition, b: &Condition) -> std::cmp::Ordering {
        (unsafe { wire::compare_condition(a.signature_node(), b.signature_node()) }).cmp(&0)
    }
}
impl Structural for wire::PredicateNode {
    fn compare(a: &Predicate, b: &Predicate) -> std::cmp::Ordering {
        (unsafe { wire::compare_predicate(a.signature_node(), b.signature_node()) }).cmp(&0)
    }
}
impl Structural for wire::TypeFamilyNode {fn compare(a:&TypeFamily,b:&TypeFamily)->std::cmp::Ordering{a.kind().cmp(&b.kind())}}
impl Structural for wire::TypeParameterNode {
    fn compare(a:&TypeParameter,b:&TypeParameter)->std::cmp::Ordering{a.name().cmp(b.name()).then_with(||a.family().cmp(&b.family()))}
}
impl Structural for wire::ValueParameterNode {
    fn compare(a:&ValueParameter,b:&ValueParameter)->std::cmp::Ordering{a.name().cmp(b.name()).then_with(||a.choices().cmp(&b.choices()))}
}
impl Structural for wire::ExternalNode {
    fn compare(a:&External,b:&External)->std::cmp::Ordering{a.library().cmp(b.library()).then_with(||a.symbol().cmp(b.symbol()))}
}

impl Builder {pub(crate) fn raw_for_parser(&self)->*mut std::ffi::c_void {self.owner.raw.as_ptr()}}


